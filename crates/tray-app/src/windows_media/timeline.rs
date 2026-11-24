//! Turns the timeline Windows reports for the current Media session into the
//! progress of Now Playing, and a seek into the position Windows takes. Pure,
//! so the rules are tested without Windows media players.
//!
//! Windows counts in ticks of 100 ns: positions from the session's start
//! time, and the moment the position was taken as a UTC date (the
//! computer's wall clock). That moment is turned into an age against the
//! same wall clock, and the position moved on by it to now, so no wall
//! clock ever leaves the computer.
//!
//! Players report a new timeline when the position changes other than by
//! playing on (a seek, a new song, usually play and pause), and some also
//! every few seconds while playing. Some pause or resume without a new
//! report; [`ProgressFollower`] then carries on from the progress it had,
//! so the Device's bar neither jumps back on a pause nor ahead on a resume.

use protocol::PlaybackRate;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;
use tray_core::media::{PlaybackState, Progress};

/// Ticks from 1601-01-01 (where Windows dates start) to 1970-01-01.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

/// The timeline of a Media session as Windows reports it, in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timeline {
    pub start: i64,
    pub end: i64,
    /// The position as of `last_updated`.
    pub position: i64,
    /// A Windows date (UTC); 0 when the player never set it.
    pub last_updated: i64,
}

/// How the position of a Media session moves, from its playback info.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackInfo {
    pub playback: PlaybackState,
    pub rate: PlaybackRate,
    /// Whether the player accepts a change of position.
    pub can_seek: bool,
}

/// The playback rate Windows reports; normal when it reports none or one
/// the Device cannot follow (not above zero, above 8x, not a number).
pub fn rate(reported: Option<f64>) -> PlaybackRate {
    reported
        .and_then(PlaybackRate::from_f64)
        .unwrap_or(PlaybackRate::NORMAL)
}

/// The progress `timeline` stands for at `now` (monotonic) and `now_utc`
/// (the wall clock as a Windows date, read at the same moment): the
/// reported position moved on by its age while playing, at the rate, never
/// past the end. `None` when the session reports no duration (live streams
/// and players that leave the timeline empty) or one longer than a
/// [`protocol::Progress`] holds.
///
/// A position taken in the future (the wall clock was set back) counts as
/// taken now; one without a date too.
pub fn progress(
    timeline: Timeline,
    info: PlaybackInfo,
    now: Instant,
    now_utc: i64,
) -> Option<Progress> {
    let duration = timeline.end.saturating_sub(timeline.start);
    if duration <= 0 {
        return None;
    }
    let position = timeline
        .position
        .saturating_sub(timeline.start)
        .clamp(0, duration);
    let age = if timeline.last_updated > 0 {
        duration_from_ticks(now_utc.saturating_sub(timeline.last_updated))
    } else {
        Duration::ZERO
    };
    let reported = protocol::Progress::new(
        duration_from_ticks(position),
        duration_from_ticks(duration),
        info.rate,
        info.can_seek,
    )
    .ok()?;
    Some(Progress {
        taken: reported.after(age, info.playback.into()),
        at: now,
    })
}

/// Follows the progress of the current Media session from read to read.
#[derive(Debug, Default)]
pub struct ProgressFollower {
    last: Option<LastRead>,
}

/// What the last read found, and the progress it gave.
#[derive(Debug)]
struct LastRead {
    timeline: Timeline,
    info: PlaybackInfo,
    progress: Option<Progress>,
}

impl ProgressFollower {
    /// The progress at `now` (and `now_utc`, see [`progress`]) from a read of
    /// the session that found `timeline` (`None` when it could not be read)
    /// and `info`.
    ///
    /// When the timeline is the one read last, the player has reported
    /// nothing new: with the same `info` the progress is the one returned
    /// last, unchanged (so an unchanged read publishes nothing); with other
    /// `info` (paused, resumed or a new rate without a new report) it is the
    /// last one moved on to `now` as it was moving until then, and moving
    /// with `info` from then on. Any other timeline is taken as reported.
    pub fn read(
        &mut self,
        timeline: Option<Timeline>,
        info: PlaybackInfo,
        now: Instant,
        now_utc: i64,
    ) -> Option<Progress> {
        let Some(timeline) = timeline else {
            self.forget();
            return None;
        };
        let progress = match &self.last {
            Some(last) if last.timeline == timeline && last.info == info => last.progress,
            Some(last) if last.timeline == timeline => last
                .progress
                .and_then(|progress| carried_on(progress, last.info.playback, info, now)),
            _ => progress(timeline, info, now, now_utc),
        };
        self.last = Some(LastRead {
            timeline,
            info,
            progress,
        });
        progress
    }

    /// Starts over, as for a new Media session.
    pub fn forget(&mut self) {
        self.last = None;
    }
}

/// `progress`, which moved with `playback`, moved on to `now`, and moving
/// with `info` from then on.
fn carried_on(
    progress: Progress,
    playback: PlaybackState,
    info: PlaybackInfo,
    now: Instant,
) -> Option<Progress> {
    let then = progress.at_time(now, playback);
    let taken =
        protocol::Progress::new(then.position(), then.duration(), info.rate, info.can_seek).ok()?;
    Some(Progress { taken, at: now })
}

/// The position to ask Windows for, in ticks, to go to `position` into the
/// song of a session whose timeline runs from `start` to `end`: never
/// before the start or past the end.
pub fn seek_target(start: i64, end: i64, position: Duration) -> i64 {
    let ticks = i64::try_from(position.as_nanos() / 100).unwrap_or(i64::MAX);
    start.saturating_add(ticks).min(end).max(start)
}

/// The wall clock now, as a Windows date.
pub fn utc_now() -> i64 {
    let since_unix_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ticks = i64::try_from(since_unix_epoch.as_nanos() / 100).unwrap_or(i64::MAX);
    UNIX_EPOCH_TICKS.saturating_add(ticks)
}

/// `ticks` as a duration; none for a negative count.
fn duration_from_ticks(ticks: i64) -> Duration {
    let ticks = u64::try_from(ticks).unwrap_or(0);
    Duration::from_nanos(ticks.saturating_mul(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ticks of 100 ns in a second.
    const TICKS_PER_SECOND: i64 = 10_000_000;

    const fn ticks(seconds: i64) -> i64 {
        seconds * TICKS_PER_SECOND
    }

    /// 2026-11-24 as a Windows date.
    const NOW_UTC: i64 = 134_399_520_000_000_000;

    /// "Teardrop" (5:31) at `position_seconds`, reported that long ago.
    fn song(position_seconds: i64, taken_seconds_ago: i64) -> Timeline {
        Timeline {
            start: 0,
            end: ticks(331),
            position: ticks(position_seconds),
            last_updated: NOW_UTC - ticks(taken_seconds_ago),
        }
    }

    const PLAYING: PlaybackInfo = PlaybackInfo {
        playback: PlaybackState::Playing,
        rate: PlaybackRate::NORMAL,
        can_seek: true,
    };

    const PAUSED: PlaybackInfo = PlaybackInfo {
        playback: PlaybackState::Paused,
        ..PLAYING
    };

    fn seconds(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    /// Where `progress` has got to at `now`, playing or paused.
    fn position_at(progress: Option<Progress>, now: Instant, playback: PlaybackState) -> Duration {
        progress.unwrap().at_time(now, playback).position()
    }

    #[test]
    fn position_and_duration_come_from_the_timeline() {
        let now = Instant::now();

        let progress = progress(song(40, 0), PLAYING, now, NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(40));
        assert_eq!(progress.taken.duration(), seconds(331));
        assert_eq!(progress.at, now);
        assert!(progress.taken.can_seek());
    }

    #[test]
    fn a_position_taken_a_while_ago_is_moved_on_by_its_age_while_playing() {
        // A browser that set the position once, 90 s ago, and has played on
        // since without reporting again.
        let now = Instant::now();

        let progress = progress(song(40, 90), PLAYING, now, NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(130));
        assert_eq!(progress.at, now);
    }

    #[test]
    fn a_position_taken_a_while_ago_stays_while_paused() {
        let progress = progress(song(40, 90), PAUSED, Instant::now(), NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(40));
    }

    #[test]
    fn a_position_taken_long_ago_stops_at_the_end() {
        // Older than the monotonic clock may reach back: still not taken
        // as current.
        let long_ago = 10_000_000;

        let progress = progress(song(40, long_ago), PLAYING, Instant::now(), NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(331));
    }

    #[test]
    fn a_position_taken_a_while_ago_moves_on_at_the_playback_rate() {
        let faster = PlaybackInfo {
            rate: PlaybackRate::from_f64(1.5).unwrap(),
            ..PLAYING
        };

        let progress = progress(song(40, 60), faster, Instant::now(), NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(130));
        assert_eq!(progress.taken.rate(), faster.rate);
    }

    #[test]
    fn a_position_taken_in_the_future_counts_as_taken_now() {
        let progress = progress(song(40, -30), PLAYING, Instant::now(), NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(40));
    }

    #[test]
    fn a_position_without_a_date_counts_as_taken_now() {
        let timeline = Timeline {
            last_updated: 0,
            ..song(40, 0)
        };

        let progress = progress(timeline, PLAYING, Instant::now(), NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(40));
    }

    #[test]
    fn positions_count_from_the_start_of_the_timeline() {
        let timeline = Timeline {
            start: ticks(100),
            end: ticks(400),
            position: ticks(160),
            ..song(0, 0)
        };

        let progress = progress(timeline, PAUSED, Instant::now(), NOW_UTC).unwrap();

        assert_eq!(progress.taken.position(), seconds(60));
        assert_eq!(progress.taken.duration(), seconds(300));
        assert_eq!(
            seek_target(timeline.start, timeline.end, seconds(60)),
            ticks(160)
        );
    }

    #[test]
    fn a_position_outside_the_song_is_kept_inside() {
        let past_the_end = Timeline {
            position: ticks(400),
            ..song(0, 0)
        };
        let before_the_start = Timeline {
            position: ticks(-5),
            ..song(0, 0)
        };

        let position = |timeline| {
            progress(timeline, PAUSED, Instant::now(), NOW_UTC)
                .unwrap()
                .taken
                .position()
        };
        assert_eq!(position(past_the_end), seconds(331));
        assert_eq!(position(before_the_start), Duration::ZERO);
    }

    #[test]
    fn a_timeline_without_a_usable_duration_is_no_progress() {
        // Live streams and players that leave the timeline empty, and a
        // duration longer than any song.
        for timeline in [
            Timeline::default(),
            Timeline {
                end: -1,
                ..song(0, 0)
            },
            Timeline {
                end: ticks(25 * 60 * 60),
                ..song(0, 0)
            },
        ] {
            assert_eq!(
                progress(timeline, PLAYING, Instant::now(), NOW_UTC),
                None,
                "{timeline:?}"
            );
        }
    }

    #[test]
    fn whether_the_session_can_seek_goes_with_the_progress() {
        let fixed = PlaybackInfo {
            can_seek: false,
            ..PLAYING
        };

        let progress = progress(song(40, 0), fixed, Instant::now(), NOW_UTC).unwrap();

        assert!(!progress.taken.can_seek());
    }

    #[test]
    fn a_missing_or_unusable_rate_counts_as_normal() {
        for reported in [None, Some(0.0), Some(-1.0), Some(f64::NAN), Some(9.0)] {
            assert_eq!(rate(reported), PlaybackRate::NORMAL, "{reported:?}");
        }
        assert_eq!(rate(Some(1.5)), PlaybackRate::from_f64(1.5).unwrap());
    }

    #[test]
    fn a_seek_asks_for_the_position_in_ticks() {
        assert_eq!(
            seek_target(0, ticks(331), Duration::from_millis(83_250)),
            832_500_000
        );
    }

    #[test]
    fn a_seek_stays_inside_the_timeline() {
        // The duration the Device knows can be off from the timeline read
        // when the seek is carried out (a new song meanwhile, say).
        assert_eq!(
            seek_target(ticks(100), ticks(400), seconds(500)),
            ticks(400)
        );
        assert_eq!(seek_target(ticks(100), ticks(400), seconds(0)), ticks(100));
        assert_eq!(seek_target(ticks(100), ticks(50), seconds(10)), ticks(100));
    }

    #[test]
    fn an_unchanged_read_gives_the_same_progress() {
        let mut follower = ProgressFollower::default();
        let first = Instant::now();
        let read = follower.read(Some(song(40, 0)), PLAYING, first, NOW_UTC);

        // The same report read again a little later, its age measured on a
        // wall clock that moved on by a slightly different amount.
        let again = follower.read(
            Some(song(40, 0)),
            PLAYING,
            first + Duration::from_millis(700),
            NOW_UTC + ticks(1) / 2,
        );

        assert_eq!(again, read);
    }

    #[test]
    fn a_pause_without_a_new_report_stops_where_the_song_got_to() {
        let mut follower = ProgressFollower::default();
        let start = Instant::now();
        follower.read(Some(song(40, 0)), PLAYING, start, NOW_UTC);

        // Ten seconds on, the player pauses but leaves its timeline as it
        // was.
        let paused_at = start + seconds(10);
        let paused = follower.read(Some(song(40, 0)), PAUSED, paused_at, NOW_UTC + ticks(10));

        assert_eq!(
            position_at(paused, paused_at + seconds(30), PlaybackState::Paused),
            seconds(50)
        );
        // Read again while paused: still there.
        let still = follower.read(
            Some(song(40, 0)),
            PAUSED,
            paused_at + seconds(5),
            NOW_UTC + ticks(15),
        );
        assert_eq!(still, paused);
    }

    #[test]
    fn a_resume_without_a_new_report_moves_on_from_where_it_was_paused() {
        let mut follower = ProgressFollower::default();
        let start = Instant::now();
        follower.read(Some(song(40, 0)), PAUSED, start, NOW_UTC);

        // A minute later the player plays again, its timeline as it was.
        let resumed_at = start + seconds(60);
        let resumed = follower.read(Some(song(40, 0)), PLAYING, resumed_at, NOW_UTC + ticks(60));

        assert_eq!(
            position_at(resumed, resumed_at + seconds(5), PlaybackState::Playing),
            seconds(45)
        );
    }

    #[test]
    fn a_rate_change_without_a_new_report_moves_on_at_the_new_rate_from_then() {
        let mut follower = ProgressFollower::default();
        let start = Instant::now();
        follower.read(Some(song(40, 0)), PLAYING, start, NOW_UTC);

        let faster = PlaybackInfo {
            rate: PlaybackRate::from_f64(2.0).unwrap(),
            ..PLAYING
        };
        let changed_at = start + seconds(10);
        let changed = follower.read(Some(song(40, 0)), faster, changed_at, NOW_UTC + ticks(10));

        assert_eq!(
            position_at(changed, changed_at + seconds(10), PlaybackState::Playing),
            seconds(70)
        );
    }

    #[test]
    fn a_new_report_is_taken_as_reported() {
        let mut follower = ProgressFollower::default();
        let start = Instant::now();
        follower.read(Some(song(40, 0)), PLAYING, start, NOW_UTC);

        // A seek on the computer, then a pause reported with its own
        // timeline.
        let now = start + seconds(10);
        let now_utc = NOW_UTC + ticks(10);
        let reported_now = |position_seconds| Timeline {
            last_updated: now_utc,
            ..song(position_seconds, 0)
        };
        let sought = follower.read(Some(reported_now(120)), PLAYING, now, now_utc);
        assert_eq!(
            position_at(sought, now, PlaybackState::Playing),
            seconds(120)
        );
        let paused = follower.read(Some(reported_now(125)), PAUSED, now, now_utc);
        assert_eq!(
            position_at(paused, now, PlaybackState::Paused),
            seconds(125)
        );
    }

    #[test]
    fn a_timeline_that_could_not_be_read_is_no_progress_and_starts_over() {
        let mut follower = ProgressFollower::default();
        let start = Instant::now();
        follower.read(Some(song(40, 0)), PLAYING, start, NOW_UTC);

        assert_eq!(follower.read(None, PLAYING, start, NOW_UTC), None);

        // Read again: the report as reported, not carried on from before.
        let later = start + seconds(10);
        let again = follower.read(Some(song(40, 0)), PAUSED, later, NOW_UTC);
        assert_eq!(
            position_at(again, later, PlaybackState::Paused),
            seconds(40)
        );
    }

    #[test]
    fn the_wall_clock_reads_as_a_windows_date() {
        // 2026-01-01 is 13_411_699_200 s after 1601-01-01; this test runs
        // later than that.
        assert!(utc_now() > 13_411_699_200 * TICKS_PER_SECOND);
    }
}
