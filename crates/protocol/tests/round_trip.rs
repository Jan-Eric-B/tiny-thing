//! Every message survives the trip through framing and postcard unchanged.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use protocol::local::{AppMessage, DaemonMessage};
use protocol::{
    Artwork, ArtworkId, DeviceMessage, Handshake, Image, InputAction, MessageReader, MessageWriter,
    NowPlaying, PROTOCOL_VERSION, Percent, PlaybackState, ProtocolVersion, Rgb, RgbPixels,
    SoftwareVersion, TrayMessage, Volume,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

fn percent(value: u8) -> Percent {
    Percent::try_from(value).unwrap()
}

fn now_playing(title: &str, artist: &str) -> NowPlaying {
    NowPlaying {
        title: title.to_owned(),
        artist: artist.to_owned(),
        playback: PlaybackState::Playing,
        artwork: Some(ArtworkId(0x0123_4567_89ab_cdef)),
    }
}

/// An image of one colour.
fn image(width: u32, height: u32, colour: [u8; 3]) -> Image {
    Image::encode(&RgbPixels {
        width,
        height,
        rgb: colour.repeat((width * height) as usize),
    })
    .unwrap()
}

fn artwork(id: u64) -> Artwork {
    Artwork::new(
        ArtworkId(id),
        image(352, 198, [200, 30, 40]),
        image(800, 480, [20, 3, 4]),
        Rgb {
            r: 255,
            g: 140,
            b: 150,
        },
    )
    .unwrap()
}

/// Every input action. The exhaustive match makes adding an action without
/// listing it here a compile error.
fn all_input_actions() -> Vec<InputAction> {
    fn covered(action: InputAction) {
        match action {
            InputAction::PlayPause
            | InputAction::Previous
            | InputAction::Next
            | InputAction::VolumeUp
            | InputAction::VolumeDown => {}
        }
    }
    let actions = vec![
        InputAction::PlayPause,
        InputAction::Previous,
        InputAction::Next,
        InputAction::VolumeUp,
        InputAction::VolumeDown,
    ];
    actions.iter().copied().for_each(covered);
    actions
}

/// One sample per variant, plus edge cases. The exhaustive match makes adding a
/// variant without a sample a compile error.
fn tray_message_samples() -> Vec<TrayMessage> {
    fn covered(message: &TrayMessage) {
        match message {
            TrayMessage::NowPlaying(_)
            | TrayMessage::Volume(_)
            | TrayMessage::Artwork(_)
            | TrayMessage::ArtworkUnavailable(_) => {}
        }
    }
    let samples = vec![
        TrayMessage::NowPlaying(Some(now_playing("Teardrop", "Massive Attack"))),
        TrayMessage::NowPlaying(Some(now_playing("", ""))),
        TrayMessage::NowPlaying(Some(now_playing(
            "Sigur Rós - Hoppípolla \u{1F3B5}",
            "日本語のアーティスト",
        ))),
        TrayMessage::NowPlaying(Some(now_playing(&"long ".repeat(10_000), "x"))),
        TrayMessage::NowPlaying(Some(NowPlaying {
            playback: PlaybackState::Paused,
            ..now_playing("Teardrop", "Massive Attack")
        })),
        TrayMessage::NowPlaying(Some(NowPlaying {
            artwork: None,
            ..now_playing("Teardrop", "Massive Attack")
        })),
        TrayMessage::NowPlaying(None),
        TrayMessage::Artwork(artwork(1)),
        TrayMessage::Artwork(artwork(u64::MAX)),
        TrayMessage::ArtworkUnavailable(ArtworkId(3)),
        TrayMessage::Volume(Some(Volume {
            level: percent(0),
            muted: false,
        })),
        TrayMessage::Volume(Some(Volume {
            level: percent(100),
            muted: true,
        })),
        TrayMessage::Volume(None),
    ];
    samples.iter().for_each(covered);
    samples
}

fn device_message_samples() -> Vec<DeviceMessage> {
    fn covered(message: &DeviceMessage) {
        match message {
            DeviceMessage::Input(_) | DeviceMessage::MissingArtwork(_) => {}
        }
    }
    let mut samples: Vec<DeviceMessage> = all_input_actions()
        .into_iter()
        .map(DeviceMessage::Input)
        .collect();
    samples.push(DeviceMessage::MissingArtwork(ArtworkId(42)));
    samples.iter().for_each(covered);
    samples
}

fn daemon_message_samples() -> Vec<DaemonMessage> {
    fn covered(message: &DaemonMessage) {
        match message {
            DaemonMessage::NowPlaying(_)
            | DaemonMessage::Volume(_)
            | DaemonMessage::Artwork(_)
            | DaemonMessage::ArtworkUnavailable(_) => {}
        }
    }
    let samples = vec![
        DaemonMessage::NowPlaying(Some(now_playing("Windowlicker", "Aphex Twin"))),
        DaemonMessage::NowPlaying(Some(NowPlaying {
            playback: PlaybackState::Paused,
            ..now_playing("Windowlicker", "Aphex Twin")
        })),
        DaemonMessage::NowPlaying(None),
        DaemonMessage::Artwork(artwork(7)),
        DaemonMessage::ArtworkUnavailable(ArtworkId(8)),
        DaemonMessage::Volume(Some(Volume {
            level: percent(42),
            muted: true,
        })),
        DaemonMessage::Volume(None),
    ];
    samples.iter().for_each(covered);
    samples
}

fn app_message_samples() -> Vec<AppMessage> {
    fn covered(message: &AppMessage) {
        match message {
            AppMessage::Input(_) => {}
        }
    }
    let samples: Vec<AppMessage> = all_input_actions()
        .into_iter()
        .map(AppMessage::Input)
        .collect();
    samples.iter().for_each(covered);
    samples
}

async fn assert_round_trip<M>(samples: Vec<M>)
where
    M: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let (near, far) = tokio::io::duplex(64 * 1024);
    let mut writer = MessageWriter::<_, M>::new(near);
    let mut reader = MessageReader::<_, M>::new(far);

    let send_all = async {
        for message in &samples {
            writer.send(message).await.unwrap();
        }
        // Closing the stream ends the receiving loop.
        drop(writer);
    };
    let receive_all = async {
        let mut received = Vec::new();
        while let Some(message) = reader.recv().await.unwrap() {
            received.push(message);
        }
        received
    };

    let ((), received) = tokio::join!(send_all, receive_all);
    assert_eq!(received, samples);
}

#[tokio::test]
async fn tray_messages_round_trip() {
    assert_round_trip(tray_message_samples()).await;
}

#[tokio::test]
async fn device_messages_round_trip() {
    assert_round_trip(device_message_samples()).await;
}

#[tokio::test]
async fn daemon_messages_round_trip() {
    assert_round_trip(daemon_message_samples()).await;
}

#[tokio::test]
async fn app_messages_round_trip() {
    assert_round_trip(app_message_samples()).await;
}

#[tokio::test]
async fn both_sides_learn_the_other_sides_handshake() {
    let tray = Handshake {
        protocol_version: PROTOCOL_VERSION,
        software_version: SoftwareVersion::new(1, 2, 3),
    };
    let device = Handshake {
        protocol_version: PROTOCOL_VERSION,
        software_version: SoftwareVersion::new(4, 5, 6),
    };
    let (tray_stream, device_stream) = tokio::io::duplex(1024);
    let (tray_read, tray_write) = tokio::io::split(tray_stream);
    let (device_read, device_write) = tokio::io::split(device_stream);
    let mut tray_reader = MessageReader::<_, DeviceMessage>::new(tray_read);
    let mut tray_writer = MessageWriter::<_, TrayMessage>::new(tray_write);
    let mut device_reader = MessageReader::<_, TrayMessage>::new(device_read);
    let mut device_writer = MessageWriter::<_, DeviceMessage>::new(device_write);

    let (seen_by_tray, seen_by_device) = tokio::join!(
        tray.exchange(&mut tray_reader, &mut tray_writer),
        device.exchange(&mut device_reader, &mut device_writer),
    );

    assert_eq!(seen_by_tray.unwrap(), device);
    assert_eq!(seen_by_device.unwrap(), tray);
}

#[test]
fn the_handshake_encoding_keeps_the_protocol_version_first() {
    // Pinned bytes: a peer of any release reads the protocol version from the
    // front of the Handshake, so reordering its fields breaks every mismatch
    // report. Each number is one postcard varint byte.
    let handshake = Handshake {
        protocol_version: ProtocolVersion(7),
        software_version: SoftwareVersion::new(1, 2, 3),
    };
    assert_eq!(postcard::to_allocvec(&handshake).unwrap(), [7, 1, 2, 3]);
}

#[test]
fn software_version_displays_as_major_minor_patch() {
    assert_eq!(SoftwareVersion::new(0, 12, 345).to_string(), "0.12.345");
}

#[test]
fn every_percent_survives_the_trip_through_postcard() {
    for value in 0..=100 {
        let level = percent(value);
        let bytes = postcard::to_allocvec(&level).unwrap();
        assert_eq!(postcard::from_bytes::<Percent>(&bytes).unwrap(), level);
    }
}

#[test]
fn a_percent_above_100_is_rejected() {
    assert!(Percent::try_from(101).is_err());
    assert!(Percent::try_from(u8::MAX).is_err());
}

#[test]
fn a_volume_level_above_100_from_the_wire_is_rejected() {
    /// `Volume` as a peer without the 0-100 check would encode it.
    #[derive(Serialize)]
    struct UncheckedVolume {
        level: u8,
        muted: bool,
    }
    let encode = |level| {
        postcard::to_allocvec(&UncheckedVolume {
            level,
            muted: false,
        })
        .unwrap()
    };

    assert_eq!(
        postcard::from_bytes::<Volume>(&encode(100)).unwrap(),
        Volume {
            level: percent(100),
            muted: false,
        },
    );
    assert!(postcard::from_bytes::<Volume>(&encode(101)).is_err());
}
