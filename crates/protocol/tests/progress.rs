//! Where in the song Now Playing is, a while after it was sent, and what a
//! Progress may hold at all: a duration of more than zero and at most
//! [`MAX_DURATION`], and a playback rate above zero and at most 8x. One that
//! does not cannot be made, nor read from a message.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use protocol::{MAX_DURATION, NowPlaying, PlaybackRate, PlaybackState, Progress, ProgressError};
use serde::Serialize;
use std::time::Duration;

const fn seconds(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn progress(position: Duration, rate: PlaybackRate) -> Progress {
    Progress::new(position, seconds(331), rate, true).unwrap()
}

fn teardrop(playback: PlaybackState, position: Duration) -> NowPlaying {
    NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
        playback,
        artwork: None,
        progress: Some(progress(position, PlaybackRate::NORMAL)),
    }
}

fn position(now_playing: &NowPlaying) -> Option<Duration> {
    now_playing.progress.map(|progress| progress.position())
}

/// A Progress as a peer could put it on the wire, unchecked: the same fields
/// in the same order as [`Progress`], the rate in thousandths.
#[derive(Serialize)]
struct WireProgress {
    position: Duration,
    duration: Duration,
    rate: u16,
    can_seek: bool,
}

fn read(wire: &WireProgress) -> Result<Progress, postcard::Error> {
    postcard::from_bytes(&postcard::to_allocvec(wire).unwrap())
}

fn wire(duration: Duration, rate: u16) -> WireProgress {
    WireProgress {
        position: seconds(10),
        duration,
        rate,
        can_seek: true,
    }
}

#[test]
fn the_position_moves_on_while_playing() {
    let aged = teardrop(PlaybackState::Playing, seconds(40)).aged(seconds(5));

    assert_eq!(position(&aged), Some(seconds(45)));
}

#[test]
fn the_position_stays_while_paused() {
    let aged = teardrop(PlaybackState::Paused, seconds(40)).aged(seconds(5));

    assert_eq!(position(&aged), Some(seconds(40)));
}

#[test]
fn the_position_stops_at_the_end_of_the_song() {
    let aged = teardrop(PlaybackState::Playing, seconds(330)).aged(seconds(60));

    assert_eq!(position(&aged), Some(seconds(331)));
}

#[test]
fn a_song_without_progress_stays_without() {
    let live = NowPlaying {
        progress: None,
        ..teardrop(PlaybackState::Playing, seconds(0))
    };

    assert_eq!(live.clone().aged(seconds(5)), live);
}

#[test]
fn at_one_and_a_half_times_the_position_moves_15_seconds_in_10() {
    let rate = PlaybackRate::from_f64(1.5).unwrap();

    let after = progress(seconds(40), rate).position_after(seconds(10), PlaybackState::Playing);

    assert_eq!(after, seconds(55));
}

#[test]
fn at_half_speed_the_position_moves_5_seconds_in_10() {
    let rate = PlaybackRate::from_f64(0.5).unwrap();

    let after = progress(seconds(40), rate).position_after(seconds(10), PlaybackState::Playing);

    assert_eq!(after, seconds(45));
}

#[test]
fn a_huge_elapsed_time_stops_at_the_end_without_panicking() {
    let fastest = PlaybackRate::from_f64(8.0).unwrap();
    let longest = Progress::new(MAX_DURATION, MAX_DURATION, fastest, true).unwrap();

    assert_eq!(
        longest.position_after(Duration::MAX, PlaybackState::Playing),
        MAX_DURATION
    );
    assert_eq!(
        progress(seconds(0), fastest).position_after(Duration::MAX, PlaybackState::Playing),
        seconds(331)
    );
}

#[test]
fn a_position_past_the_duration_is_taken_as_the_end() {
    let progress = Progress::new(seconds(400), seconds(331), PlaybackRate::NORMAL, true).unwrap();

    assert_eq!(progress.position(), seconds(331));
}

#[test]
fn a_duration_of_zero_cannot_be_made() {
    assert_eq!(
        Progress::new(seconds(0), Duration::ZERO, PlaybackRate::NORMAL, true),
        Err(ProgressError::NoDuration)
    );
}

#[test]
fn a_duration_over_24_hours_cannot_be_made() {
    assert_eq!(MAX_DURATION, seconds(24 * 3600));
    let too_long = MAX_DURATION + Duration::from_nanos(1);

    assert_eq!(
        Progress::new(seconds(0), too_long, PlaybackRate::NORMAL, true),
        Err(ProgressError::TooLong(too_long))
    );
    assert!(Progress::new(seconds(0), MAX_DURATION, PlaybackRate::NORMAL, true).is_ok());
}

#[test]
fn a_progress_reads_back_from_the_wire() {
    let progress = read(&wire(seconds(331), 1500)).unwrap();

    assert_eq!(progress.position(), seconds(10));
    assert_eq!(progress.duration(), seconds(331));
    assert_eq!(progress.rate(), PlaybackRate::from_f64(1.5).unwrap());
    assert!(progress.can_seek());
}

#[test]
fn a_duration_of_zero_from_the_wire_is_rejected() {
    assert!(read(&wire(Duration::ZERO, 1000)).is_err());
}

#[test]
fn a_duration_over_24_hours_from_the_wire_is_rejected() {
    assert!(read(&wire(MAX_DURATION + seconds(1), 1000)).is_err());
}

#[test]
fn a_position_past_the_duration_from_the_wire_is_taken_as_the_end() {
    let progress = read(&WireProgress {
        position: seconds(400),
        ..wire(seconds(331), 1000)
    })
    .unwrap();

    assert_eq!(progress.position(), seconds(331));
}

#[test]
fn a_rate_of_zero_or_above_8x_from_the_wire_is_rejected() {
    assert!(read(&wire(seconds(331), 0)).is_err());
    assert!(read(&wire(seconds(331), 8001)).is_err());
    assert!(read(&wire(seconds(331), 1)).is_ok());
    assert!(read(&wire(seconds(331), 8000)).is_ok());
}

#[test]
fn a_rate_is_taken_in_thousandths() {
    assert_eq!(PlaybackRate::from_f64(1.0), Some(PlaybackRate::NORMAL));
    assert_eq!(PlaybackRate::from_f64(1.0004), Some(PlaybackRate::NORMAL));
    assert_eq!(PlaybackRate::from_f64(1.25).unwrap().as_f64(), 1.25);
    assert_eq!(PlaybackRate::from_f64(8.0).unwrap().as_f64(), 8.0);
}

#[test]
fn a_rate_that_does_not_move_forwards_or_is_above_8x_cannot_be_made() {
    for rate in [
        0.0,
        0.0004,
        -1.0,
        8.001,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert_eq!(PlaybackRate::from_f64(rate), None, "{rate}");
    }
}
