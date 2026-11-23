//! The connection to the Tray app.

use crate::{DEVICE_DAEMON_VERSION, newest_connection};
use protocol::{
    DeviceMessage, FrameError, Handshake, HandshakeError, InputAction, MessageReader,
    MessageWriter, NowPlaying, TrayMessage,
};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};

#[derive(Debug, thiserror::Error)]
enum TrayLinkError {
    #[error("Handshake failed: {0}")]
    Handshake(#[from] HandshakeError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// Serves the Tray app for as long as it runs; the newest connection wins.
/// Input actions sent to `inputs` go to the Tray app.
pub(crate) async fn serve(
    listener: TcpListener,
    now_playing: watch::Sender<Option<NowPlaying>>,
    inputs: broadcast::Sender<InputAction>,
) {
    newest_connection::serve(listener, "Tray app", |stream| {
        // Subscribed on accept, so input actions count from the moment the
        // Tray app connects; earlier ones are not replayed.
        handle(stream, now_playing.clone(), inputs.subscribe())
    })
    .await;
}

async fn handle(
    stream: TcpStream,
    now_playing: watch::Sender<Option<NowPlaying>>,
    inputs: broadcast::Receiver<InputAction>,
) -> Result<(), TrayLinkError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    let tray = Handshake::current(DEVICE_DAEMON_VERSION)
        .exchange(&mut reader, &mut writer)
        .await?;
    tracing::info!(tray_version = %tray.software_version, "Handshake with the Tray app complete");

    tokio::select! {
        result = send_to_tray(&mut writer, inputs) => result,
        result = receive_from_tray(&mut reader, &now_playing) => result,
    }
}

/// Sends the input actions from the Device app to the Tray app, in order.
async fn send_to_tray(
    writer: &mut MessageWriter<OwnedWriteHalf, DeviceMessage>,
    mut inputs: broadcast::Receiver<InputAction>,
) -> Result<(), TrayLinkError> {
    loop {
        match inputs.recv().await {
            Ok(action) => writer.send(&DeviceMessage::Input(action)).await?,
            Err(RecvError::Lagged(dropped)) => {
                tracing::warn!(
                    dropped,
                    "input actions arrived faster than the Tray app took them"
                );
            }
            // Unreachable: `serve` owns a sender and ends every connection it
            // accepted before it drops that sender, so the channel never
            // closes under a running connection.
            Err(RecvError::Closed) => return Ok(()),
        }
    }
}

async fn receive_from_tray(
    reader: &mut MessageReader<OwnedReadHalf, TrayMessage>,
    now_playing: &watch::Sender<Option<NowPlaying>>,
) -> Result<(), TrayLinkError> {
    while let Some(message) = reader.recv().await? {
        match message {
            TrayMessage::NowPlaying(current) => {
                now_playing.send_replace(current);
            }
        }
    }
    Ok(())
}
