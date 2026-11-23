//! Now Playing as the operating system reports it.

mod fake;

pub use fake::FakeMediaSource;

use tokio::sync::watch;

/// What the operating system reports as Now Playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
}

/// The operating system's Now Playing, behind an interface so the Tray core
/// runs without a real operating system. Each platform implements it.
pub trait MediaSource: Send + Sync {
    /// Now Playing as it changes: the receiver always holds the latest value,
    /// `None` while nothing plays. The channel closes when the source stops.
    fn now_playing(&self) -> watch::Receiver<Option<NowPlaying>>;
}

impl From<&NowPlaying> for protocol::NowPlaying {
    fn from(now_playing: &NowPlaying) -> Self {
        Self {
            title: now_playing.title.clone(),
            artist: now_playing.artist.clone(),
        }
    }
}
