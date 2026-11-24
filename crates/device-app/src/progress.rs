//! Where in the song the Device shows Now Playing: the Tray app's position,
//! moved on by the Device's own clock while playing, except right after the
//! user seeked or toggled play/pause on the Device.

use protocol::PlaybackState;
use std::time::{Duration, Instant};

/// How far a position from the Tray app may be from the one expected before
/// it counts as the player having moved (it acted on a seek). The Tray app
/// sends a position that only drifted by more than its own tolerance (1 s),
/// and players report rounded positions.
const SAME_POSITION: Duration = Duration::from_secs(1);

/// The progress to show.
///
/// Two positions are kept. The reported one moves on while the Tray app
/// says playing; the shown one moves on while the Device shows playing,
/// which a play/pause toggled on the Device changes at once. Whenever both
/// say the same, the shown position is the reported one, so a toggle the
/// player ignored ends where the song really is.
///
/// A seek on the Device is shown at once. It ends when the Tray app
/// reports a position away from the one it would have reached anyway (the
/// player acted), or when the owner calls [`expire_seek`](Self::expire_seek)
/// because the player did not act in time. A report that leaves the
/// position where it would be anyway keeps the shown position, also while a
/// toggled play/pause waits for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    reported: Anchor,
    reported_playback: PlaybackState,
    shown: Anchor,
    shown_playback: PlaybackState,
    /// A seek on the Device that the player has not acted on yet.
    seeking: bool,
}

/// A progress and when it was there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Anchor {
    progress: protocol::Progress,
    at: Instant,
}

impl Anchor {
    /// The progress at `now`, moved on while `playback` is playing.
    fn at(self, now: Instant, playback: PlaybackState) -> protocol::Progress {
        self.progress
            .after(now.saturating_duration_since(self.at), playback)
    }
}

impl Progress {
    /// The progress the Tray app reported at `now`, with `reported` playback
    /// and `shown` the playback the Device shows.
    pub fn new(
        progress: protocol::Progress,
        reported: PlaybackState,
        shown: PlaybackState,
        now: Instant,
    ) -> Self {
        let anchor = Anchor { progress, at: now };
        Self {
            reported: anchor,
            reported_playback: reported,
            shown: anchor,
            shown_playback: shown,
            seeking: false,
        }
    }

    /// The Tray app reported `progress` of the same song at `now`. Ends a
    /// seek the player has acted on.
    pub fn report(
        &mut self,
        progress: protocol::Progress,
        reported: PlaybackState,
        shown: PlaybackState,
        now: Instant,
    ) {
        let expected = self.reported_position(now);
        let acted = progress.duration() != self.duration()
            || progress.position().abs_diff(expected) > SAME_POSITION;
        let keep = (self.seeking || shown != reported) && !acted;
        let (seeking, kept) = (self.seeking, self.shown_position(now));
        *self = Self::new(progress, reported, shown, now);
        if keep {
            self.seeking = seeking;
            self.shown.progress = progress.with_position(kept);
        }
    }

    /// The Device shows `playback` from `now` on.
    pub fn show_playback(&mut self, playback: PlaybackState, now: Instant) {
        self.shown = Anchor {
            progress: self.shown.at(now, self.shown_playback),
            at: now,
        };
        self.shown_playback = playback;
        self.follow_reported(now);
    }

    /// The position `fraction` of the way along the song, in whole
    /// milliseconds (finer than anyone taps); `None` when the Media session
    /// cannot seek or `fraction` is not a number.
    pub fn seek_position(&self, fraction: f32) -> Option<Duration> {
        if !self.can_seek() || !fraction.is_finite() {
            return None;
        }
        let fraction = f64::from(fraction.clamp(0.0, 1.0));
        // At most 24 hours, so exact in an f64 and never negative.
        let duration = self.duration().as_millis() as f64;
        Some(Duration::from_millis((duration * fraction) as u64))
    }

    /// The user seeked to `position` at `now`: shown at once.
    pub fn seek(&mut self, position: Duration, now: Instant) {
        self.shown = Anchor {
            progress: self.shown.progress.with_position(position),
            at: now,
        };
        self.seeking = true;
    }

    /// The player did not act on the seek in time: show the reported
    /// position again.
    pub fn expire_seek(&mut self, now: Instant) {
        self.seeking = false;
        self.follow_reported(now);
    }

    /// The position to show at `now`.
    pub fn position(&self, now: Instant) -> Duration {
        self.shown_position(now)
    }

    pub fn duration(&self) -> Duration {
        self.reported.progress.duration()
    }

    /// Whether the Media session lets the Device seek.
    pub fn can_seek(&self) -> bool {
        self.reported.progress.can_seek()
    }

    /// Whether the shown position is moving: playing and not at the end.
    pub fn moving(&self, now: Instant) -> bool {
        self.shown_playback == PlaybackState::Playing && self.position(now) < self.duration()
    }

    fn reported_position(&self, now: Instant) -> Duration {
        self.reported.at(now, self.reported_playback).position()
    }

    fn shown_position(&self, now: Instant) -> Duration {
        self.shown.at(now, self.shown_playback).position()
    }

    /// Shows the reported position, unless a seek or a play/pause toggled
    /// on the Device is waiting for the player.
    fn follow_reported(&mut self, now: Instant) {
        if !self.seeking && self.shown_playback == self.reported_playback {
            self.shown = Anchor {
                progress: self.reported.at(now, self.reported_playback),
                at: now,
            };
        }
    }
}
