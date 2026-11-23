use super::{Percent, Step, Volume, VolumeControl, VolumeError, stepped};
use std::sync::Arc;
use tokio::sync::watch;

/// A [`VolumeControl`] whose Volume is set by hand or by
/// [`step`](VolumeControl::step), for tests and for running the Tray core
/// against the Simulator without touching the computer's Volume.
///
/// Like a real computer, it reports every change, including its own, and a
/// step unmutes it.
/// Clones share the same Volume.
#[derive(Debug, Clone)]
pub struct FakeVolumeControl {
    volume: Arc<watch::Sender<Option<Volume>>>,
}

impl FakeVolumeControl {
    /// A computer whose output device is at `volume`.
    pub fn new(volume: Volume) -> Self {
        Self {
            volume: Arc::new(watch::Sender::new(Some(volume))),
        }
    }

    /// Changes the Volume as the user would on the computer; `None` takes the
    /// output device away.
    pub fn set(&self, volume: Option<Volume>) {
        self.volume.send_replace(volume);
    }
}

/// 50 %, checked when this is compiled.
const HALF: Percent = match Percent::new(50) {
    Some(half) => half,
    None => panic!("50 % is a Percent"),
};

impl Default for FakeVolumeControl {
    /// Half the level, not muted.
    fn default() -> Self {
        Self::new(Volume {
            level: HALF,
            muted: false,
        })
    }
}

impl VolumeControl for FakeVolumeControl {
    fn volume(&self) -> watch::Receiver<Option<Volume>> {
        self.volume.subscribe()
    }

    async fn step(&self, step: Step) -> Result<(), VolumeError> {
        let mut result = Err(VolumeError::NoOutputDevice);
        self.volume.send_if_modified(|volume| match volume {
            Some(volume) => {
                result = Ok(());
                let next = Volume {
                    level: stepped(volume.level, step),
                    muted: false,
                };
                let changed = *volume != next;
                *volume = next;
                changed
            }
            None => false,
        });
        result
    }
}
