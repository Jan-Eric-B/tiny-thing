//! Seam 1: the Artwork of Now Playing travels from the `MediaSource` through
//! the Tray core and the Device daemon to the Device app, scaled to the exact
//! size the Device draws, with a Backdrop that white text stays readable on
//! and a colored Accent that the icons stay readable in on a control's
//! circle, and that is no darker than those two need.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

mod support;

use image::{ImageFormat, RgbImage};
use protocol::local::DaemonMessage;
use protocol::{
    ArtworkId, CONTROL_CIRCLE_WHITE_ALPHA, DeviceMessage, Handshake, MessageReader, MessageWriter,
    RgbPixels, SoftwareVersion, TrayMessage,
};
use std::io::Cursor;
use std::net::SocketAddr;
use std::time::Duration;
use support::{PATIENCE, RecordingDeviceApp, RunningDaemon, start_device_daemon};
use tokio::net::TcpListener;
use tray_core::Session;
use tray_core::media::{Artwork, FakeMediaSource, NowPlaying, PlaybackState};
use tray_core::volume::FakeVolumeControl;

const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];

/// A cover drawn by `pixel`, encoded as `format`, as a media player hands it
/// out.
fn cover_as(
    format: ImageFormat,
    width: u32,
    height: u32,
    pixel: impl Fn(u32, u32) -> [u8; 3],
) -> Artwork {
    let image = RgbImage::from_fn(width, height, |x, y| image::Rgb(pixel(x, y)));
    let mut encoded = Cursor::new(Vec::new());
    image.write_to(&mut encoded, format).unwrap();
    Artwork::from_encoded(encoded.into_inner()).unwrap()
}

fn cover(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Artwork {
    cover_as(ImageFormat::Png, width, height, pixel)
}

fn plain_cover(color: [u8; 3]) -> Artwork {
    cover(300, 300, |_, _| color)
}

fn song(title: &str, artwork: Option<Artwork>) -> NowPlaying {
    NowPlaying {
        title: title.to_owned(),
        artist: "Massive Attack".to_owned(),
        playback: PlaybackState::Playing,
        artwork,
    }
}

/// A running session whose `MediaSource` the test controls.
async fn connected() -> (RunningDaemon, RecordingDeviceApp, FakeMediaSource) {
    let daemon = start_device_daemon().await;
    let device_app = RecordingDeviceApp::connect(daemon.app_address).await;
    let media = FakeMediaSource::new();
    let session = Session::connect(daemon.tray_address).await.unwrap();
    let media_for_session = media.clone();
    tokio::spawn(async move {
        session
            .run(&media_for_session, &FakeVolumeControl::default())
            .await
    });
    (daemon, device_app, media)
}

/// The Artwork id the Device app is told for the Now Playing titled `title`.
async fn artwork_id_of(device_app: &mut RecordingDeviceApp, title: &str) -> Option<ArtworkId> {
    let message = device_app
        .wait_until(&format!("Now Playing {title:?}"), |message| {
            matches!(message, DaemonMessage::NowPlaying(Some(now_playing)) if now_playing.title == title)
        })
        .await;
    match message {
        DaemonMessage::NowPlaying(Some(now_playing)) => now_playing.artwork,
        _ => unreachable!(),
    }
}

/// The Artwork the Device app receives for `id`.
async fn artwork(device_app: &mut RecordingDeviceApp, id: ArtworkId) -> protocol::Artwork {
    let message = device_app
        .wait_until(
            &format!("Artwork {id:?}"),
            |message| matches!(message, DaemonMessage::Artwork(artwork) if artwork.id() == id),
        )
        .await;
    match message {
        DaemonMessage::Artwork(artwork) => artwork,
        _ => unreachable!(),
    }
}

/// Plays `cover` under `title` and returns the Artwork the Device app gets.
async fn shown(
    media: &FakeMediaSource,
    device_app: &mut RecordingDeviceApp,
    title: &str,
    cover: Artwork,
) -> protocol::Artwork {
    media.set(Some(song(title, Some(cover))));
    let id = artwork_id_of(device_app, title)
        .await
        .expect("Now Playing names no Artwork");
    artwork(device_app, id).await
}

/// WCAG 2 relative luminance of an sRGB color, its components 0 to 255.
fn luminance_of([r, g, b]: [f64; 3]) -> f64 {
    let linear = |component: f64| {
        let c = component / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn luminance(color: [u8; 3]) -> f64 {
    luminance_of(color.map(f64::from))
}

/// WCAG 2 contrast ratio of two luminances, 1 to 21.
fn contrast_of(a: f64, b: f64) -> f64 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    contrast_of(luminance(a), luminance(b))
}

/// The luminance of a Backdrop pixel under a control's circle: the Device
/// app lays white over it, blending sRGB values.
fn on_control(pixel: [u8; 3]) -> f64 {
    let alpha = f64::from(CONTROL_CIRCLE_WHITE_ALPHA) / 255.0;
    luminance_of(pixel.map(|component| f64::from(component) * (1.0 - alpha) + 255.0 * alpha))
}

/// HSV saturation: how far the weakest channel is below the strongest.
fn saturation(color: [u8; 3]) -> f64 {
    let max = f64::from(color.into_iter().max().unwrap());
    let min = f64::from(color.into_iter().min().unwrap());
    if max == 0.0 { 0.0 } else { (max - min) / max }
}

fn pixels(image: &protocol::Image) -> RgbPixels {
    image.decode().unwrap()
}

fn each_pixel(pixels: &RgbPixels) -> impl Iterator<Item = [u8; 3]> + '_ {
    pixels.rgb.as_chunks::<3>().0.iter().copied()
}

fn accent(artwork: &protocol::Artwork) -> [u8; 3] {
    let accent = artwork.accent();
    [accent.r, accent.g, accent.b]
}

#[tokio::test]
async fn artwork_reaches_the_device_app_at_the_exact_size_it_is_drawn() {
    let (_daemon, mut device_app, media) = connected().await;
    // Source size, and the size the Device draws: the longer side fills the
    // 352-pixel Artwork box, enlarged at most twice.
    let cases = [
        ("large square", ImageFormat::Png, (1000, 1000), (352, 352)),
        (
            "video thumbnail",
            ImageFormat::Jpeg,
            (1280, 720),
            (352, 198),
        ),
        ("portrait", ImageFormat::Png, (720, 1280), (198, 352)),
        ("a bit small", ImageFormat::Png, (300, 300), (352, 352)),
        ("tiny", ImageFormat::Png, (100, 100), (200, 200)),
        ("tiny video", ImageFormat::Jpeg, (160, 90), (320, 180)),
    ];

    for (title, format, (width, height), expected) in cases {
        let source = cover_as(format, width, height, |x, y| {
            [(x % 256) as u8, (y % 256) as u8, 128]
        });
        let artwork = shown(&media, &mut device_app, title, source).await;

        assert_eq!(artwork.image().size(), Some(expected), "{title}");
        assert_eq!(artwork.backdrop().size(), Some((800, 480)), "{title}");
    }
}

#[tokio::test]
async fn non_square_artwork_is_shown_whole_and_undistorted() {
    let (_daemon, mut device_app, media) = connected().await;
    // A 16:9 thumbnail: white with a black frame one sixteenth wide.
    let framed = cover(1280, 720, |x, y| {
        let frame = !(80..1200).contains(&x) || !(45..675).contains(&y);
        if frame { BLACK } else { WHITE }
    });

    let artwork = shown(&media, &mut device_app, "Video", framed).await;

    // Nothing cut off: the frame is there on all four sides.
    let image = pixels(artwork.image());
    let at = |x: u32, y: u32| {
        let index = ((y * image.width + x) * 3) as usize;
        image.rgb[index]
    };
    let (w, h) = (image.width, image.height);
    for (x, y) in [(5, h / 2), (w - 6, h / 2), (w / 2, 3), (w / 2, h - 4)] {
        assert!(at(x, y) < 40, "frame missing at {x},{y}");
    }
    assert!(at(w / 2, h / 2) > 215, "center not white");
}

#[tokio::test]
async fn downscaling_mixes_colors_in_linear_light() {
    let (_daemon, mut device_app, media) = connected().await;
    // Black and white pixels in a checkerboard, twice the Artwork box. Mixed
    // in linear light they make a gray of half the light (sRGB 188); mixed
    // in sRGB values, a far too dark 128.
    let checkerboard = cover(
        704,
        704,
        |x, y| if (x + y) % 2 == 0 { BLACK } else { WHITE },
    );

    let artwork = shown(&media, &mut device_app, "Checkerboard", checkerboard).await;

    let image = pixels(artwork.image());
    let total: u64 = image.rgb.iter().map(|&c| u64::from(c)).sum();
    let mean = total / image.rgb.len() as u64;
    assert!((180..=196).contains(&mean), "mean {mean}");
}

#[tokio::test]
async fn the_artwork_id_is_the_same_for_the_same_image() {
    let (_daemon, mut device_app, media) = connected().await;
    media.set(Some(song("Teardrop", Some(plain_cover([200, 30, 40])))));
    let first = artwork_id_of(&mut device_app, "Teardrop").await;
    media.set(Some(song("Angel", Some(plain_cover([30, 40, 200])))));
    let other = artwork_id_of(&mut device_app, "Angel").await;

    // The same image again, read anew, under the same and another title.
    media.set(Some(song("Teardrop", Some(plain_cover([200, 30, 40])))));
    let again = artwork_id_of(&mut device_app, "Teardrop").await;
    media.set(Some(song(
        "Teardrop (live)",
        Some(plain_cover([200, 30, 40])),
    )));
    let other_title = artwork_id_of(&mut device_app, "Teardrop (live)").await;

    assert!(first.is_some());
    assert_ne!(first, other);
    assert_eq!(again, first);
    assert_eq!(other_title, first);
}

#[tokio::test]
async fn a_new_image_after_the_new_title_replaces_the_artwork() {
    let (_daemon, mut device_app, media) = connected().await;
    let red = [200, 30, 40];
    let blue = [30, 40, 200];
    let old = shown(&media, &mut device_app, "Teardrop", plain_cover(red)).await;

    // Some players (Tidal) report the new title first, still with the old
    // image, and the new image a moment later.
    media.set(Some(song("Angel", Some(plain_cover(red)))));
    assert_eq!(
        artwork_id_of(&mut device_app, "Angel").await,
        Some(old.id())
    );
    media.set(Some(song("Angel", Some(plain_cover(blue)))));
    let new_id = artwork_id_of(&mut device_app, "Angel").await.unwrap();
    let new = artwork(&mut device_app, new_id).await;

    assert_ne!(new_id, old.id());
    assert_eq!(each_pixel(&pixels(new.image())).next(), Some(blue));
}

#[tokio::test]
async fn now_playing_without_artwork_names_none() {
    let (_daemon, mut device_app, media) = connected().await;

    media.set(Some(song("Lecture 3", None)));

    assert_eq!(artwork_id_of(&mut device_app, "Lecture 3").await, None);
}

#[test]
fn bytes_that_are_no_readable_image_are_no_artwork() {
    let empty_png = {
        // A PNG header claiming 0x0 pixels.
        // The PNG signature, then an IHDR chunk.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 13, 10, 0x1a, 10, 0, 0, 0, 13];
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 8, 2, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    };

    for (what, bytes) in [
        ("garbage", b"not an image".to_vec()),
        ("no pixels", empty_png),
    ] {
        assert!(Artwork::from_encoded(bytes).is_err(), "{what}");
    }
}

#[test]
fn an_image_larger_than_any_cover_is_no_artwork() {
    let image = RgbImage::new(4097, 1);
    let mut encoded = Cursor::new(Vec::new());
    image.write_to(&mut encoded, ImageFormat::Png).unwrap();

    assert!(Artwork::from_encoded(encoded.into_inner()).is_err());
}

#[tokio::test]
async fn artwork_whose_pixels_cannot_be_read_reaches_the_device_app_as_unavailable() {
    let (_daemon, mut device_app, media) = connected().await;
    // The header is fine, the pixels are cut off halfway.
    let noise = cover(300, 300, |x, y| {
        let [a, b, c, _] = (x * 7919 + y * 104_729).to_le_bytes();
        [a, b ^ c, c]
    });
    let cut = &noise.encoded()[..noise.encoded().len() / 2];
    media.set(Some(song(
        "Broken",
        Some(Artwork::from_encoded(cut.to_vec()).unwrap()),
    )));
    let id = artwork_id_of(&mut device_app, "Broken")
        .await
        .expect("Now Playing names no Artwork");

    device_app
        .wait_for(&DaemonMessage::ArtworkUnavailable(id))
        .await;

    // The session goes on.
    let working = shown(&media, &mut device_app, "Teardrop", plain_cover(WHITE)).await;
    assert_eq!(working.image().size(), Some((352, 352)));
}

/// Covers from very bright to very dark, flat and busy, gray and colorful.
fn test_covers() -> Vec<(&'static str, Artwork)> {
    vec![
        ("white", plain_cover(WHITE)),
        ("black", plain_cover(BLACK)),
        ("mid gray", plain_cover([128, 128, 128])),
        ("bright yellow", plain_cover([255, 240, 60])),
        ("saturated red", plain_cover([230, 20, 20])),
        ("dark blue", plain_cover([10, 20, 90])),
        (
            "half white, half black",
            cover(300, 300, |x, _| if x < 150 { WHITE } else { BLACK }),
        ),
        (
            "half deep blue, half white",
            cover(300, 300, |x, _| if x < 150 { [0, 0, 30] } else { WHITE }),
        ),
        (
            "white with a small red square",
            cover(300, 300, |x, y| {
                if (140..160).contains(&x) && (140..160).contains(&y) {
                    [255, 0, 0]
                } else {
                    WHITE
                }
            }),
        ),
        (
            "rainbow gradient",
            cover(400, 400, |x, y| {
                [
                    (x * 255 / 399) as u8,
                    (y * 255 / 399) as u8,
                    255 - (x * 255 / 399) as u8,
                ]
            }),
        ),
        (
            "noise",
            cover(300, 300, |x, y| {
                let n = x
                    .wrapping_mul(2_654_435_761)
                    .wrapping_add(y.wrapping_mul(40_503));
                let [a, b, c, _] = n.to_le_bytes();
                [a, b, c]
            }),
        ),
    ]
}

#[tokio::test]
async fn white_text_is_readable_on_the_backdrop_of_every_test_cover() {
    let (_daemon, mut device_app, media) = connected().await;

    for (title, cover) in test_covers() {
        let artwork = shown(&media, &mut device_app, title, cover).await;

        let backdrop = pixels(artwork.backdrop());
        let worst = each_pixel(&backdrop)
            .map(|pixel| contrast(pixel, WHITE))
            .fold(f64::INFINITY, f64::min);
        assert!(
            worst >= 4.5,
            "{title}: white on the Backdrop is {worst:.2}:1"
        );
    }
}

#[tokio::test]
async fn the_accent_stands_out_on_a_control_anywhere_on_the_backdrop_of_every_test_cover() {
    let (_daemon, mut device_app, media) = connected().await;

    for (title, cover) in test_covers() {
        let artwork = shown(&media, &mut device_app, title, cover).await;

        let accent = luminance(accent(&artwork));
        let backdrop = pixels(artwork.backdrop());
        let worst = each_pixel(&backdrop)
            .map(|pixel| contrast_of(accent, on_control(pixel)))
            .fold(f64::INFINITY, f64::min);
        assert!(
            worst >= 4.5,
            "{title}: the Accent on a control is {worst:.2}:1"
        );
    }
}

#[tokio::test]
async fn the_backdrop_of_a_bright_cover_is_darkened_no_further_than_text_and_accent_need() {
    let (_daemon, mut device_app, media) = connected().await;
    let red_square = cover(300, 300, |x, y| {
        if (140..160).contains(&x) && (140..160).contains(&y) {
            [255, 0, 0]
        } else {
            WHITE
        }
    });
    let blue_and_white = cover(300, 300, |x, _| if x < 150 { [0, 0, 30] } else { WHITE });

    for (title, cover) in [
        ("white", plain_cover(WHITE)),
        ("bright yellow", plain_cover([255, 240, 60])),
        ("light pink", plain_cover([250, 200, 210])),
        ("white with a small red square", red_square),
        ("half deep blue, half white", blue_and_white),
    ] {
        let artwork = shown(&media, &mut device_app, title, cover).await;

        // The tighter of the two contrasts at the Backdrop's worst pixel is
        // just above 4.5:1, not far beyond it: darker would only be gloomier.
        let accent = luminance(accent(&artwork));
        let tightest = each_pixel(&pixels(artwork.backdrop()))
            .map(|pixel| contrast(pixel, WHITE).min(contrast_of(accent, on_control(pixel))))
            .fold(f64::INFINITY, f64::min);
        assert!(
            tightest < 4.75,
            "{title}: darkened to {tightest:.2}:1 where 4.5:1 is needed"
        );
    }
}

const RED: usize = 0;
const GREEN: usize = 1;
const BLUE: usize = 2;

/// Covers with a clear hue, and the strongest channel in that hue.
fn colored_covers() -> Vec<(&'static str, Artwork, usize)> {
    vec![
        ("red", plain_cover([200, 20, 30]), RED),
        ("saturated red", plain_cover([230, 20, 20]), RED),
        ("blue", plain_cover([20, 40, 200]), BLUE),
        ("dark blue", plain_cover([10, 20, 90]), BLUE),
        ("green", plain_cover([30, 160, 40]), GREEN),
        ("light pink", plain_cover([250, 200, 210]), RED),
        (
            "white with a small red square",
            cover(300, 300, |x, y| {
                if (140..160).contains(&x) && (140..160).contains(&y) {
                    [255, 0, 0]
                } else {
                    WHITE
                }
            }),
            RED,
        ),
        (
            "half deep blue, half white",
            cover(300, 300, |x, _| if x < 150 { [0, 0, 60] } else { WHITE }),
            BLUE,
        ),
    ]
}

#[tokio::test]
async fn the_accent_of_a_colored_cover_is_clearly_colored_in_its_hue() {
    let (_daemon, mut device_app, media) = connected().await;

    for (title, cover, strongest) in colored_covers() {
        let accent = accent(&shown(&media, &mut device_app, title, cover).await);

        // Even the darkest hue, blue, keeps a quarter of its saturation once
        // light enough for a control.
        assert!(
            saturation(accent) >= 0.25,
            "{title}: the Accent {accent:?} is nearly white"
        );
        for other in (0..3).filter(|&channel| channel != strongest) {
            assert!(
                accent[strongest] > accent[other],
                "{title}: the Accent {accent:?} is not in the hue of the cover"
            );
        }
    }
}

#[tokio::test]
async fn the_accent_of_a_gray_cover_is_white() {
    let (_daemon, mut device_app, media) = connected().await;

    for (title, cover) in [
        ("white", plain_cover(WHITE)),
        ("black", plain_cover(BLACK)),
        ("mid gray", plain_cover([128, 128, 128])),
        (
            "half white, half black",
            cover(300, 300, |x, _| if x < 150 { WHITE } else { BLACK }),
        ),
    ] {
        let accent = accent(&shown(&media, &mut device_app, title, cover).await);

        assert_eq!(accent, WHITE, "{title}");
    }
}

#[tokio::test]
async fn the_backdrop_of_a_dark_cover_is_not_darkened_further() {
    let (_daemon, mut device_app, media) = connected().await;
    // Dark enough for white text, and for its Accent on a control.
    let dark_blue = [10, 20, 45];

    let artwork = shown(&media, &mut device_app, "Dark", plain_cover(dark_blue)).await;

    for pixel in each_pixel(&pixels(artwork.backdrop())) {
        for (channel, expected) in pixel.into_iter().zip(dark_blue) {
            assert!(channel.abs_diff(expected) <= 2, "Backdrop pixel {pixel:?}");
        }
    }
}

#[tokio::test]
async fn the_tray_core_sends_artwork_only_when_the_device_asks_for_it() {
    // A stand-in Device daemon, to see everything the Tray core sends.
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let device_address = listener.local_addr().unwrap();
    let media = FakeMediaSource::new();
    media.set(Some(song("Teardrop", Some(plain_cover([200, 30, 40])))));
    let media_for_session = media.clone();
    tokio::spawn(async move {
        let session = Session::connect(device_address).await.unwrap();
        session
            .run(&media_for_session, &FakeVolumeControl::default())
            .await
    });
    let (stream, _) = listener.accept().await.unwrap();
    let (read, write) = stream.into_split();
    let mut reader = MessageReader::<_, TrayMessage>::new(read);
    let mut writer = MessageWriter::<_, DeviceMessage>::new(write);
    Handshake::current(SoftwareVersion::new(0, 1, 0))
        .exchange(&mut reader, &mut writer)
        .await
        .unwrap();
    let mut next = async || {
        tokio::time::timeout(PATIENCE, reader.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Now Playing names the Artwork; the Volume follows; then nothing, for
    // long enough to process the Artwork many times over.
    let mut id = None;
    for _ in 0..2 {
        if let TrayMessage::NowPlaying(Some(now_playing)) = next().await {
            id = now_playing.artwork;
        }
    }
    let id = id.expect("Now Playing names no Artwork");
    let unasked = tokio::time::timeout(Duration::from_millis(500), next()).await;
    assert!(unasked.is_err(), "sent unasked: {unasked:?}");

    writer
        .send(&DeviceMessage::MissingArtwork(id))
        .await
        .unwrap();
    match next().await {
        TrayMessage::Artwork(artwork) => assert_eq!(artwork.id(), id),
        other => panic!("expected the Artwork, got {other:?}"),
    }
}
