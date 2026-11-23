use super::{ControlError, MediaSource, NowPlaying, TransportControl};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::watch;

/// A [`MediaSource`] whose Now Playing is set by hand and which records the
/// transport controls it receives, for tests and for running the Tray core
/// against the Simulator without a real media player.
///
/// Its controls change nothing about Now Playing; set that by hand too.
/// Clones share the same Now Playing and the same record of calls.
#[derive(Debug, Clone)]
pub struct FakeMediaSource {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    /// `None` once the source is stopped, which closes the channel.
    now_playing: Mutex<Option<watch::Sender<Option<NowPlaying>>>>,
    calls: watch::Sender<Vec<TransportControl>>,
    fail_controls: AtomicBool,
    /// `true` while controls wait before they answer.
    stall_controls: watch::Sender<bool>,
}

impl FakeMediaSource {
    /// A source with nothing playing.
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                now_playing: Mutex::new(Some(watch::Sender::new(None))),
                calls: watch::Sender::new(Vec::new()),
                fail_controls: AtomicBool::new(false),
                stall_controls: watch::Sender::new(false),
            }),
        }
    }

    /// Reports `now_playing` as the new Now Playing; `None` for nothing.
    /// Does nothing once the source is stopped.
    pub fn set(&self, now_playing: Option<NowPlaying>) {
        if let Some(sender) = self.sender().as_ref() {
            sender.send_replace(now_playing);
        }
    }

    /// Stops the source, as when the operating system stops reporting Now
    /// Playing: the Now Playing channel closes.
    pub fn stop(&self) {
        self.sender().take();
    }

    /// Every transport control received so far, oldest first; the receiver
    /// sees each new one.
    pub fn calls(&self) -> watch::Receiver<Vec<TransportControl>> {
        self.shared.calls.subscribe()
    }

    /// Makes the transport controls fail (`true`) or succeed (`false`, the
    /// default). Failed calls are recorded too.
    pub fn fail_controls(&self, fail: bool) {
        self.shared.fail_controls.store(fail, Ordering::Relaxed);
    }

    /// Makes the transport controls wait before they answer (`true`), as a
    /// slow player does, until called with `false` (the default). A stalled
    /// control is recorded when it arrives, before it waits.
    pub fn stall_controls(&self, stall: bool) {
        self.shared.stall_controls.send_replace(stall);
    }

    fn sender(&self) -> std::sync::MutexGuard<'_, Option<watch::Sender<Option<NowPlaying>>>> {
        // The guarded value stays consistent even if a holder panicked.
        self.shared
            .now_playing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    async fn answer(&self, control: TransportControl) -> Result<(), ControlError> {
        self.shared.calls.send_modify(|calls| calls.push(control));
        let mut stalled = self.shared.stall_controls.subscribe();
        // Cannot fail: `self` holds the sender.
        let _released = stalled.wait_for(|stalled| !stalled).await;
        if self.shared.fail_controls.load(Ordering::Relaxed) {
            Err(ControlError::Refused)
        } else {
            Ok(())
        }
    }
}

impl Default for FakeMediaSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaSource for FakeMediaSource {
    fn now_playing(&self) -> watch::Receiver<Option<NowPlaying>> {
        match self.sender().as_ref() {
            Some(sender) => sender.subscribe(),
            // Stopped: a channel that is closed already.
            None => watch::channel(None).1,
        }
    }

    async fn control(&self, control: TransportControl) -> Result<(), ControlError> {
        self.answer(control).await
    }
}
