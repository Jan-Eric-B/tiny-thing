//! Now Playing as the operating system reports it, and its transport
//! controls.

mod fake;

pub use fake::FakeMediaSource;

use tokio::sync::watch;

/// What the operating system reports as Now Playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playback: PlaybackState,
}

/// Whether Now Playing is playing or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaybackState {
    Playing,
    /// Anything that is not playing: paused, stopped, or between songs.
    Paused,
}

/// A transport control of the Media session that is Now Playing.
///
/// Volume is not one of them: it belongs to the computer, not to a Media
/// session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportControl {
    /// Play if paused, pause if playing.
    TogglePlayPause,
    /// Go to the previous song.
    Previous,
    /// Go to the next song.
    Next,
}

/// Why a transport control did not reach the Media session.
///
/// A failed control changes nothing: the Device shows what the next Now
/// Playing says.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("no Media session to control")]
    NoMediaSession,
    #[error("the Media session did not accept the command")]
    Refused,
    #[error("the operating system did not answer in time")]
    TimedOut,
    #[error("the operating system failed: {0}")]
    Platform(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// The operating system's Now Playing and its transport controls, behind an
/// interface so the Tray core runs without a real operating system. Each
/// platform implements it.
///
/// The controls act on the Media session that is Now Playing when they are
/// called.
pub trait MediaSource: Send + Sync {
    /// Now Playing as it changes: the receiver always holds the latest value,
    /// `None` while nothing plays. The channel closes when the source stops.
    fn now_playing(&self) -> watch::Receiver<Option<NowPlaying>>;

    /// Sends `control` to the Media session that is Now Playing.
    fn control(
        &self,
        control: TransportControl,
    ) -> impl Future<Output = Result<(), ControlError>> + Send;
}

impl From<protocol::InputAction> for TransportControl {
    fn from(action: protocol::InputAction) -> Self {
        match action {
            protocol::InputAction::PlayPause => Self::TogglePlayPause,
            protocol::InputAction::Previous => Self::Previous,
            protocol::InputAction::Next => Self::Next,
        }
    }
}

impl From<PlaybackState> for protocol::PlaybackState {
    fn from(playback: PlaybackState) -> Self {
        match playback {
            PlaybackState::Playing => Self::Playing,
            PlaybackState::Paused => Self::Paused,
        }
    }
}

impl From<&NowPlaying> for protocol::NowPlaying {
    fn from(now_playing: &NowPlaying) -> Self {
        Self {
            title: now_playing.title.clone(),
            artist: now_playing.artist.clone(),
            playback: now_playing.playback.into(),
        }
    }
}
