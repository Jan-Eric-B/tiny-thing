//! The Device daemon keeps the latest Volume the Tray app reported and hands
//! it to every new Device app connection (ADR-0004: the Device app may come
//! and go, the Device daemon stays).

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_daemon::{DaemonConfig, DeviceDaemon};
use protocol::local::DaemonMessage;
use protocol::{
    DeviceMessage, Handshake, MessageReader, MessageWriter, Percent, SoftwareVersion, TrayMessage,
    Volume,
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// How long a test waits for something that should happen at once.
const PATIENCE: Duration = Duration::from_secs(5);

/// A Volume of `level` percent, not muted. The one place a test builds a
/// level.
fn volume(level: u8) -> Volume {
    Volume {
        level: Percent::try_from(level).unwrap(),
        muted: false,
    }
}

/// Starts a Device daemon on loopback ports; returns the Tray app address and
/// the Device app address. The daemon stops with the test's runtime.
async fn start_device_daemon() -> (SocketAddr, SocketAddr) {
    let loopback_any_port = SocketAddr::from(([127, 0, 0, 1], 0));
    let daemon = DeviceDaemon::bind(&DaemonConfig {
        tray_address: loopback_any_port,
        app_address: loopback_any_port,
    })
    .await
    .unwrap();
    let addresses = (
        daemon.tray_address().unwrap(),
        daemon.app_address().unwrap(),
    );
    tokio::spawn(daemon.run());
    addresses
}

/// Stands in for the Tray app: connects, completes the Handshake and returns
/// the writer for the messages after it. The reader is returned too, so the
/// connection stays open for as long as the test holds both.
async fn connect_tray_app(
    tray_address: SocketAddr,
) -> (
    MessageReader<OwnedReadHalf, DeviceMessage>,
    MessageWriter<OwnedWriteHalf, TrayMessage>,
) {
    let (read, write) = TcpStream::connect(tray_address).await.unwrap().into_split();
    let mut reader = MessageReader::new(read);
    let mut writer = MessageWriter::new(write);
    Handshake::current(SoftwareVersion::new(0, 1, 0))
        .exchange(&mut reader, &mut writer)
        .await
        .unwrap();
    (reader, writer)
}

/// Connects a stand-in Device app and waits until `expected` arrives on it,
/// failing the test after [`PATIENCE`].
async fn device_app_receives(app_address: SocketAddr, expected: &DaemonMessage) {
    // The write half stays open while the Device app listens: the Device
    // daemon ends a connection whose Device app has stopped sending.
    let (read, _write) = TcpStream::connect(app_address).await.unwrap().into_split();
    let mut reader = MessageReader::<_, DaemonMessage>::new(read);
    let mut received = Vec::new();
    let found = tokio::time::timeout(PATIENCE, async {
        loop {
            let message = reader
                .recv()
                .await
                .unwrap()
                .expect("the Device daemon closed the local socket");
            if &message == expected {
                return;
            }
            received.push(message);
        }
    })
    .await;
    assert!(
        found.is_ok(),
        "expected {expected:?}, received only {received:?}"
    );
}

#[tokio::test]
async fn a_device_app_that_starts_late_still_gets_the_volume() {
    let (tray_address, app_address) = start_device_daemon().await;
    let (_tray_reader, mut tray_writer) = connect_tray_app(tray_address).await;
    tray_writer
        .send(&TrayMessage::Volume(Some(volume(42))))
        .await
        .unwrap();
    let shown = DaemonMessage::Volume(Some(volume(42)));

    // Give the Volume time to reach the Device daemon before the Device app
    // exists, then check that the Device app still learns about it.
    device_app_receives(app_address, &shown).await;
    device_app_receives(app_address, &shown).await;
}
