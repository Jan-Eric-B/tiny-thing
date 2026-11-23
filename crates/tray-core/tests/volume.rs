//! Seam 1: Volume steps travel from the Device app through the Device daemon
//! and the Tray core to the `VolumeControl`, and the computer's Volume travels
//! back to the Device app.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::InputAction;
use protocol::local::DaemonMessage;
use support::{PATIENCE, RecordingDeviceApp, RunningDaemon, start_device_daemon};
use tray_core::Session;
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState, TransportControl};
use tray_core::volume::{FakeVolumeControl, Percent, Step, Volume, VolumeControl, stepped};

fn percent(value: u8) -> Percent {
    Percent::try_from(value).unwrap()
}

fn volume(level: u8) -> Volume {
    Volume {
        level: percent(level),
        muted: false,
    }
}

fn shown(volume: Option<Volume>) -> DaemonMessage {
    DaemonMessage::Volume(volume.map(protocol::Volume::from))
}

/// A Device daemon with a recording Device app and a running session whose
/// computer is at `start`. Returns once that Volume has reached the Device
/// app, so the Device daemon passes input actions on to the Tray core from
/// then on.
async fn connected(
    start: Volume,
) -> (
    RunningDaemon,
    RecordingDeviceApp,
    FakeMediaSource,
    FakeVolumeControl,
) {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
        playback: PlaybackState::Playing,
        artwork: None,
    }));
    let computer = FakeVolumeControl::new(start);
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let (media_for_session, computer_for_session) = (media.clone(), computer.clone());
    tokio::spawn(async move { session.run(&media_for_session, &computer_for_session).await });
    device_app.wait_for(&shown(Some(start))).await;
    (daemon, device_app, media, computer)
}

/// Waits until the computer's Volume level is `level`, failing the test after
/// [`PATIENCE`].
async fn assert_level(computer: &FakeVolumeControl, level: u8) {
    let mut observed = computer.volume();
    let reached = tokio::time::timeout(
        PATIENCE,
        observed.wait_for(|volume| volume.is_some_and(|volume| volume.level.get() == level)),
    )
    .await
    .is_ok();
    assert!(
        reached,
        "expected Volume {level} %, the computer is at {:?}",
        *observed.borrow()
    );
}

#[tokio::test]
async fn each_volume_step_changes_the_volume_by_2_percent() {
    let (_daemon, mut device_app, _media, computer) = connected(volume(50)).await;

    device_app.send(InputAction::VolumeUp).await;
    assert_level(&computer, 52).await;
    device_app.send(InputAction::VolumeUp).await;
    assert_level(&computer, 54).await;
    device_app.send(InputAction::VolumeDown).await;
    assert_level(&computer, 52).await;
}

#[test]
fn a_step_moves_the_level_by_2_percent() {
    assert_eq!(stepped(percent(50), Step::Up), percent(52));
    assert_eq!(stepped(percent(50), Step::Down), percent(48));
}

#[test]
fn a_step_up_stops_at_100_percent() {
    assert_eq!(stepped(percent(99), Step::Up), percent(100));
    assert_eq!(stepped(percent(100), Step::Up), percent(100));
}

#[test]
fn a_step_down_stops_at_0_percent() {
    assert_eq!(stepped(percent(1), Step::Down), percent(0));
    assert_eq!(stepped(percent(0), Step::Down), percent(0));
}

#[tokio::test]
async fn a_volume_step_up_at_100_percent_keeps_the_volume_and_the_session() {
    let (_daemon, mut device_app, _media, computer) = connected(volume(100)).await;

    device_app.send(InputAction::VolumeUp).await;
    device_app.send(InputAction::VolumeDown).await;

    // Steps run in order: from 100 the step up stays at 100, so the step down
    // ends at 98.
    assert_level(&computer, 98).await;
}

#[tokio::test]
async fn a_volume_step_while_muted_unmutes_and_changes_the_level() {
    let (_daemon, mut device_app, _media, _computer) = connected(Volume {
        level: percent(30),
        muted: true,
    })
    .await;

    // Like the Windows volume keys: a step unmutes, up or down.
    device_app.send(InputAction::VolumeDown).await;

    device_app.wait_for(&shown(Some(volume(28)))).await;
}

#[tokio::test]
async fn a_volume_change_on_the_computer_reaches_the_device_app() {
    let (_daemon, mut device_app, _media, computer) = connected(volume(40)).await;

    computer.set(Some(volume(30)));
    device_app.wait_for(&shown(Some(volume(30)))).await;

    computer.set(Some(Volume {
        level: percent(30),
        muted: true,
    }));
    device_app
        .wait_for(&shown(Some(Volume {
            level: percent(30),
            muted: true,
        })))
        .await;
}

#[tokio::test]
async fn losing_the_output_device_reaches_the_device_app_and_the_session_goes_on() {
    let (_daemon, mut device_app, _media, computer) = connected(volume(40)).await;

    computer.set(None);
    device_app.wait_for(&shown(None)).await;
    // Nothing to turn up: the step is not carried out, and the session goes
    // on.
    device_app.send(InputAction::VolumeUp).await;

    computer.set(Some(volume(60)));
    device_app.wait_for(&shown(Some(volume(60)))).await;
    device_app.send(InputAction::VolumeUp).await;
    assert_level(&computer, 62).await;
}

#[tokio::test]
async fn volume_steps_are_not_held_up_by_a_stalled_transport_control() {
    let (_daemon, mut device_app, media, computer) = connected(volume(50)).await;
    media.stall_controls(true);
    device_app.send(InputAction::Next).await;
    let mut calls = media.calls();
    tokio::time::timeout(PATIENCE, calls.wait_for(|calls| !calls.is_empty()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*calls.borrow(), [TransportControl::Next]);

    device_app.send(InputAction::VolumeUp).await;

    assert_level(&computer, 52).await;
}
