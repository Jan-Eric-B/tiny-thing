//! The connection to the Tray app.

use crate::{Arrived, ArtworkNews, DEVICE_DAEMON_VERSION, State, newest_connection};
use protocol::{
    ArtworkId, DeviceMessage, FrameError, Handshake, HandshakeError, InputAction, MessageReader,
    MessageWriter, TrayMessage,
};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{broadcast, mpsc};

/// How many requests for Artwork may wait to go out to the Tray app. The
/// Device asks at most once per Now Playing, so this fills up only when the
/// Tray app sends Now Playing faster than it reads; requests beyond it are
/// dropped and made again when that Artwork plays next.
const ARTWORK_REQUEST_QUEUE: usize = 4;

#[derive(Debug, thiserror::Error)]
enum TrayLinkError {
    #[error("Handshake failed: {0}")]
    Handshake(#[from] HandshakeError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// Serves the Tray app for as long as it runs; the newest connection wins.
/// What the Tray app reports goes to `state`; input actions sent to `inputs`
/// go to the Tray app.
pub(crate) async fn serve(
    listener: TcpListener,
    state: State,
    inputs: broadcast::Sender<InputAction>,
) {
    newest_connection::serve(listener, "Tray app", |stream| {
        // Subscribed on accept, so input actions count from the moment the
        // Tray app connects; earlier ones are not replayed.
        handle(stream, state.clone(), inputs.subscribe())
    })
    .await;
}

async fn handle(
    stream: TcpStream,
    state: State,
    inputs: broadcast::Receiver<InputAction>,
) -> Result<(), TrayLinkError> {
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    let tray = Handshake::current(DEVICE_DAEMON_VERSION)
        .exchange(&mut reader, &mut writer)
        .await?;
    tracing::info!(tray_version = %tray.software_version, "Handshake with the Tray app complete");

    // Never waited on (`try_send`), so reading from the Tray app never waits
    // for writing to it: with both sides waiting to write, neither would
    // read.
    let (missing, missing_artwork) = mpsc::channel(ARTWORK_REQUEST_QUEUE);
    tokio::select! {
        result = send_to_tray(&mut writer, inputs, missing_artwork) => result,
        result = receive_from_tray(&mut reader, &state, missing) => result,
    }
}

/// Sends the input actions from the Device app and the requests for missing
/// Artwork to the Tray app, in order.
async fn send_to_tray(
    writer: &mut MessageWriter<OwnedWriteHalf, DeviceMessage>,
    mut inputs: broadcast::Receiver<InputAction>,
    mut missing_artwork: mpsc::Receiver<ArtworkId>,
) -> Result<(), TrayLinkError> {
    loop {
        let input = tokio::select! {
            input = inputs.recv() => input,
            // Ends only with `receive_from_tray`, which ends the connection.
            Some(id) = missing_artwork.recv() => {
                writer.send(&DeviceMessage::MissingArtwork(id)).await?;
                continue;
            }
        };
        match input {
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

/// Keeps what the Tray app reports in `state`. Artwork that Now Playing names
/// and the Device does not keep is asked for through `missing`, once.
///
/// An Artwork that breaks the size contract does not get past
/// [`MessageReader::recv`]: the message cannot be read, which ends the
/// connection with the reason logged, before anything is kept.
async fn receive_from_tray(
    reader: &mut MessageReader<OwnedReadHalf, TrayMessage>,
    state: &State,
    missing: mpsc::Sender<ArtworkId>,
) -> Result<(), TrayLinkError> {
    let mut asked_for: Option<ArtworkId> = None;
    // The Artwork the Tray app said it cannot make.
    let mut unavailable: Option<ArtworkId> = None;
    while let Some(message) = reader.recv().await? {
        match message {
            TrayMessage::NowPlaying(current) => {
                let id = current.as_ref().and_then(|now_playing| now_playing.artwork);
                state.now_playing.send_replace(current.map(Arrived::new));
                let Some(id) = id else { continue };
                let kept = state.artworks().get(id);
                match kept {
                    Some(artwork) => state.tell(ArtworkNews::Here(artwork)),
                    None if unavailable == Some(id) => state.tell(ArtworkNews::Unavailable(id)),
                    // Asked for already: it is on its way.
                    None if asked_for == Some(id) => {}
                    None => match missing.try_send(id) {
                        Ok(()) => {
                            tracing::debug!(?id, "asking the Tray app for Artwork");
                            asked_for = Some(id);
                        }
                        Err(TrySendError::Full(_)) => {
                            tracing::debug!(?id, "too many requests for Artwork waiting; dropped");
                        }
                        // `send_to_tray` has ended, which ends this
                        // connection too.
                        Err(TrySendError::Closed(_)) => {}
                    },
                }
            }
            TrayMessage::Artwork(artwork) => {
                tracing::debug!(id = ?artwork.id(), "Artwork from the Tray app");
                state.artworks().insert(artwork.clone());
                if names(state, artwork.id()) {
                    state.tell(ArtworkNews::Here(artwork));
                }
            }
            TrayMessage::ArtworkUnavailable(id) => {
                tracing::debug!(?id, "the Tray app cannot make the Artwork");
                unavailable = Some(id);
                if names(state, id) {
                    state.tell(ArtworkNews::Unavailable(id));
                }
            }
            TrayMessage::Volume(current) => {
                state.volume.send_replace(current);
            }
        }
    }
    Ok(())
}

/// Whether the current Now Playing names the Artwork `id`.
fn names(state: &State, id: ArtworkId) -> bool {
    state
        .now_playing
        .borrow()
        .as_ref()
        .and_then(|arrived| arrived.now_playing.artwork)
        == Some(id)
}
