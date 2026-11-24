//! Seam 1: Now Playing travels from the `MediaSource` through the Tray core
//! and the Device daemon to the Device app.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::PlaybackRate;
use protocol::local::DaemonMessage;
use std::time::Duration;
use support::{PATIENCE, RecordingDeviceApp, RunningDaemon, start_device_daemon};
use tokio::time::Instant;
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState, Progress};
use tray_core::volume::FakeVolumeControl;
use tray_core::{NOTHING_PLAYING_HOLD, Session};

fn song(title: &str, artist: &str) -> NowPlaying {
    NowPlaying {
        title: title.to_owned(),
        artist: artist.to_owned(),
        playback: PlaybackState::Playing,
        artwork: None,
        progress: None,
    }
}

fn shown(title: &str, artist: &str) -> DaemonMessage {
    DaemonMessage::NowPlaying(Some(protocol::NowPlaying {
        title: title.to_owned(),
        artist: artist.to_owned(),
        playback: protocol::PlaybackState::Playing,
        artwork: None,
        progress: None,
    }))
}

#[tokio::test]
async fn title_and_artist_reach_the_device_app() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", "Massive Attack")));

    let session = Session::connect(daemon.tray_address).await.unwrap();
    tokio::spawn(async move { session.run(&media, &FakeVolumeControl::default()).await });

    device_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;
}

#[tokio::test]
async fn a_song_change_reaches_the_device_app() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", "Massive Attack")));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let media_for_session = media.clone();
    tokio::spawn(async move {
        session
            .run(&media_for_session, &FakeVolumeControl::default())
            .await
    });
    device_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;

    media.set(Some(song("Angel", "Massive Attack")));
    device_app.wait_for(&shown("Angel", "Massive Attack")).await;

    media.set(None);
    device_app.wait_for(&DaemonMessage::NowPlaying(None)).await;
}

#[tokio::test]
async fn a_device_app_that_starts_late_still_gets_what_is_playing() {
    let daemon = start_device_daemon().await;
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", "Massive Attack")));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let media_for_session = media.clone();
    tokio::spawn(async move {
        session
            .run(&media_for_session, &FakeVolumeControl::default())
            .await
    });

    // Give the Now Playing time to reach the Device daemon before the Device
    // app exists, then check that the Device app still learns about it.
    let mut first_app = RecordingDeviceApp::connect(daemon.app_address).await;
    first_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;
    drop(first_app);

    let mut late_app = RecordingDeviceApp::connect(daemon.app_address).await;
    late_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;
}

#[tokio::test]
async fn pausing_and_playing_reach_the_device_app() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", "Massive Attack")));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let media_for_session = media.clone();
    tokio::spawn(async move {
        session
            .run(&media_for_session, &FakeVolumeControl::default())
            .await
    });
    device_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;

    media.set(Some(NowPlaying {
        playback: PlaybackState::Paused,
        ..song("Teardrop", "Massive Attack")
    }));
    device_app
        .wait_for(&DaemonMessage::NowPlaying(Some(protocol::NowPlaying {
            playback: protocol::PlaybackState::Paused,
            ..song("Teardrop", "Massive Attack").to_device(tokio::time::Instant::now())
        })))
        .await;

    media.set(Some(song("Teardrop", "Massive Attack")));
    device_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;
}

/// Keeps a paused clock still while messages travel over the loopback
/// connections, so it moves only by `tokio::time::advance`, until the
/// returned sender is dropped. On its own, Tokio moves a paused clock to the
/// next timer whenever the runtime waits, also for I/O, which would let the
/// hold run out by itself. A running blocking task keeps it still; this one
/// lets go after [`PATIENCE`] of real time, so a test waiting for something
/// that never comes still fails instead of hanging.
fn keep_clock_still() -> std::sync::mpsc::Sender<()> {
    let (release, released) = std::sync::mpsc::channel::<()>();
    tokio::task::spawn_blocking(move || {
        let _ = released.recv_timeout(PATIENCE);
    });
    release
}

/// A Device daemon with a recording Device app and a running session that
/// plays "Teardrop". Returns once that has reached the Device app.
async fn playing() -> (
    RunningDaemon,
    RecordingDeviceApp,
    FakeMediaSource,
    FakeVolumeControl,
) {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", "Massive Attack")));
    let volume = FakeVolumeControl::default();
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let (media_for_session, volume_for_session) = (media.clone(), volume.clone());
    tokio::spawn(async move { session.run(&media_for_session, &volume_for_session).await });
    device_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;
    (daemon, device_app, media, volume)
}

/// Lets the session, which runs on this test's thread, take in a change
/// before the clock moves on.
async fn session_takes_it_in() {
    tokio::task::yield_now().await;
}

/// The next Now Playing the Device app receives.
async fn next_now_playing(device_app: &mut RecordingDeviceApp) -> DaemonMessage {
    device_app
        .wait_until("a Now Playing", |message| {
            matches!(message, DaemonMessage::NowPlaying(_))
        })
        .await
}

/// Checks that no Now Playing has gone out since the last one the Device app
/// received, by taking the output device away: that goes out at once, so a
/// Now Playing sent before it reaches the Device app first.
async fn assert_no_now_playing_went_out(
    device_app: &mut RecordingDeviceApp,
    volume: &FakeVolumeControl,
) {
    volume.set(None);
    let next = device_app
        .wait_until("a Now Playing or no Volume", |message| {
            matches!(
                message,
                DaemonMessage::NowPlaying(_) | DaemonMessage::Volume(None)
            )
        })
        .await;
    assert_eq!(next, DaemonMessage::Volume(None));
}

#[tokio::test(start_paused = true)]
async fn a_moment_of_nothing_playing_between_songs_never_reaches_the_device_app() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, _volume) = playing().await;

    media.set(None);
    session_takes_it_in().await;
    tokio::time::advance(NOTHING_PLAYING_HOLD - Duration::from_millis(1)).await;
    media.set(Some(song("Angel", "Massive Attack")));

    assert_eq!(
        next_now_playing(&mut device_app).await,
        shown("Angel", "Massive Attack")
    );
}

#[tokio::test(start_paused = true)]
async fn nothing_playing_that_lasts_reaches_the_device_app_after_the_hold() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, volume) = playing().await;

    media.set(None);
    let nothing_since = Instant::now();
    session_takes_it_in().await;
    tokio::time::advance(NOTHING_PLAYING_HOLD - Duration::from_millis(1)).await;
    assert_no_now_playing_went_out(&mut device_app, &volume).await;

    tokio::time::advance(Duration::from_millis(1)).await;
    assert_eq!(
        next_now_playing(&mut device_app).await,
        DaemonMessage::NowPlaying(None)
    );
    assert_eq!(nothing_since.elapsed(), NOTHING_PLAYING_HOLD);
}

#[tokio::test(start_paused = true)]
async fn a_new_title_while_nothing_playing_is_held_back_goes_out_at_once_and_ends_the_hold() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, volume) = playing().await;
    media.set(None);
    session_takes_it_in().await;
    tokio::time::advance(Duration::from_millis(100)).await;

    let angel_since = Instant::now();
    media.set(Some(song("Angel", "Massive Attack")));
    assert_eq!(
        next_now_playing(&mut device_app).await,
        shown("Angel", "Massive Attack")
    );
    assert_eq!(angel_since.elapsed(), Duration::ZERO);

    tokio::time::advance(NOTHING_PLAYING_HOLD).await;
    assert_no_now_playing_went_out(&mut device_app, &volume).await;
}

/// "Teardrop" (5:31), at `position` seconds now, playing or paused.
fn teardrop_at(position: f64, playback: PlaybackState) -> NowPlaying {
    teardrop_at_rate(position, playback, PlaybackRate::NORMAL)
}

/// "Teardrop" (5:31), at `position` seconds now, playing or paused at
/// `rate`.
fn teardrop_at_rate(position: f64, playback: PlaybackState, rate: PlaybackRate) -> NowPlaying {
    NowPlaying {
        playback,
        progress: Some(Progress {
            taken: protocol::Progress::new(
                Duration::from_secs_f64(position),
                Duration::from_secs(331),
                rate,
                true,
            )
            .unwrap(),
            at: Instant::now(),
        }),
        ..song("Teardrop", "Massive Attack")
    }
}

/// The position of the next Now Playing with something playing that the
/// Device app receives.
async fn next_position(device_app: &mut RecordingDeviceApp) -> Option<Duration> {
    next_progress(device_app)
        .await
        .map(|progress| progress.position())
}

/// The progress of the next Now Playing with something playing that the
/// Device app receives.
async fn next_progress(device_app: &mut RecordingDeviceApp) -> Option<protocol::Progress> {
    let next = device_app
        .wait_until("something playing", |message| {
            matches!(message, DaemonMessage::NowPlaying(Some(_)))
        })
        .await;
    match next {
        DaemonMessage::NowPlaying(Some(now_playing)) => now_playing.progress,
        _ => unreachable!("matched above"),
    }
}

/// Starts a session once `media` has played for a while.
async fn start_session(media: &FakeMediaSource) -> (RunningDaemon, RecordingDeviceApp) {
    let daemon = start_device_daemon().await;
    let device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let media = media.clone();
    tokio::spawn(async move { session.run(&media, &FakeVolumeControl::default()).await });
    (daemon, device_app)
}

#[tokio::test(start_paused = true)]
async fn the_device_gets_the_position_as_it_is_when_sent_not_as_it_was_reported() {
    let _still = keep_clock_still();
    let media = FakeMediaSource::new();
    // A browser that reported the position once and has played on since.
    media.set(Some(teardrop_at(40.0, PlaybackState::Playing)));
    tokio::time::advance(Duration::from_secs(30)).await;

    let (_daemon, mut device_app) = start_session(&media).await;

    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(70))
    );
}

#[tokio::test(start_paused = true)]
async fn a_paused_position_stays_where_it_was_reported() {
    let _still = keep_clock_still();
    let media = FakeMediaSource::new();
    media.set(Some(teardrop_at(40.0, PlaybackState::Paused)));
    tokio::time::advance(Duration::from_secs(30)).await;

    let (_daemon, mut device_app) = start_session(&media).await;

    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );
}

#[tokio::test(start_paused = true)]
async fn a_position_report_that_agrees_with_the_device_is_not_sent_again() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, volume) = playing().await;
    media.set(Some(teardrop_at(40.0, PlaybackState::Playing)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );

    // Ten seconds on, the player reports where the Device has got to by
    // itself, give or take a little.
    tokio::time::advance(Duration::from_secs(10)).await;
    media.set(Some(teardrop_at(50.4, PlaybackState::Playing)));
    session_takes_it_in().await;
    assert_no_now_playing_went_out(&mut device_app, &volume).await;

    // A position elsewhere (a seek on the computer) goes out.
    media.set(Some(teardrop_at(120.0, PlaybackState::Playing)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(120))
    );
}

#[tokio::test(start_paused = true)]
async fn a_new_position_while_paused_goes_out() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, _volume) = playing().await;
    media.set(Some(teardrop_at(40.0, PlaybackState::Paused)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );

    tokio::time::advance(Duration::from_secs(10)).await;
    media.set(Some(teardrop_at(45.0, PlaybackState::Paused)));

    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(45))
    );
}

#[tokio::test(start_paused = true)]
async fn the_same_song_after_nothing_playing_went_out_is_sent_again() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, _volume) = playing().await;
    media.set(Some(teardrop_at(40.0, PlaybackState::Paused)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );
    media.set(None);
    session_takes_it_in().await;
    tokio::time::advance(NOTHING_PLAYING_HOLD).await;
    assert_eq!(
        next_now_playing(&mut device_app).await,
        DaemonMessage::NowPlaying(None)
    );

    // The player comes back with the same song, paused where it was.
    media.set(Some(teardrop_at(40.0, PlaybackState::Paused)));

    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );
}

#[tokio::test(start_paused = true)]
async fn the_same_song_back_within_the_hold_where_the_device_has_got_to_is_not_sent_again() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, volume) = playing().await;
    media.set(Some(teardrop_at(40.0, PlaybackState::Playing)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );

    media.set(None);
    session_takes_it_in().await;
    tokio::time::advance(Duration::from_millis(200)).await;
    media.set(Some(teardrop_at(40.2, PlaybackState::Playing)));
    session_takes_it_in().await;
    tokio::time::advance(NOTHING_PLAYING_HOLD).await;

    assert_no_now_playing_went_out(&mut device_app, &volume).await;
}

#[tokio::test(start_paused = true)]
async fn a_pause_where_the_device_has_got_to_goes_out() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, _volume) = playing().await;
    media.set(Some(teardrop_at(40.0, PlaybackState::Playing)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );

    tokio::time::advance(Duration::from_millis(500)).await;
    media.set(Some(teardrop_at(40.5, PlaybackState::Paused)));

    let next = next_now_playing(&mut device_app).await;
    let DaemonMessage::NowPlaying(Some(paused)) = next else {
        panic!("expected something playing, got {next:?}");
    };
    assert_eq!(paused.playback, protocol::PlaybackState::Paused);
}

#[tokio::test(start_paused = true)]
async fn a_faster_playback_rate_moves_the_position_on_faster() {
    let _still = keep_clock_still();
    let media = FakeMediaSource::new();
    // A podcast at 1.5x, reported 30 s ago.
    let faster = PlaybackRate::from_f64(1.5).unwrap();
    media.set(Some(teardrop_at_rate(40.0, PlaybackState::Playing, faster)));
    tokio::time::advance(Duration::from_secs(30)).await;

    let (_daemon, mut device_app) = start_session(&media).await;

    let progress = next_progress(&mut device_app).await.unwrap();
    assert_eq!(progress.position(), Duration::from_secs(85));
    assert_eq!(progress.rate(), faster);
}

#[tokio::test(start_paused = true)]
async fn a_report_at_a_faster_rate_that_agrees_with_the_device_is_not_sent_again() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, volume) = playing().await;
    let faster = PlaybackRate::from_f64(2.0).unwrap();
    media.set(Some(teardrop_at_rate(40.0, PlaybackState::Playing, faster)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );

    // Ten seconds at 2x: the Device has got to 60 s by itself.
    tokio::time::advance(Duration::from_secs(10)).await;
    media.set(Some(teardrop_at_rate(60.4, PlaybackState::Playing, faster)));
    session_takes_it_in().await;

    assert_no_now_playing_went_out(&mut device_app, &volume).await;
}

#[tokio::test(start_paused = true)]
async fn a_change_of_playback_rate_goes_out_at_once() {
    let _still = keep_clock_still();
    let (_daemon, mut device_app, media, _volume) = playing().await;
    media.set(Some(teardrop_at(40.0, PlaybackState::Playing)));
    assert_eq!(
        next_position(&mut device_app).await,
        Some(Duration::from_secs(40))
    );

    tokio::time::advance(Duration::from_millis(500)).await;
    let faster = PlaybackRate::from_f64(1.5).unwrap();
    media.set(Some(teardrop_at_rate(40.5, PlaybackState::Playing, faster)));

    assert_eq!(
        next_progress(&mut device_app)
            .await
            .map(|progress| progress.rate()),
        Some(faster)
    );
}
