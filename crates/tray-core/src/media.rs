//! Now Playing as the operating system reports it, and its transport
//! controls.

mod fake;

pub use fake::FakeMediaSource;

use crate::artwork::{self, PIPELINE_VERSION};
use protocol::ArtworkId;
use std::fmt;
use std::hash::{DefaultHasher, Hasher};
use std::sync::Arc;
use tokio::sync::watch;

/// What the operating system reports as Now Playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playback: PlaybackState,
    /// `None` when the Media session supplies no Artwork.
    pub artwork: Option<Artwork>,
}

/// The Artwork a Media session supplies, as the operating system hands it
/// out: an encoded image (PNG, JPEG, BMP or WebP) of any size and aspect
/// ratio. Cheap to clone.
#[derive(Clone, PartialEq, Eq)]
pub struct Artwork {
    id: ArtworkId,
    encoded: Arc<[u8]>,
}

/// Why bytes a Media session supplied are no Artwork.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the Artwork cannot be read: {reason}")]
pub struct UnreadableArtwork {
    reason: String,
}

impl Artwork {
    /// The Artwork in `encoded`, if its header names an image of a known
    /// format, at least one and at most 4096 pixels a side. Only the header
    /// is read; an image whose pixels turn out unreadable shows when the
    /// Device asks for it, and the Device is then told it cannot have it.
    pub fn from_encoded(encoded: impl Into<Arc<[u8]>>) -> Result<Self, UnreadableArtwork> {
        let encoded = encoded.into();
        artwork::size(&encoded).map_err(|reason| UnreadableArtwork { reason })?;
        // The std hasher is fixed within a build, which is all the Device
        // needs: a new Tray app release at most misses its kept Artwork once.
        let mut hasher = DefaultHasher::new();
        hasher.write_u32(PIPELINE_VERSION);
        hasher.write(&encoded);
        Ok(Self {
            id: ArtworkId(hasher.finish()),
            encoded,
        })
    }

    /// Identifies the Artwork by a hash of its content and the version of
    /// the pipeline that prepares it for the Device: the same image has the
    /// same id under any title, and a new image a new one.
    pub fn id(&self) -> ArtworkId {
        self.id
    }

    /// The image as the operating system handed it out.
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
}

impl fmt::Debug for Artwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Artwork({:?}, {} bytes)", self.id, self.encoded.len())
    }
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
            artwork: now_playing.artwork.as_ref().map(Artwork::id),
        }
    }
}
