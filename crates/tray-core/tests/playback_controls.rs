//! Seam 1: input actions travel from the Device app through the Device daemon
//! and the Tray core to the `MediaSource`.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::InputAction;
use protocol::local::DaemonMessage;
use std::time::Duration;
use support::{PATIENCE, RecordingDeviceApp, RunningDaemon, start_device_daemon};
use tokio::task::JoinHandle;
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState, TransportControl};
use tray_core::volume::FakeVolumeControl;
use tray_core::{Session, SessionError};

/// How long a test lets messages already sent settle where nothing tells
/// when they have arrived. Loopback takes microseconds.
const SETTLE: Duration = Duration::from_millis(200);

fn song(title: &str) -> NowPlaying {
    NowPlaying {
        title: title.to_owned(),
        artist: "Massive Attack".to_owned(),
        playback: PlaybackState::Playing,
        artwork: None,
    }
}

fn shown(title: &str) -> DaemonMessage {
    DaemonMessage::NowPlaying(Some(protocol::NowPlaying::from(&song(title))))
}

/// A Device daemon with a recording Device app and a running session. Returns
/// once Now Playing has reached the Device app, so the Device daemon passes
/// input actions on to the Tray core from then on.
async fn connected() -> (
    RunningDaemon,
    RecordingDeviceApp,
    FakeMediaSource,
    JoinHandle<Result<(), SessionError>>,
) {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let (media, running) = start_session(&daemon, &mut device_app, "Teardrop").await;
    (daemon, device_app, media, running)
}

/// Starts a session whose media source plays `title`, and waits until that
/// has reached the Device app.
async fn start_session(
    daemon: &RunningDaemon,
    device_app: &mut RecordingDeviceApp,
    title: &str,
) -> (FakeMediaSource, JoinHandle<Result<(), SessionError>>) {
    let media = FakeMediaSource::new();
    media.set(Some(song(title)));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let media_for_session = media.clone();
    let running = tokio::spawn(async move {
        session
            .run(&media_for_session, &FakeVolumeControl::default())
            .await
    });
    device_app.wait_for(&shown(title)).await;
    (media, running)
}

/// Waits until `media` has seen `expected.len()` control calls, failing the
/// test after [`PATIENCE`], and checks that they are `expected`.
async fn assert_calls(media: &FakeMediaSource, expected: &[TransportControl]) {
    let mut calls = media.calls();
    let arrived = tokio::time::timeout(
        PATIENCE,
        calls.wait_for(|calls| calls.len() >= expected.len()),
    )
    .await
    .map(|calls| calls.unwrap().clone());
    assert_eq!(arrived.as_deref(), Ok(expected), "control calls");
}

#[tokio::test]
async fn each_input_action_arrives_as_the_matching_media_source_call() {
    let (_daemon, mut device_app, media, _running) = connected().await;

    device_app.send(InputAction::PlayPause).await;
    device_app.send(InputAction::Previous).await;
    device_app.send(InputAction::Next).await;

    assert_calls(
        &media,
        &[
            TransportControl::TogglePlayPause,
            TransportControl::Previous,
            TransportControl::Next,
        ],
    )
    .await;
}

#[tokio::test]
async fn a_failed_control_leaves_the_session_running() {
    let (_daemon, mut device_app, media, _running) = connected().await;
    media.fail_controls(true);

    device_app.send(InputAction::Next).await;
    assert_calls(&media, &[TransportControl::Next]).await;

    // Now Playing still flows, and so do later actions.
    media.set(Some(song("Angel")));
    device_app.wait_for(&shown("Angel")).await;
    media.fail_controls(false);
    device_app.send(InputAction::Previous).await;
    assert_calls(
        &media,
        &[TransportControl::Next, TransportControl::Previous],
    )
    .await;
}

#[tokio::test]
async fn a_stalled_control_does_not_stop_the_session_reading_the_device() {
    let (daemon, mut device_app, media, running) = connected().await;
    media.stall_controls(true);
    device_app.send(InputAction::Next).await;
    assert_calls(&media, &[TransportControl::Next]).await;

    // The Device goes away while the player has not answered yet.
    drop(daemon);

    let result = tokio::time::timeout(PATIENCE, running)
        .await
        .expect("the stalled control kept the session from noticing")
        .unwrap();
    assert!(
        matches!(result, Err(SessionError::DeviceGone)),
        "{result:?}"
    );
}

#[tokio::test]
async fn presses_during_a_stalled_control_are_dropped_beyond_one() {
    let (_daemon, mut device_app, media, _running) = connected().await;
    media.stall_controls(true);
    device_app.send(InputAction::Next).await;
    assert_calls(&media, &[TransportControl::Next]).await;

    // One press waits for the stalled control; the rest are dropped.
    device_app.send(InputAction::Previous).await;
    device_app.send(InputAction::Next).await;
    device_app.send(InputAction::Next).await;
    tokio::time::sleep(SETTLE).await;
    media.stall_controls(false);
    assert_calls(
        &media,
        &[TransportControl::Next, TransportControl::Previous],
    )
    .await;

    // A fresh press is carried out next, with no stale one before it.
    device_app.send(InputAction::PlayPause).await;
    assert_calls(
        &media,
        &[
            TransportControl::Next,
            TransportControl::Previous,
            TransportControl::TogglePlayPause,
        ],
    )
    .await;
}

#[tokio::test]
async fn a_press_before_any_session_is_not_carried_out_later() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    device_app.send(InputAction::PlayPause).await;
    // Let the Device daemon take the press while no Tray app is connected.
    tokio::time::sleep(SETTLE).await;

    let (media, _running) = start_session(&daemon, &mut device_app, "Teardrop").await;
    device_app.send(InputAction::Next).await;

    assert_calls(&media, &[TransportControl::Next]).await;
}

#[tokio::test]
async fn after_a_new_session_replaces_the_old_one_presses_reach_only_the_new_one() {
    let (daemon, mut device_app, old_media, old_running) = connected().await;
    let (new_media, _new_running) = start_session(&daemon, &mut device_app, "Angel").await;

    device_app.send(InputAction::Next).await;
    assert_calls(&new_media, &[TransportControl::Next]).await;

    // Once the old session has ended, nothing more can reach its source.
    let old_result = tokio::time::timeout(PATIENCE, old_running)
        .await
        .expect("the Device daemon kept the old connection open")
        .unwrap();
    assert!(
        matches!(old_result, Err(SessionError::DeviceGone)),
        "{old_result:?}"
    );
    assert_eq!(*old_media.calls().borrow(), [], "old session's calls");
}
