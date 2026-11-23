//! The local socket to the Device app.

use crate::newest_connection;
use protocol::local::{AppMessage, DaemonMessage};
use protocol::{FrameError, MessageReader, MessageWriter, NowPlaying};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
enum AppLinkError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("the Device daemon's state is gone")]
    StateClosed,
}

/// Serves the Device app for as long as it runs; the newest connection wins.
pub(crate) async fn serve(listener: TcpListener, now_playing: watch::Receiver<Option<NowPlaying>>) {
    newest_connection::serve(listener, "Device app", |stream| {
        handle(stream, now_playing.clone())
    })
    .await;
}

async fn handle(
    stream: TcpStream,
    now_playing: watch::Receiver<Option<NowPlaying>>,
) -> Result<(), AppLinkError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    tokio::select! {
        result = send_to_app(&mut writer, now_playing) => result,
        result = receive_from_app(&mut reader) => result,
    }
}

/// Sends the current state at once, then every change.
async fn send_to_app(
    writer: &mut MessageWriter<OwnedWriteHalf, DaemonMessage>,
    mut now_playing: watch::Receiver<Option<NowPlaying>>,
) -> Result<(), AppLinkError> {
    now_playing.mark_changed();
    loop {
        if now_playing.changed().await.is_err() {
            return Err(AppLinkError::StateClosed);
        }
        let message = DaemonMessage::NowPlaying(now_playing.borrow_and_update().clone());
        writer.send(&message).await?;
    }
}

/// Handles what the Device app sends. That is nothing yet: `AppMessage` gains
/// variants with the input actions.
#[expect(
    clippy::never_loop,
    reason = "AppMessage has no variants yet; this loops once it has"
)]
async fn receive_from_app(
    reader: &mut MessageReader<OwnedReadHalf, AppMessage>,
) -> Result<(), AppLinkError> {
    while let Some(message) = reader.recv().await? {
        match message {}
    }
    Ok(())
}
