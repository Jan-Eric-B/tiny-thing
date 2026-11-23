//! The connection to the Tray app.

use crate::{DEVICE_DAEMON_VERSION, newest_connection};
use protocol::{
    DeviceMessage, FrameError, Handshake, HandshakeError, MessageReader, MessageWriter, NowPlaying,
    TrayMessage,
};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
enum TrayLinkError {
    #[error("Handshake failed: {0}")]
    Handshake(#[from] HandshakeError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// Serves the Tray app for as long as it runs; the newest connection wins.
pub(crate) async fn serve(listener: TcpListener, now_playing: watch::Sender<Option<NowPlaying>>) {
    newest_connection::serve(listener, "Tray app", |stream| {
        handle(stream, now_playing.clone())
    })
    .await;
}

async fn handle(
    stream: TcpStream,
    now_playing: watch::Sender<Option<NowPlaying>>,
) -> Result<(), TrayLinkError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    let tray = Handshake::current(DEVICE_DAEMON_VERSION)
        .exchange(&mut reader, &mut writer)
        .await?;
    tracing::info!(tray_version = %tray.software_version, "Handshake with the Tray app complete");

    tokio::select! {
        result = send_to_tray(&mut writer) => result,
        result = receive_from_tray(&mut reader, &now_playing) => result,
    }
}

/// Sends what the Device has to tell the Tray app. That is nothing yet:
/// `DeviceMessage` gains variants with the input actions.
async fn send_to_tray(
    _writer: &mut MessageWriter<OwnedWriteHalf, DeviceMessage>,
) -> Result<(), TrayLinkError> {
    std::future::pending().await
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
