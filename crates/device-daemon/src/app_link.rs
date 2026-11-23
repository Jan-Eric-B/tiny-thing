//! The local socket to the Device app.

use crate::{StateView, newest_connection};
use protocol::local::{AppMessage, DaemonMessage};
use protocol::{FrameError, InputAction, MessageReader, MessageWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;

#[derive(Debug, thiserror::Error)]
enum AppLinkError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("the Device daemon's state is gone")]
    StateClosed,
}

/// Serves the Device app for as long as it runs; the newest connection wins.
/// Input actions from the Device app go to `inputs`.
pub(crate) async fn serve(
    listener: TcpListener,
    state: StateView,
    inputs: broadcast::Sender<InputAction>,
) {
    newest_connection::serve(listener, "Device app", |stream| {
        handle(stream, state.clone(), inputs.clone())
    })
    .await;
}

async fn handle(
    stream: TcpStream,
    state: StateView,
    inputs: broadcast::Sender<InputAction>,
) -> Result<(), AppLinkError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    tokio::select! {
        result = send_to_app(&mut writer, state) => result,
        result = receive_from_app(&mut reader, &inputs) => result,
    }
}

/// Sends the current state at once, then every change.
async fn send_to_app(
    writer: &mut MessageWriter<OwnedWriteHalf, DaemonMessage>,
    mut state: StateView,
) -> Result<(), AppLinkError> {
    state.now_playing.mark_changed();
    state.volume.mark_changed();
    loop {
        let message = tokio::select! {
            changed = state.now_playing.changed() => {
                changed.map_err(|_| AppLinkError::StateClosed)?;
                DaemonMessage::NowPlaying(state.now_playing.borrow_and_update().clone())
            }
            changed = state.volume.changed() => {
                changed.map_err(|_| AppLinkError::StateClosed)?;
                DaemonMessage::Volume(*state.volume.borrow_and_update())
            }
        };
        writer.send(&message).await?;
    }
}

/// Passes the Device app's input actions on to the Tray app link.
async fn receive_from_app(
    reader: &mut MessageReader<OwnedReadHalf, AppMessage>,
    inputs: &broadcast::Sender<InputAction>,
) -> Result<(), AppLinkError> {
    while let Some(message) = reader.recv().await? {
        match message {
            AppMessage::Input(action) => {
                tracing::debug!(?action, "input action from the Device app");
                if inputs.send(action).is_err() {
                    tracing::info!(?action, "no Tray app connected; input action dropped");
                }
            }
        }
    }
    Ok(())
}
