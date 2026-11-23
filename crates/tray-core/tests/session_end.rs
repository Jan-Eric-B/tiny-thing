//! Seam 1: how a running session ends. The Tray app reconnects after the
//! first ending and stops after the second, so the two must stay apart.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::local::DaemonMessage;
use support::{PATIENCE, RecordingDeviceApp, start_device_daemon};
use tray_core::media::{FakeMediaSource, NowPlaying};
use tray_core::{Session, SessionError};

fn teardrop() -> NowPlaying {
    NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
    }
}

fn teardrop_shown() -> DaemonMessage {
    DaemonMessage::NowPlaying(Some(protocol::NowPlaying::from(&teardrop())))
}

#[tokio::test]
async fn the_session_reports_the_device_gone_when_the_device_daemon_stops() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(teardrop()));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let running = tokio::spawn(session.run(&media));
    device_app.wait_for(&teardrop_shown()).await;

    drop(daemon);

    let result = tokio::time::timeout(PATIENCE, running)
        .await
        .expect("the session outlived the Device daemon")
        .unwrap();
    assert!(
        matches!(result, Err(SessionError::DeviceGone)),
        "{result:?}"
    );
}

#[tokio::test]
async fn the_session_ends_cleanly_when_the_media_source_stops() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    // The only handle to the source; dropping it closes its channel.
    let media = FakeMediaSource::new();
    media.set(Some(teardrop()));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let running = tokio::spawn(session.run(&media));
    device_app.wait_for(&teardrop_shown()).await;

    drop(media);

    let result = tokio::time::timeout(PATIENCE, running)
        .await
        .expect("the session outlived its media source")
        .unwrap();
    assert!(matches!(result, Ok(())), "{result:?}");
}
