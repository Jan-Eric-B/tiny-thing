use crate::PlaybackState;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The longest song a [`Progress`] holds: 24 hours.
pub const MAX_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

/// Where playback is in the song.
///
/// The position is the one at the moment the message is sent; the receiver
/// moves it on from the moment it arrives, on its own clock
/// ([`Progress::position_after`]). So no timestamp travels, and the clocks of
/// the computer and the Device never need to agree. (On the USB network a
/// message takes about a millisecond.)
///
/// The duration is more than zero and at most [`MAX_DURATION`], and the
/// position at most the duration, checked when it is made and when it is
/// read from a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "UncheckedProgress")]
pub struct Progress {
    position: Duration,
    duration: Duration,
    rate: PlaybackRate,
    can_seek: bool,
}

/// How a [`Progress`] would break its contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProgressError {
    #[error("the duration is zero")]
    NoDuration,
    #[error("the duration {0:?} is longer than {MAX_DURATION:?}")]
    TooLong(Duration),
}

impl Progress {
    /// At `position` of a song of `duration`, moving at `rate` while playing,
    /// and whether the Media session lets the Device seek. A position past
    /// the duration is taken as the end. Refused for a duration of zero or
    /// longer than [`MAX_DURATION`].
    pub fn new(
        position: Duration,
        duration: Duration,
        rate: PlaybackRate,
        can_seek: bool,
    ) -> Result<Self, ProgressError> {
        if duration.is_zero() {
            return Err(ProgressError::NoDuration);
        }
        if duration > MAX_DURATION {
            return Err(ProgressError::TooLong(duration));
        }
        Ok(Self {
            position: position.min(duration),
            duration,
            rate,
            can_seek,
        })
    }

    /// How far into the song playback is; at most the duration.
    pub fn position(&self) -> Duration {
        self.position
    }

    /// How long the song is; more than zero, at most [`MAX_DURATION`].
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// How fast the position moves while playing.
    pub fn rate(&self) -> PlaybackRate {
        self.rate
    }

    /// Whether the Media session lets the Device seek
    /// ([`crate::InputAction::Seek`]).
    pub fn can_seek(&self) -> bool {
        self.can_seek
    }

    /// The same song at `position` instead; a position past the duration is
    /// taken as the end.
    #[must_use]
    pub fn with_position(self, position: Duration) -> Self {
        Self {
            position: position.min(self.duration),
            ..self
        }
    }

    /// The position `elapsed` later: moved on by `elapsed` times the rate
    /// while `playback` is playing, unchanged while paused, and never past
    /// the duration.
    pub fn position_after(&self, elapsed: Duration, playback: PlaybackState) -> Duration {
        match playback {
            PlaybackState::Paused => self.position,
            PlaybackState::Playing => {
                // At most about 1.8e28 ns times 8000: far inside a u128.
                let moved = elapsed.as_nanos() * u128::from(self.rate.0) / 1000;
                let position = self.position.as_nanos().saturating_add(moved);
                let end = self.duration.as_nanos();
                // At most MAX_DURATION, so it fits a u64 of nanoseconds.
                Duration::from_nanos(u64::try_from(position.min(end)).unwrap_or(u64::MAX))
            }
        }
    }

    /// This Progress `elapsed` later ([`Progress::position_after`]).
    #[must_use]
    pub fn after(self, elapsed: Duration, playback: PlaybackState) -> Self {
        self.with_position(self.position_after(elapsed, playback))
    }
}

/// A [`Progress`] as read from a message, before it is checked: the same
/// fields in the same order, so it reads what a `Progress` writes.
#[derive(Deserialize)]
struct UncheckedProgress {
    position: Duration,
    duration: Duration,
    rate: PlaybackRate,
    can_seek: bool,
}

impl TryFrom<UncheckedProgress> for Progress {
    type Error = ProgressError;

    fn try_from(unchecked: UncheckedProgress) -> Result<Self, ProgressError> {
        let UncheckedProgress {
            position,
            duration,
            rate,
            can_seek,
        } = unchecked;
        Self::new(position, duration, rate, can_seek)
    }
}

/// How fast the position moves while playing, in thousandths of normal
/// speed: above zero and at most 8x. A player that runs backwards, stands
/// still or runs faster has no rate the Device can follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct PlaybackRate(u16);

impl PlaybackRate {
    /// Normal speed.
    pub const NORMAL: Self = Self(1000);
    /// The fastest rate: 8x.
    pub const MAX: Self = Self(8000);

    /// `rate` times normal speed, to the nearest thousandth; `None` when
    /// that is not above zero, above 8x, or not a number.
    pub fn from_f64(rate: f64) -> Option<Self> {
        let thousandths = (rate * 1000.0).round();
        if (1.0..=f64::from(Self::MAX.0)).contains(&thousandths) {
            // A whole number from 1 to 8000, checked just above.
            Some(Self(thousandths as u16))
        } else {
            None
        }
    }

    /// The rate as a multiple of normal speed.
    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

/// A rate of zero or above 8x where a [`PlaybackRate`] was expected, in
/// thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a playback rate of {0}/1000 is not above zero and at most 8x")]
pub struct OutOfRateRange(pub u16);

impl TryFrom<u16> for PlaybackRate {
    type Error = OutOfRateRange;

    fn try_from(thousandths: u16) -> Result<Self, OutOfRateRange> {
        if (1..=Self::MAX.0).contains(&thousandths) {
            Ok(Self(thousandths))
        } else {
            Err(OutOfRateRange(thousandths))
        }
    }
}

impl From<PlaybackRate> for u16 {
    fn from(rate: PlaybackRate) -> Self {
        rate.0
    }
}
