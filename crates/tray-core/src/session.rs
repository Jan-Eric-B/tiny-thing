//! One TinyThing session between the Tray app and the Device daemon: from the
//! Handshake until either side closes the connection.

use crate::TRAY_APP_VERSION;
use crate::artwork;
use crate::artwork_supply::ArtworkSupply;
use crate::media::{ControlError, MediaSource, NowPlaying, TransportControl};
use crate::volume::{Step, Volume, VolumeControl, VolumeError};
use protocol::{
    ArtworkId, DeviceMessage, FrameError, Handshake, HandshakeError, InputAction, MessageReader,
    MessageWriter, TrayMessage,
};
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

/// How many input actions from the Device may wait while a transport control
/// runs. More would only replay stale presses once a slow player answers.
const CONTROL_QUEUE: usize = 1;

/// How many Volume steps from the Device may wait while one is carried out.
/// Setting the Volume takes milliseconds, so this only fills up when the
/// operating system hangs; the steps beyond it are dropped.
const VOLUME_QUEUE: usize = 16;

/// How many requests for Artwork from the Device may wait while one is
/// answered. The Device asks at most once per Now Playing; requests beyond
/// this are dropped.
const ARTWORK_REQUEST_QUEUE: usize = 4;

/// Why no session could be started.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("cannot reach the Device daemon: {0}")]
    Io(#[from] io::Error),
    #[error("Handshake with the Device daemon failed: {0}")]
    Handshake(#[from] HandshakeError),
}

/// Why a running session ended.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The connection to the Device daemon is gone. Whether a lost connection
    /// shows up as a clean close, a reset or a broken pipe depends on the
    /// operating system and on timing (Windows resets a connection where
    /// Linux often closes it), so all of them end up here.
    #[error("the connection to the Device daemon is gone")]
    DeviceGone,
    /// The connection is still there, but a message could not be sent or the
    /// Device daemon sent one that cannot be read.
    #[error(transparent)]
    Frame(FrameError),
}

impl From<FrameError> for SessionError {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::Truncated => Self::DeviceGone,
            FrameError::Io(io_error) if is_connection_lost(&io_error) => Self::DeviceGone,
            other => Self::Frame(other),
        }
    }
}

fn is_connection_lost(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
    )
}

/// A connection to the Device daemon whose Handshake has completed.
#[derive(Debug)]
pub struct Session {
    device_handshake: Handshake,
    reader: MessageReader<OwnedReadHalf, DeviceMessage>,
    writer: MessageWriter<OwnedWriteHalf, TrayMessage>,
}

impl Session {
    /// Connects to the Device daemon at `device` and runs the Handshake.
    ///
    /// A protocol version mismatch is reported as
    /// [`HandshakeError::ProtocolMismatch`], a Device daemon that does not
    /// answer in time as [`HandshakeError::TimedOut`].
    pub async fn connect(device: SocketAddr) -> Result<Self, ConnectError> {
        let (read, write) = TcpStream::connect(device).await?.into_split();
        let mut reader = MessageReader::new(read);
        let mut writer = MessageWriter::new(write);
        let device_handshake = Handshake::current(TRAY_APP_VERSION)
            .exchange(&mut reader, &mut writer)
            .await?;
        tracing::info!(
            %device,
            device_version = %device_handshake.software_version,
            "Handshake with the Device daemon complete"
        );
        Ok(Self {
            device_handshake,
            reader,
            writer,
        })
    }

    /// The versions the Device daemon stated in the Handshake.
    pub fn device_handshake(&self) -> &Handshake {
        &self.device_handshake
    }

    /// Forwards Now Playing from `media` and the Volume from `volume` to the
    /// Device, and the Device's input actions to `media`'s transport controls
    /// and to `volume`, until the session ends.
    ///
    /// Ends with `Ok(())` when `media` stops, and with
    /// [`SessionError::DeviceGone`] when the connection to the Device daemon
    /// is lost. A transport control or Volume step that fails is logged and
    /// does not end the session: the next Now Playing or Volume shows the
    /// Device what actually happened.
    ///
    /// Transport controls run one after the other, apart from reading the
    /// Device, so a slow player does not hold up the Device's other messages.
    /// While one runs, one more input action may wait; further ones are
    /// dropped, since a press is meant for now, not for whenever the
    /// player is done. Volume steps run one after the other too, apart from
    /// the transport controls, so a slow player does not hold them up.
    pub async fn run<M: MediaSource, V: VolumeControl>(
        self,
        media: &M,
        volume: &V,
    ) -> Result<(), SessionError> {
        let Self {
            mut reader,
            mut writer,
            ..
        } = self;
        let (controls, pending_controls) = mpsc::channel(CONTROL_QUEUE);
        let (steps, pending_steps) = mpsc::channel(VOLUME_QUEUE);
        let (missing_artwork, wanted_artwork) = mpsc::channel(ARTWORK_REQUEST_QUEUE);
        let routes = Routes {
            controls,
            steps,
            missing_artwork,
        };
        let changes = Changes {
            now_playing: media.now_playing(),
            volume: volume.volume(),
            wanted_artwork,
        };
        tokio::select! {
            // In this order on every poll, so `carry_out` and `adjust` take an
            // action handed over by `receive_from_device` before that reads
            // on.
            biased;
            // The pending channels close only once `receive_from_device` has
            // dropped its senders, which means it has ended, and that ends
            // the session first.
            () = carry_out(media, pending_controls) => Err(SessionError::DeviceGone),
            () = adjust(volume, pending_steps) => Err(SessionError::DeviceGone),
            result = receive_from_device(&mut reader, routes) => result,
            result = send_to_device(&mut writer, changes) => result,
        }
    }
}

/// What [`send_to_device`] tells the Device about.
struct Changes {
    now_playing: watch::Receiver<Option<NowPlaying>>,
    volume: watch::Receiver<Option<Volume>>,
    /// The Artwork the Device asked for.
    wanted_artwork: mpsc::Receiver<ArtworkId>,
}

/// Sends the current Now Playing and Volume at once, then every change, and
/// the Artwork of Now Playing when the Device asks for it.
///
/// Artwork is prepared off this loop ([`ArtworkSupply`]), so Now Playing and
/// the Volume never wait for it.
async fn send_to_device(
    writer: &mut MessageWriter<OwnedWriteHalf, TrayMessage>,
    changes: Changes,
) -> Result<(), SessionError> {
    let Changes {
        mut now_playing,
        mut volume,
        mut wanted_artwork,
    } = changes;
    now_playing.mark_changed();
    volume.mark_changed();
    let mut artwork = ArtworkSupply::new(artwork::prepare);
    loop {
        let message = tokio::select! {
            changed = now_playing.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let current = now_playing.borrow_and_update();
                artwork.now_playing(current.as_ref().and_then(|current| current.artwork.as_ref()));
                TrayMessage::NowPlaying(current.as_ref().map(protocol::NowPlaying::from))
            }
            // Never fails: the channel stays open as long as the
            // `VolumeControl` lives, which outlives the session.
            Ok(()) = volume.changed() => {
                TrayMessage::Volume(volume.borrow_and_update().map(protocol::Volume::from))
            }
            // Closes only when `receive_from_device` has ended, which ends
            // the session.
            Some(id) = wanted_artwork.recv() => match artwork.wanted(id) {
                Some(answer) => answer,
                None => continue,
            },
            finished = artwork.finished() => match finished {
                Some(answer) => answer,
                None => continue,
            },
        };
        writer.send(&message).await?;
    }
}

/// Where [`receive_from_device`] hands what the Device asks for over to.
struct Routes {
    controls: mpsc::Sender<TransportControl>,
    steps: mpsc::Sender<Step>,
    missing_artwork: mpsc::Sender<ArtworkId>,
}

/// What an input action from the Device acts on.
enum Route {
    Control(TransportControl),
    Volume(Step),
}

fn route(action: InputAction) -> Route {
    match action {
        InputAction::PlayPause => Route::Control(TransportControl::TogglePlayPause),
        InputAction::Previous => Route::Control(TransportControl::Previous),
        InputAction::Next => Route::Control(TransportControl::Next),
        InputAction::VolumeUp => Route::Volume(Step::Up),
        InputAction::VolumeDown => Route::Volume(Step::Down),
    }
}

/// Passes each input action from the Device on to [`carry_out`] or
/// [`adjust`], dropping it when their queue is full, and each request for
/// Artwork on to [`send_to_device`].
async fn receive_from_device(
    reader: &mut MessageReader<OwnedReadHalf, DeviceMessage>,
    routes: Routes,
) -> Result<(), SessionError> {
    while let Some(message) = reader.recv().await? {
        match message {
            DeviceMessage::Input(action) => {
                tracing::debug!(?action, "input action from the Device");
                let handed_over = match route(action) {
                    Route::Control(control) => routes.controls.try_send(control).is_ok(),
                    Route::Volume(step) => routes.steps.try_send(step).is_ok(),
                };
                if !handed_over {
                    tracing::debug!(?action, "still busy with earlier input actions; dropped");
                }
                // `carry_out` and `adjust` run in this task: give them the
                // chance (they are polled first) to take the action before
                // the next message is read, so presses that arrive in one read
                // count as one after the other, not as presses during a
                // control still running.
                tokio::task::yield_now().await;
            }
            DeviceMessage::MissingArtwork(id) => {
                tracing::debug!(?id, "the Device asks for Artwork");
                // Never waited on, so reading from the Device never waits
                // for writing to it.
                match routes.missing_artwork.try_send(id) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        tracing::debug!(?id, "too many requests for Artwork waiting; dropped");
                    }
                    // `send_to_device` has ended, which ends the session.
                    Err(TrySendError::Closed(_)) => {}
                }
            }
        }
    }
    Err(SessionError::DeviceGone)
}

/// Carries out the transport controls in `pending` on `media`, one after the
/// other, until `pending` closes.
async fn carry_out<M: MediaSource>(media: &M, mut pending: mpsc::Receiver<TransportControl>) {
    while let Some(control) = pending.recv().await {
        match media.control(control).await {
            Ok(()) => {}
            // Every press while nothing plays ends up here; nothing is wrong.
            Err(error @ ControlError::NoMediaSession) => {
                tracing::debug!(?control, %error, "transport control from the Device not carried out");
            }
            Err(error) => {
                tracing::warn!(?control, %error, "cannot carry out a transport control from the Device");
            }
        }
    }
}

/// Carries out the Volume steps in `pending` on `volume`, one after the
/// other, until `pending` closes.
async fn adjust<V: VolumeControl>(volume: &V, mut pending: mpsc::Receiver<Step>) {
    while let Some(step) = pending.recv().await {
        match volume.step(step).await {
            Ok(()) => {}
            // Every step while the computer has no output device ends up
            // here; nothing is wrong.
            Err(error @ VolumeError::NoOutputDevice) => {
                tracing::debug!(?step, %error, "Volume step from the Device not carried out");
            }
            Err(error) => {
                tracing::warn!(?step, %error, "cannot carry out a Volume step from the Device");
            }
        }
    }
}
