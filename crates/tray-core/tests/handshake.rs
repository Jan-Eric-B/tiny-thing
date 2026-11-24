//! Seam 1: the Handshake between the Tray core and the Device daemon, and what
//! happens when the two sides speak different protocol versions.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use protocol::local::DaemonMessage;
use protocol::{
    DeviceMessage, Handshake, HandshakeError, MessageReader, MessageWriter, PROTOCOL_VERSION,
    ProtocolVersion, SoftwareVersion, TrayMessage,
};
use std::net::SocketAddr;
use support::{PATIENCE, RecordingDeviceApp, start_device_daemon};
use tokio::net::{TcpListener, TcpStream};
use tray_core::media::{FakeMediaSource, NowPlaying, PlaybackState};
use tray_core::volume::FakeVolumeControl;
use tray_core::{ConnectError, Session};

fn other_protocol_version() -> ProtocolVersion {
    ProtocolVersion(PROTOCOL_VERSION.0 + 1)
}

#[tokio::test]
async fn after_the_handshake_the_tray_core_knows_the_device_versions() {
    let daemon = start_device_daemon().await;

    let session = Session::connect(daemon.tray_address).await.unwrap();

    assert_eq!(
        session.device_handshake(),
        &Handshake::current(device_daemon::DEVICE_DAEMON_VERSION)
    );
}

#[tokio::test]
async fn the_tray_core_reports_a_device_speaking_another_protocol_version() {
    // A stand-in Device from another release: it answers with its own Handshake.
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let device_address = listener.local_addr().unwrap();
    let device = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, write) = stream.into_split();
        let mut reader = MessageReader::<_, TrayMessage>::new(read);
        let mut writer = MessageWriter::<_, DeviceMessage>::new(write);
        let theirs = Handshake {
            protocol_version: other_protocol_version(),
            software_version: SoftwareVersion::new(9, 0, 0),
        };
        theirs.exchange(&mut reader, &mut writer).await
    });

    let result = Session::connect(device_address).await;

    assert!(
        matches!(
            &result,
            Err(ConnectError::Handshake(HandshakeError::ProtocolMismatch { ours, theirs }))
                if *ours == PROTOCOL_VERSION && *theirs == other_protocol_version()
        ),
        "{:?}",
        result.err()
    );
    // The stand-in Device received the Tray core's Handshake and so can
    // report the mismatch from its side as well.
    assert!(matches!(
        device.await.unwrap(),
        Err(HandshakeError::ProtocolMismatch { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn the_tray_core_gives_up_on_a_device_that_never_answers_the_handshake() {
    // The operating system accepts the connection into the listener's backlog,
    // but nothing ever answers, as behind a half-open connection.
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();

    let result = Session::connect(listener.local_addr().unwrap()).await;

    assert!(
        matches!(
            result,
            Err(ConnectError::Handshake(HandshakeError::TimedOut))
        ),
        "{:?}",
        result.err()
    );
}

#[tokio::test]
async fn the_device_daemon_refuses_a_tray_app_speaking_another_protocol_version() {
    let daemon = start_device_daemon().await;
    let mut device_app = RecordingDeviceApp::connect(daemon.app_address).await;

    // A stand-in Tray app from another release.
    let (read, write) = TcpStream::connect(daemon.tray_address)
        .await
        .unwrap()
        .into_split();
    let mut reader = MessageReader::<_, DeviceMessage>::new(read);
    let mut writer = MessageWriter::<_, TrayMessage>::new(write);
    let theirs = Handshake {
        protocol_version: other_protocol_version(),
        software_version: SoftwareVersion::new(9, 0, 0),
    };
    let result = theirs.exchange(&mut reader, &mut writer).await;
    assert!(
        matches!(
            result,
            Err(HandshakeError::ProtocolMismatch { ours, theirs })
                if ours == other_protocol_version() && theirs == PROTOCOL_VERSION
        ),
        "the Device daemon must state its own version: {result:?}"
    );

    // The Device daemon closes the connection instead of accepting messages
    // it cannot understand.
    let _ = writer.send(&TrayMessage::NowPlaying(None)).await;
    let closed = tokio::time::timeout(PATIENCE, reader.recv()).await;
    assert!(
        matches!(closed, Ok(Ok(None) | Err(_))),
        "connection still open"
    );

    // It stays reachable: a Tray core of the same release connects afterwards.
    let media = FakeMediaSource::new();
    media.set(Some(NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
        playback: PlaybackState::Playing,
        artwork: None,
        progress: None,
    }));
    let session = Session::connect(daemon.tray_address).await.unwrap();
    tokio::spawn(async move { session.run(&media, &FakeVolumeControl::default()).await });
    device_app
        .wait_for(&DaemonMessage::NowPlaying(Some(protocol::NowPlaying {
            title: "Teardrop".to_owned(),
            artist: "Massive Attack".to_owned(),
            playback: protocol::PlaybackState::Playing,
            artwork: None,
            progress: None,
        })))
        .await;
}
