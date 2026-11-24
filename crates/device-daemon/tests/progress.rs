//! The Device daemon hands Now Playing to a Device app that starts late with
//! the position it has now, not the one it had when it arrived.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_daemon::{DaemonConfig, DeviceDaemon};
use protocol::local::DaemonMessage;
use protocol::{
    DeviceMessage, Handshake, MessageReader, MessageWriter, NowPlaying, PlaybackRate,
    PlaybackState, Progress, SoftwareVersion, TrayMessage,
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// How long a test waits for something that should happen at once.
const PATIENCE: Duration = Duration::from_secs(5);

/// How long the Now Playing waits in the Device daemon before the Device
/// app starts.
const WAITED: Duration = Duration::from_millis(300);

fn teardrop(playback: PlaybackState) -> NowPlaying {
    NowPlaying {
        title: "Teardrop".to_owned(),
        artist: "Massive Attack".to_owned(),
        playback,
        artwork: None,
        progress: Some(
            Progress::new(
                Duration::from_secs(40),
                Duration::from_secs(331),
                PlaybackRate::NORMAL,
                true,
            )
            .unwrap(),
        ),
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
/// the reader and writer for the messages after it.
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

/// Reports `now_playing` to a Device daemon, waits [`WAITED`], then starts
/// a Device app and returns the position of the first Now Playing it gets.
async fn position_shown_by_a_late_device_app(now_playing: NowPlaying) -> Duration {
    let (tray_address, app_address) = start_device_daemon().await;
    let (_tray_reader, mut tray_writer) = connect_tray_app(tray_address).await;
    tray_writer
        .send(&TrayMessage::NowPlaying(Some(now_playing)))
        .await
        .unwrap();
    tokio::time::sleep(WAITED).await;

    let (read, _write) = TcpStream::connect(app_address).await.unwrap().into_split();
    let mut reader = MessageReader::<_, DaemonMessage>::new(read);
    tokio::time::timeout(PATIENCE, async {
        loop {
            if let DaemonMessage::NowPlaying(Some(now_playing)) =
                reader.recv().await.unwrap().unwrap()
            {
                return now_playing.progress.unwrap().position();
            }
        }
    })
    .await
    .expect("no Now Playing reached the Device app")
}

#[tokio::test]
async fn a_device_app_that_starts_late_gets_the_position_moved_on_while_playing() {
    let position = position_shown_by_a_late_device_app(teardrop(PlaybackState::Playing)).await;

    // Moved on by about WAITED: less when the Device daemon read the
    // message late (its clock starts at the read, the test's at the send),
    // more for the time the Device app took to connect. Half of WAITED is
    // far from both that and from not moved on at all.
    let reported = Duration::from_secs(40);
    assert!(
        position >= reported + WAITED / 2 && position < reported + WAITED + PATIENCE,
        "{position:?}"
    );
}

#[tokio::test]
async fn a_device_app_that_starts_late_gets_a_paused_position_as_it_was() {
    let position = position_shown_by_a_late_device_app(teardrop(PlaybackState::Paused)).await;

    assert_eq!(position, Duration::from_secs(40));
}
