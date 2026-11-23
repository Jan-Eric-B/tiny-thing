//! The Device daemon keeps the last few Artworks, asks the Tray app only for
//! Artwork it does not have, and hands the Artwork of Now Playing to the
//! Device app, also to one that starts late. Artwork the Tray app cannot
//! make shows as none; Artwork that breaks the size contract is dropped.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_daemon::{ARTWORK_CACHE_LEN, DaemonConfig, DeviceDaemon};
use protocol::local::DaemonMessage;
use protocol::{
    Artwork, ArtworkId, DeviceMessage, Handshake, Image, MessageReader, MessageWriter, NowPlaying,
    PlaybackState, Rgb, RgbPixels, SCREEN_HEIGHT, SCREEN_WIDTH, SoftwareVersion, TrayMessage,
    Volume,
};
use serde::Serialize;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

/// How long a test waits for something that should happen at once.
const PATIENCE: Duration = Duration::from_secs(5);

/// How long a test waits to be sure something does not happen.
const QUIET: Duration = Duration::from_millis(200);

const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

/// An image of `width` by `height` in a color of its own for each `id`.
fn image(id: u64, width: u32, height: u32) -> Image {
    Image::encode(&RgbPixels {
        width,
        height,
        rgb: [id.to_le_bytes()[0], 0, 0].repeat((width * height) as usize),
    })
    .unwrap()
}

fn artwork(id: u64) -> Artwork {
    Artwork::new(
        ArtworkId(id),
        image(id, 4, 4),
        image(id, SCREEN_WIDTH, SCREEN_HEIGHT),
        WHITE,
    )
    .unwrap()
}

fn now_playing(title: &str, artwork: Option<u64>) -> Option<NowPlaying> {
    Some(NowPlaying {
        title: title.to_owned(),
        artist: "Massive Attack".to_owned(),
        playback: PlaybackState::Playing,
        artwork: artwork.map(ArtworkId),
    })
}

fn playing(title: &str, artwork: Option<u64>) -> TrayMessage {
    TrayMessage::NowPlaying(now_playing(title, artwork))
}

/// A Tray message as a broken or hostile Tray app could send it: the same
/// variants in the same order as [`TrayMessage`], but with an Artwork whose
/// sizes nobody checked.
#[derive(Serialize)]
enum UncheckedTrayMessage {
    NowPlaying(Option<NowPlaying>),
    #[allow(dead_code, reason = "keeps the variant order of TrayMessage")]
    Volume(Option<Volume>),
    Artwork(UncheckedArtwork),
}

/// The fields of an [`Artwork`] in the same order.
#[derive(Serialize)]
struct UncheckedArtwork {
    id: ArtworkId,
    image: Image,
    backdrop: Image,
    accent: Rgb,
}

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

/// Stands in for the Tray app after the Handshake; `M` is what it sends.
struct TrayApp<M = TrayMessage> {
    reader: MessageReader<OwnedReadHalf, DeviceMessage>,
    writer: MessageWriter<OwnedWriteHalf, M>,
}

impl<M: Serialize> TrayApp<M> {
    async fn connect(tray_address: SocketAddr) -> Self {
        let (read, write) = TcpStream::connect(tray_address).await.unwrap().into_split();
        let mut reader = MessageReader::new(read);
        let mut writer = MessageWriter::new(write);
        Handshake::current(SoftwareVersion::new(0, 1, 0))
            .exchange(&mut reader, &mut writer)
            .await
            .unwrap();
        Self { reader, writer }
    }

    async fn send(&mut self, message: M) {
        self.writer.send(&message).await.unwrap();
    }

    /// The next Artwork the Device asks for, failing the test after
    /// [`PATIENCE`].
    async fn asked_for(&mut self) -> ArtworkId {
        let message = tokio::time::timeout(PATIENCE, self.reader.recv())
            .await
            .expect("the Device asked for no Artwork")
            .unwrap()
            .unwrap();
        match message {
            DeviceMessage::MissingArtwork(id) => id,
            other => panic!("expected a request for Artwork, got {other:?}"),
        }
    }

    /// Fails the test if the Device asks for anything within [`QUIET`].
    async fn asks_for_nothing(&mut self) {
        if let Ok(message) = tokio::time::timeout(QUIET, self.reader.recv()).await {
            panic!("expected no request, got {message:?}");
        }
    }
}

impl TrayApp {
    /// Plays a song with Artwork `id` and answers the Device's request for it.
    async fn play_new_artwork(&mut self, title: &str, id: u64) {
        self.send(playing(title, Some(id))).await;
        assert_eq!(self.asked_for().await, ArtworkId(id));
        self.send(TrayMessage::Artwork(artwork(id))).await;
    }
}

/// Stands in for the Device app.
struct DeviceApp {
    reader: MessageReader<OwnedReadHalf, DaemonMessage>,
    // The Device daemon ends a connection whose Device app stopped sending.
    _writer: OwnedWriteHalf,
}

impl DeviceApp {
    async fn connect(app_address: SocketAddr) -> Self {
        let (read, write) = TcpStream::connect(app_address).await.unwrap().into_split();
        Self {
            reader: MessageReader::new(read),
            _writer: write,
        }
    }

    /// Waits until `expected` arrives, failing the test after [`PATIENCE`].
    async fn receives(&mut self, expected: &DaemonMessage) {
        let mut received = Vec::new();
        let found = tokio::time::timeout(PATIENCE, async {
            loop {
                let message = self.reader.recv().await.unwrap().unwrap();
                if &message == expected {
                    return;
                }
                received.push(message);
            }
        })
        .await;
        assert!(
            found.is_ok(),
            "expected {expected:?}, received {received:?}"
        );
    }

    /// Waits until Artwork `id` arrives, failing the test after [`PATIENCE`].
    async fn receives_artwork(&mut self, id: u64) {
        self.receives(&DaemonMessage::Artwork(artwork(id))).await;
    }

    /// Fails the test if any Artwork arrives within [`QUIET`].
    async fn receives_no_artwork(&mut self) {
        let _quiet = tokio::time::timeout(QUIET, async {
            loop {
                let message = self.reader.recv().await.unwrap().unwrap();
                assert!(
                    !matches!(message, DaemonMessage::Artwork(_)),
                    "expected no Artwork, got {message:?}"
                );
            }
        })
        .await;
    }
}

#[tokio::test]
async fn artwork_the_device_does_not_have_is_asked_for_and_reaches_the_device_app() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut device_app = DeviceApp::connect(app_address).await;
    let mut tray_app = TrayApp::connect(tray_address).await;

    tray_app.send(playing("Teardrop", Some(1))).await;
    assert_eq!(tray_app.asked_for().await, ArtworkId(1));
    tray_app.send(TrayMessage::Artwork(artwork(1))).await;

    device_app.receives_artwork(1).await;
}

#[tokio::test]
async fn artwork_is_asked_for_once_however_often_now_playing_changes() {
    let (tray_address, _app_address) = start_device_daemon().await;
    let mut tray_app = TrayApp::connect(tray_address).await;

    tray_app.send(playing("Teardrop", Some(1))).await;
    assert_eq!(tray_app.asked_for().await, ArtworkId(1));
    // Paused, then playing again: the same Artwork, still on its way.
    tray_app
        .send(TrayMessage::NowPlaying(Some(NowPlaying {
            title: "Teardrop".to_owned(),
            artist: "Massive Attack".to_owned(),
            playback: PlaybackState::Paused,
            artwork: Some(ArtworkId(1)),
        })))
        .await;
    tray_app.send(playing("Teardrop", Some(1))).await;

    tray_app.asks_for_nothing().await;
}

#[tokio::test]
async fn artwork_the_device_keeps_is_not_asked_for_again() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut device_app = DeviceApp::connect(app_address).await;
    let mut tray_app = TrayApp::connect(tray_address).await;
    tray_app.play_new_artwork("Teardrop", 1).await;
    device_app.receives_artwork(1).await;
    tray_app.play_new_artwork("Angel", 2).await;
    device_app.receives_artwork(2).await;

    tray_app.send(playing("Teardrop", Some(1))).await;

    device_app.receives_artwork(1).await;
    tray_app.asks_for_nothing().await;
}

#[tokio::test]
async fn the_device_keeps_artwork_across_tray_app_connections() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut device_app = DeviceApp::connect(app_address).await;
    let mut first = TrayApp::connect(tray_address).await;
    first.play_new_artwork("Teardrop", 1).await;
    device_app.receives_artwork(1).await;
    first.play_new_artwork("Angel", 2).await;
    device_app.receives_artwork(2).await;
    drop(first);

    let mut second = TrayApp::connect(tray_address).await;
    second.send(playing("Teardrop", Some(1))).await;

    device_app.receives_artwork(1).await;
    second.asks_for_nothing().await;
}

#[tokio::test]
async fn the_device_keeps_only_the_last_few_artworks() {
    let (tray_address, _app_address) = start_device_daemon().await;
    let mut tray_app = TrayApp::connect(tray_address).await;
    let count = u64::try_from(ARTWORK_CACHE_LEN).unwrap();
    for id in 1..=count + 1 {
        tray_app.play_new_artwork("Song", id).await;
    }

    // The newest are still there; the oldest is gone.
    tray_app.send(playing("Song", Some(2))).await;
    tray_app.asks_for_nothing().await;
    tray_app.send(playing("Song", Some(1))).await;
    assert_eq!(tray_app.asked_for().await, ArtworkId(1));
}

#[tokio::test]
async fn artwork_for_a_song_no_longer_playing_is_kept_but_not_shown() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut device_app = DeviceApp::connect(app_address).await;
    let mut tray_app = TrayApp::connect(tray_address).await;
    tray_app.send(playing("Teardrop", Some(1))).await;
    assert_eq!(tray_app.asked_for().await, ArtworkId(1));
    tray_app.play_new_artwork("Angel", 2).await;
    device_app.receives_artwork(2).await;

    // The answer to the first request arrives late.
    tray_app.send(TrayMessage::Artwork(artwork(1))).await;
    tray_app.send(playing("Teardrop", Some(1))).await;

    device_app.receives_artwork(1).await;
    tray_app.asks_for_nothing().await;
}

#[tokio::test]
async fn a_device_app_that_starts_late_still_gets_the_artwork() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut tray_app = TrayApp::connect(tray_address).await;
    tray_app.play_new_artwork("Teardrop", 1).await;
    DeviceApp::connect(app_address)
        .await
        .receives_artwork(1)
        .await;

    DeviceApp::connect(app_address)
        .await
        .receives_artwork(1)
        .await;
}

#[tokio::test]
async fn artwork_the_tray_app_cannot_make_reaches_the_device_app_as_unavailable() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut device_app = DeviceApp::connect(app_address).await;
    let mut tray_app = TrayApp::connect(tray_address).await;
    tray_app.send(playing("Broken", Some(1))).await;
    assert_eq!(tray_app.asked_for().await, ArtworkId(1));

    tray_app
        .send(TrayMessage::ArtworkUnavailable(ArtworkId(1)))
        .await;

    let unavailable = DaemonMessage::ArtworkUnavailable(ArtworkId(1));
    device_app.receives(&unavailable).await;
    // Also for a Device app that starts later (and replaces the first), and
    // when the song comes back after one with Artwork.
    let mut restarted = DeviceApp::connect(app_address).await;
    restarted.receives(&unavailable).await;
    tray_app.play_new_artwork("Teardrop", 2).await;
    restarted.receives_artwork(2).await;
    tray_app.send(playing("Broken", Some(1))).await;
    restarted.receives(&unavailable).await;
    tray_app.asks_for_nothing().await;
}

#[tokio::test]
async fn artwork_that_breaks_the_size_contract_is_dropped_before_it_is_kept() {
    let (tray_address, app_address) = start_device_daemon().await;
    let mut device_app = DeviceApp::connect(app_address).await;
    let mut broken = TrayApp::<UncheckedTrayMessage>::connect(tray_address).await;
    broken
        .send(UncheckedTrayMessage::NowPlaying(now_playing(
            "Teardrop",
            Some(1),
        )))
        .await;
    assert_eq!(broken.asked_for().await, ArtworkId(1));

    // A Backdrop the size of the Artwork box instead of the screen.
    broken
        .send(UncheckedTrayMessage::Artwork(UncheckedArtwork {
            id: ArtworkId(1),
            image: image(1, 4, 4),
            backdrop: image(1, 352, 352),
            accent: WHITE,
        }))
        .await;

    device_app.receives_no_artwork().await;
    // Not kept either: a Tray app that plays it again is asked for it.
    let mut tray_app = TrayApp::connect(tray_address).await;
    tray_app.send(playing("Teardrop", Some(1))).await;
    assert_eq!(tray_app.asked_for().await, ArtworkId(1));
}
