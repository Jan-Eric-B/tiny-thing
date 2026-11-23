//! Seam 1: a peer that vanished without closing its connection (USB unplug,
//! computer asleep) must not lock out the next one. There is exactly one Tray
//! app and one Device app, so the Device daemon lets the newest connection win.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::local::DaemonMessage;
use protocol::{
    DeviceMessage, Handshake, MessageReader, MessageWriter, SoftwareVersion, TrayMessage,
};
use support::{PATIENCE, RecordingDeviceApp, start_device_daemon};
use tokio::net::TcpStream;
use tray_core::Session;
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState};
use tray_core::volume::FakeVolumeControl;

fn teardrop() -> NowPlaying {
    NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
        playback: PlaybackState::Playing,
    }
}

fn teardrop_shown() -> DaemonMessage {
    DaemonMessage::NowPlaying(Some(protocol::NowPlaying::from(&teardrop())))
}

#[tokio::test]
async fn a_silent_old_tray_app_connection_does_not_block_a_new_session() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;

    // An old Tray app connection that completed its Handshake and then went
    // silent without closing.
    let (read, write) = TcpStream::connect(daemon.tray_address)
        .await
        .unwrap()
        .into_split();
    let mut old_reader = MessageReader::<_, DeviceMessage>::new(read);
    let mut old_writer = MessageWriter::<_, TrayMessage>::new(write);
    Handshake::current(SoftwareVersion::new(0, 1, 0))
        .exchange(&mut old_reader, &mut old_writer)
        .await
        .unwrap();

    let media = FakeMediaSource::new();
    media.set(Some(teardrop()));
    let session = tokio::time::timeout(PATIENCE, Session::connect(daemon.tray_address))
        .await
        .expect("the Handshake hangs behind the old connection")
        .unwrap();
    tokio::spawn(async move { session.run(&media, &FakeVolumeControl::default()).await });

    device_app.wait_for(&teardrop_shown()).await;
    // The new session replaced the old connection, so the daemon closed it.
    let old_end = tokio::time::timeout(PATIENCE, old_reader.recv()).await;
    assert!(
        matches!(old_end, Ok(Ok(None) | Err(_))),
        "old connection still open"
    );
}

#[tokio::test]
async fn a_silent_old_device_app_connection_does_not_block_a_new_device_app() {
    let daemon = start_device_daemon().await;
    // An old Device app connection that never reads and never closes.
    let mut old_app = RecordingDeviceApp::connect(daemon.app_address).await;

    let media = FakeMediaSource::new();
    media.set(Some(teardrop()));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    tokio::spawn(async move { session.run(&media, &FakeVolumeControl::default()).await });

    let mut new_app = RecordingDeviceApp::connect(daemon.app_address).await;
    new_app.wait_for(&teardrop_shown()).await;
    old_app.wait_until_closed().await;
}
