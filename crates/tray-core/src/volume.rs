//! The computer's Volume (its master output volume, not that of an app).

mod fake;

pub use fake::FakeVolumeControl;
pub use protocol::Percent;

use tokio::sync::watch;

/// How far one Volume step from the Device moves the level, in percent.
pub const VOLUME_STEP: u8 = 2;

/// The computer's Volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Volume {
    /// The level.
    pub level: Percent,
    /// Whether the computer is muted. The level stays what it was.
    pub muted: bool,
}

/// Why the Volume could not be changed.
#[derive(Debug, thiserror::Error)]
pub enum VolumeError {
    #[error("the computer has no output device")]
    NoOutputDevice,
    #[error("the operating system failed: {0}")]
    Platform(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// The computer's master Volume, behind an interface so the Tray core runs
/// without a real operating system. Each platform implements it.
///
/// It always acts on the computer's current default output device, also
/// after the user switches to another one.
pub trait VolumeControl: Send + Sync {
    /// The Volume as it changes, whoever changed it: the receiver always
    /// holds the latest value, `None` while there is no output device.
    ///
    /// The channel stays open for as long as the `VolumeControl` lives.
    fn volume(&self) -> watch::Receiver<Option<Volume>>;

    /// Moves the level one step ([`stepped`]) from the level the operating
    /// system reports right now and unmutes the computer, like the Windows
    /// volume keys (up or down, also when the level is already at its
    /// limit). Reading, setting and unmuting are one operation on one output
    /// device, so a switch to another default device never applies one
    /// device's level to another.
    fn step(&self, step: Step) -> impl Future<Output = Result<(), VolumeError>> + Send;
}

/// The level one step from `level` leads to: [`VOLUME_STEP`] up or down,
/// within 0 to 100 %.
pub fn stepped(level: Percent, step: Step) -> Percent {
    match step {
        Step::Up => level.saturating_add(VOLUME_STEP),
        Step::Down => level.saturating_sub(VOLUME_STEP),
    }
}

/// The direction of one Volume step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
}

impl From<Volume> for protocol::Volume {
    fn from(volume: Volume) -> Self {
        Self {
            level: volume.level,
            muted: volume.muted,
        }
    }
}
