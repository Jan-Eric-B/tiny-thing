//! The Device app's end of the local socket to the Device daemon.

use protocol::local::{AppMessage, DaemonMessage};
use protocol::{FrameError, MessageReader, MessageWriter};
use std::net::SocketAddr;
use std::ops::ControlFlow;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// How long to wait before connecting again when the Device daemon is not
/// (or no longer) there, for example while it starts.
pub const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Receives messages from the Device daemon at `address` and hands each to
/// `on_message`, connecting again whenever the local socket closes. Returns
/// when `on_message` breaks.
pub async fn run(
    address: SocketAddr,
    mut on_message: impl FnMut(DaemonMessage) -> ControlFlow<()>,
) {
    loop {
        match TcpStream::connect(address).await {
            Ok(stream) => {
                tracing::info!(%address, "connected to the Device daemon");
                match handle(stream, &mut on_message).await {
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

async fn handle(
    stream: TcpStream,
    on_message: &mut impl FnMut(DaemonMessage) -> ControlFlow<()>,
) -> Result<ControlFlow<()>, FrameError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    tokio::select! {
        result = send_to_daemon(&mut writer) => result,
        result = receive_from_daemon(&mut reader, on_message) => result,
    }
}

/// Sends what the Device app has to tell the Device daemon. That is nothing
/// yet: `AppMessage` gains variants with the input actions.
async fn send_to_daemon(
    _writer: &mut MessageWriter<OwnedWriteHalf, AppMessage>,
) -> Result<ControlFlow<()>, FrameError> {
    std::future::pending().await
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
