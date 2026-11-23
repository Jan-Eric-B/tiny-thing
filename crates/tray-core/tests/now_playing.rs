//! Seam 1: Now Playing travels from the `MediaSource` through the Tray core
//! and the Device daemon to the Device app.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::local::DaemonMessage;
use support::{RecordingDeviceApp, start_device_daemon};
use tray_core::Session;
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState};

fn song(title: &str, artist: &str) -> NowPlaying {
    NowPlaying {
        title: title.to_owned(),
        artist: artist.to_owned(),
        playback: PlaybackState::Playing,
    }
}

fn shown(title: &str, artist: &str) -> DaemonMessage {
    DaemonMessage::NowPlaying(Some(protocol::NowPlaying {
        title: title.to_owned(),
        artist: artist.to_owned(),
        playback: protocol::PlaybackState::Playing,
    }))
}

#[tokio::test]
async fn title_and_artist_reach_the_device_app() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", "Massive Attack")));

    let session = Session::connect(daemon.tray_address).await.unwrap();
    tokio::spawn(async move { session.run(&media).await });

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
    tokio::spawn(async move { session.run(&media_for_session).await });
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
    tokio::spawn(async move { session.run(&media_for_session).await });

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
    tokio::spawn(async move { session.run(&media_for_session).await });
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
            ..protocol::NowPlaying::from(&song("Teardrop", "Massive Attack"))
        })))
        .await;

    media.set(Some(song("Teardrop", "Massive Attack")));
    device_app
        .wait_for(&shown("Teardrop", "Massive Attack"))
        .await;
}
