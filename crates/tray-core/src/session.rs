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
use std::convert::Infallible;
use std::io;
use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc, watch};
use tokio::time::Instant;

/// How many Volume steps from the Device may wait while one is carried out.
/// Setting the Volume takes milliseconds, so this only fills up when the
/// operating system hangs; the steps beyond it are dropped.
const VOLUME_QUEUE: usize = 16;

/// How many requests for Artwork from the Device may wait while one is
/// answered. The Device asks at most once per Now Playing; requests beyond
/// this are dropped.
const ARTWORK_REQUEST_QUEUE: usize = 4;

/// How long nothing playing is held back after something played. Some
/// players clear the title for a moment during a song change; when the next
/// title follows within this time, the Device never shows the gap. Short
/// enough that stopping a player still shows on the Device promptly.
pub const NOTHING_PLAYING_HOLD: Duration = Duration::from_millis(500);

/// How far a reported position may be from where the Device has moved it on
/// by itself before it is sent again. Players report rounded positions, and
/// some report every few seconds while playing; within this the Device
/// already shows the right second.
const POSITION_TOLERANCE: Duration = Duration::from_secs(1);

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
    /// While one runs, one more may wait ([`WaitingControl`]); further ones
    /// are dropped, since a press is meant for now, not for whenever the
    /// player is done, except that a seek replaces a seek that waits: the
    /// last tap on the progress bar is where the user wants to be. Volume steps run one after the other too, apart from
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
        let waiting_control = WaitingControl::default();
        let (steps, pending_steps) = mpsc::channel(VOLUME_QUEUE);
        let (missing_artwork, wanted_artwork) = mpsc::channel(ARTWORK_REQUEST_QUEUE);
        let routes = Routes {
            controls: &waiting_control,
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
            // `carry_out` never ends. The pending steps close only once
            // `receive_from_device` has dropped its sender, which means it
            // has ended, and that ends the session first.
            never = carry_out(media, &waiting_control) => match never {},
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
/// the Artwork of Now Playing when the Device asks for it. Nothing playing
/// after something playing goes out only once it has lasted
/// [`NOTHING_PLAYING_HOLD`] ([`Told`]). A change of position alone goes out
/// only when it is more than [`POSITION_TOLERANCE`] from where the Device
/// has got to by itself ([`Sent`]).
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
    let mut told = Told::default();
    loop {
        let message = tokio::select! {
            changed = now_playing.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let current = now_playing.borrow_and_update();
                let now = Instant::now();
                let message = current.as_ref().map(|current| current.to_device(now));
                if !told.changed(message.as_ref(), now) {
                    continue;
                }
                // The Artwork follows what the Device is told, so while
                // nothing playing is held back, the Device still gets the
                // Artwork of what it shows.
                artwork.now_playing(current.as_ref().and_then(|current| current.artwork.as_ref()));
                TrayMessage::NowPlaying(message)
            }
            () = told.nothing_due() => {
                told = Told::Nothing;
                artwork.now_playing(None);
                TrayMessage::NowPlaying(None)
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

/// What the Device was last told about Now Playing, which decides whether a
/// change goes out at once. Nothing playing after something playing is held
/// back for [`NOTHING_PLAYING_HOLD`]; something playing goes out at once
/// unless the Device already shows it ([`Sent::agrees`]), and drops nothing
/// playing held back; nothing playing after nothing playing changes nothing.
#[derive(Debug, Default)]
enum Told {
    /// Nothing yet in this session: whatever plays goes out at once.
    #[default]
    NotYet,
    Something(Sent),
    /// Something, and nothing playing is held back until then.
    SomethingUntil(Sent, Instant),
    Nothing,
}

impl Told {
    /// Now Playing changed to `current` (`None` for nothing playing), as the
    /// Device would be told it at `now`. Returns whether it goes out now,
    /// and if so takes it as told.
    fn changed(&mut self, current: Option<&protocol::NowPlaying>, now: Instant) -> bool {
        let (next, goes_out) = match (std::mem::take(self), current) {
            (Self::Something(sent) | Self::SomethingUntil(sent, _), Some(current))
                if sent.agrees(current, now) =>
            {
                (Self::Something(sent), false)
            }
            (_, Some(current)) => (
                Self::Something(Sent {
                    message: current.clone(),
                    at: now,
                }),
                true,
            ),
            (Self::NotYet, None) => (Self::Nothing, true),
            (Self::Something(sent), None) => (
                Self::SomethingUntil(sent, now + NOTHING_PLAYING_HOLD),
                false,
            ),
            (held @ (Self::SomethingUntil(..) | Self::Nothing), None) => (held, false),
        };
        *self = next;
        goes_out
    }

    /// Finishes when nothing playing held back is due to go out; never while
    /// none is held back.
    async fn nothing_due(&self) {
        match self {
            Self::SomethingUntil(_, until) => tokio::time::sleep_until(*until).await,
            Self::NotYet | Self::Something(_) | Self::Nothing => std::future::pending().await,
        }
    }
}

/// Something playing that the Device was told, and when.
#[derive(Debug)]
struct Sent {
    message: protocol::NowPlaying,
    at: Instant,
}

impl Sent {
    /// Whether the Device, told this, already shows `current` at `now`: the
    /// same in everything but the position, and the position it has moved
    /// on to by itself within [`POSITION_TOLERANCE`] of `current`'s.
    fn agrees(&self, current: &protocol::NowPlaying, now: Instant) -> bool {
        let shown = self
            .message
            .clone()
            .aged(now.saturating_duration_since(self.at));
        let mut aligned = current.clone();
        if let (Some(aligned), Some(shown)) = (&mut aligned.progress, shown.progress)
            && aligned.position().abs_diff(shown.position()) <= POSITION_TOLERANCE
        {
            *aligned = aligned.with_position(shown.position());
        }
        aligned == shown
    }
}

/// Where [`receive_from_device`] hands what the Device asks for over to.
struct Routes<'a> {
    controls: &'a WaitingControl,
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
        InputAction::Seek(position) => Route::Control(TransportControl::Seek(position)),
    }
}

/// Passes each input action from the Device on to [`carry_out`] or
/// [`adjust`], dropping it when they have no room for it, and each request for
/// Artwork on to [`send_to_device`].
async fn receive_from_device(
    reader: &mut MessageReader<OwnedReadHalf, DeviceMessage>,
    routes: Routes<'_>,
) -> Result<(), SessionError> {
    while let Some(message) = reader.recv().await? {
        match message {
            DeviceMessage::Input(action) => {
                tracing::debug!(?action, "input action from the Device");
                let handed_over = match route(action) {
                    Route::Control(control) => routes.controls.offer(control),
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

/// The transport control that waits while another runs: at most one, as
/// more would only replay stale presses once a slow player answers. A seek
/// replaces a seek that waits, so of several taps on the progress bar the
/// last one is carried out.
#[derive(Debug, Default)]
struct WaitingControl {
    control: Mutex<Option<TransportControl>>,
    offered: Notify,
}

impl WaitingControl {
    /// Lets `control` wait if nothing waits, or if it and the one waiting
    /// are both seeks. Returns whether it waits now; otherwise it is dropped.
    fn offer(&self, control: TransportControl) -> bool {
        let mut waiting = self.lock();
        let taken = match (*waiting, control) {
            (None, _) | (Some(TransportControl::Seek(_)), TransportControl::Seek(_)) => {
                *waiting = Some(control);
                true
            }
            (Some(_), _) => false,
        };
        drop(waiting);
        if taken {
            self.offered.notify_one();
        }
        taken
    }

    /// The control that waits, once there is one. For one taker only.
    async fn next(&self) -> TransportControl {
        loop {
            if let Some(control) = self.lock().take() {
                return control;
            }
            // A control offered since the check above left a permit, so
            // this returns at once.
            self.offered.notified().await;
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<TransportControl>> {
        // An `Option` of a `Copy` value is consistent even if a holder
        // panicked.
        self.control.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Carries out the transport controls that wait in `waiting` on `media`, one
/// after the other. Never ends: the session ends when the Device does.
async fn carry_out<M: MediaSource>(media: &M, waiting: &WaitingControl) -> Infallible {
    loop {
        let control = waiting.next().await;
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
