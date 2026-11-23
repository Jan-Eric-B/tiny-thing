use super::{MediaSource, NowPlaying};
use std::sync::Arc;
use tokio::sync::watch;

/// A [`MediaSource`] whose Now Playing is set by hand, for tests and for
/// running the Tray core against the Simulator without a real media player.
///
/// Clones share the same Now Playing.
#[derive(Debug, Clone)]
pub struct FakeMediaSource {
    now_playing: Arc<watch::Sender<Option<NowPlaying>>>,
}

impl FakeMediaSource {
    /// A source with nothing playing.
    pub fn new() -> Self {
        let (sender, _) = watch::channel(None);
        Self {
            now_playing: Arc::new(sender),
        }
    }

    /// Reports `now_playing` as the new Now Playing; `None` for nothing.
    pub fn set(&self, now_playing: Option<NowPlaying>) {
        self.now_playing.send_replace(now_playing);
    }
}

impl Default for FakeMediaSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaSource for FakeMediaSource {
    fn now_playing(&self) -> watch::Receiver<Option<NowPlaying>> {
        self.now_playing.subscribe()
    }
}
