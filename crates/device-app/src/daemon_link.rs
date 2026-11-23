//! The Device app's end of the local socket to the Device daemon.

use protocol::local::{AppMessage, DaemonMessage};
use protocol::{FrameError, InputAction, MessageReader, MessageWriter};
use std::net::SocketAddr;
use std::ops::ControlFlow;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;

/// How long to wait before connecting again when the Device daemon is not
/// (or no longer) there, for example while it starts.
pub const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Receives messages from the Device daemon at `address` and hands each to
/// `on_message`, and sends it every input action from `actions`, connecting
/// again whenever the local socket closes.
///
/// Input actions that pile up while there is no connection are dropped on
/// connecting: a press is meant for now, not for whenever the Device daemon
/// is back.
///
/// Returns when `on_message` breaks or `actions` closes, which both mean
/// that the UI has shut down.
pub async fn run(
    address: SocketAddr,
    mut actions: mpsc::Receiver<InputAction>,
    mut on_message: impl FnMut(DaemonMessage) -> ControlFlow<()>,
) {
    loop {
        match TcpStream::connect(address).await {
            Ok(stream) => {
                tracing::info!(%address, "connected to the Device daemon");
                drop_stale(&mut actions);
                match handle(stream, &mut actions, &mut on_message).await {
                    Ok(ControlFlow::Break(())) => return,
                    Ok(ControlFlow::Continue(())) => {
                        tracing::info!("the Device daemon closed the local socket");
                    }
                    Err(error) => {
                        tracing::warn!(%error, "local socket to the Device daemon failed")
                    }
                }
            }
            Err(error) => tracing::debug!(%address, %error, "Device daemon not reachable yet"),
        }
        tokio::time::sleep(RETRY_DELAY).await;
    }
}

fn drop_stale(actions: &mut mpsc::Receiver<InputAction>) {
    let mut dropped = 0_usize;
    while actions.try_recv().is_ok() {
        dropped += 1;
    }
    if dropped > 0 {
        tracing::info!(
            dropped,
            "input actions from while the Device daemon was away dropped"
        );
    }
}

async fn handle(
    stream: TcpStream,
    actions: &mut mpsc::Receiver<InputAction>,
    on_message: &mut impl FnMut(DaemonMessage) -> ControlFlow<()>,
) -> Result<ControlFlow<()>, FrameError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    tokio::select! {
        result = send_to_daemon(&mut writer, actions) => result,
        result = receive_from_daemon(&mut reader, on_message) => result,
    }
}

/// Sends every input action, in order, until `actions` closes (`Break`).
async fn send_to_daemon(
    writer: &mut MessageWriter<OwnedWriteHalf, AppMessage>,
    actions: &mut mpsc::Receiver<InputAction>,
) -> Result<ControlFlow<()>, FrameError> {
    while let Some(action) = actions.recv().await {
        writer.send(&AppMessage::Input(action)).await?;
    }
    Ok(ControlFlow::Break(()))
}

/// Hands every message to `on_message` until the local socket closes
/// (`Continue`) or `on_message` breaks (`Break`).
async fn receive_from_daemon(
    reader: &mut MessageReader<OwnedReadHalf, DaemonMessage>,
    on_message: &mut impl FnMut(DaemonMessage) -> ControlFlow<()>,
) -> Result<ControlFlow<()>, FrameError> {
    while let Some(message) = reader.recv().await? {
        if on_message(message).is_break() {
            return Ok(ControlFlow::Break(()));
        }
    }
    Ok(ControlFlow::Continue(()))
}
