//! Seam 3 (Slint testing backend): the Artwork of Now Playing beside white
//! text on its Backdrop, each image drawn at its own pixel size, and the
//! song shown without Artwork when it cannot be had.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::screen::Screen;
use device_app::{AppWindow, Theme};
use i_slint_backend_testing::ElementHandle;
use protocol::local::DaemonMessage;
use protocol::{
    ARTWORK_BOX, Artwork, ArtworkId, CONTROL_CIRCLE_WHITE_ALPHA, Image, NowPlaying, PlaybackState,
    Rgb, RgbPixels, SCREEN_HEIGHT, SCREEN_WIDTH,
};
use slint::{Color, ComponentHandle, LogicalPosition, LogicalSize};

struct Harness {
    window: AppWindow,
    screen: Screen,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        let window = AppWindow::new().unwrap();
        let screen = Screen::new(&window, |_| {});
        window.show().unwrap();
        Self { window, screen }
    }

    fn report(&self, title: &str, artwork: Option<u64>) {
        self.screen.show(DaemonMessage::NowPlaying(Some(NowPlaying {
            title: title.to_owned(),
            artist: "Massive Attack".to_owned(),
            playback: PlaybackState::Playing,
            artwork: artwork.map(ArtworkId),
            progress: None,
        })));
    }

    fn send_artwork(&self, id: u64, (width, height): (u32, u32)) {
        self.screen
            .show(DaemonMessage::Artwork(artwork(id, (width, height))));
    }

    fn send_unavailable(&self, id: u64) {
        self.screen
            .show(DaemonMessage::ArtworkUnavailable(ArtworkId(id)));
    }

    fn element(&self, label: &str) -> Option<ElementHandle> {
        ElementHandle::find_by_accessible_label(&self.window, label).next()
    }

    fn by_id(&self, id: &str) -> Option<ElementHandle> {
        ElementHandle::find_by_element_id(&self.window, &format!("AppWindow::{id}")).next()
    }

    /// The Artwork's place on screen; `None` while it is not shown.
    fn artwork(&self) -> Option<Rect> {
        self.element("Artwork").map(|element| Rect::of(&element))
    }
}

fn image((width, height): (u32, u32), color: [u8; 3]) -> Image {
    Image::encode(&RgbPixels {
        width,
        height,
        rgb: color.repeat((width * height) as usize),
    })
    .unwrap()
}

fn artwork(id: u64, size: (u32, u32)) -> Artwork {
    Artwork::new(
        ArtworkId(id),
        image(size, [200, 30, 40]),
        image((SCREEN_WIDTH, SCREEN_HEIGHT), [60, 10, 12]),
        Rgb {
            r: 255,
            g: 200,
            b: 200,
        },
    )
    .unwrap()
}

/// An element's place on screen, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    position: LogicalPosition,
    size: LogicalSize,
}

impl Rect {
    fn of(element: &ElementHandle) -> Self {
        Self {
            position: element.absolute_position(),
            size: element.size(),
        }
    }

    fn overlaps(&self, other: &Rect) -> bool {
        self.position.x < other.position.x + other.size.width
            && other.position.x < self.position.x + self.size.width
            && self.position.y < other.position.y + other.size.height
            && other.position.y < self.position.y + self.size.height
    }
}

#[test]
fn the_artwork_is_drawn_at_its_own_pixel_size() {
    let harness = Harness::new();
    for (id, size) in (1..).zip([(352, 352), (352, 198), (198, 352), (200, 200)]) {
        harness.report("Teardrop", Some(id));

        harness.send_artwork(id, size);

        let shown = harness.artwork().expect("no Artwork shown");
        assert_eq!(
            shown.size,
            LogicalSize::new(size.0 as f32, size.1 as f32),
            "Artwork of {size:?}"
        );
    }
}

#[test]
fn the_backdrop_fills_the_screen_at_its_own_pixel_size() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(1));

    harness.send_artwork(1, (352, 352));

    let backdrop = harness.by_id("backdrop").expect("no Backdrop shown");
    assert_eq!(
        Rect::of(&backdrop),
        Rect {
            position: LogicalPosition::new(0.0, 0.0),
            size: LogicalSize::new(SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32),
        }
    );
}

#[test]
fn the_artwork_box_is_the_size_the_tray_app_scales_artwork_to() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(1));

    let artwork_box = harness.by_id("artwork-box").expect("no Artwork box");

    let side = ARTWORK_BOX as f32;
    assert_eq!(artwork_box.size(), LogicalSize::new(side, side));
}

#[test]
fn text_is_never_drawn_over_the_artwork() {
    let long_title = "An Extraordinarily Long Title That Goes On And On ".repeat(4);
    let harness = Harness::new();
    // Every size an Artwork can have lies within these; `Artwork::new` and
    // reading one from a message refuse any larger.
    let largest = [
        (ARTWORK_BOX, ARTWORK_BOX),
        (ARTWORK_BOX, 1),
        (1, ARTWORK_BOX),
        (352, 198),
    ];
    for (id, size) in (1..).zip(largest) {
        harness.report(&long_title, Some(id));
        harness.send_artwork(id, size);
        let artwork = harness.artwork().expect("no Artwork shown");

        for text in [long_title.as_str(), "Massive Attack"] {
            let text = Rect::of(&harness.element(text).unwrap());
            assert!(
                !text.overlaps(&artwork),
                "{text:?} overlaps the Artwork {artwork:?} of {size:?}"
            );
        }
    }
}

#[test]
fn artwork_that_arrives_after_its_now_playing_is_shown() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(1));
    assert_eq!(harness.artwork(), None);

    harness.send_artwork(1, (352, 352));

    assert!(harness.artwork().is_some());
}

#[test]
fn artwork_shows_only_with_the_now_playing_that_names_its_id() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(1));
    harness.send_artwork(1, (352, 352));

    // The next song's Artwork is still on its way.
    harness.report("Angel", Some(2));
    assert_eq!(harness.artwork(), None);
    assert!(harness.by_id("backdrop").is_none());

    harness.send_artwork(2, (352, 198));
    assert_eq!(
        harness.artwork().map(|artwork| artwork.size),
        Some(LogicalSize::new(352.0, 198.0))
    );
}

#[test]
fn artwork_kept_from_before_shows_again_when_its_song_comes_back() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(1));
    harness.send_artwork(1, (352, 352));
    harness.report("Lecture 3", None);
    assert_eq!(harness.artwork(), None);

    // The Device daemon sends no Artwork the Device app has already.
    harness.report("Teardrop", Some(1));

    assert!(harness.artwork().is_some());
}

#[test]
fn now_playing_without_artwork_shows_no_artwork_box() {
    let harness = Harness::new();

    harness.report("Lecture 3", None);

    assert!(harness.by_id("artwork-box").is_none());
    assert!(harness.element("Lecture 3").is_some());
}

#[test]
fn a_song_whose_artwork_cannot_be_had_shows_as_having_none() {
    let harness = Harness::new();
    harness.report("Lecture 3", None);
    let title_without_artwork = Rect::of(&harness.element("Lecture 3").unwrap());
    harness.report("Broken", Some(1));
    assert!(
        harness.by_id("artwork-box").is_some(),
        "no box while it is on its way"
    );

    harness.send_unavailable(1);

    assert!(harness.by_id("artwork-box").is_none());
    let title = Rect::of(&harness.element("Broken").unwrap());
    assert_eq!(
        title.position.x, title_without_artwork.position.x,
        "the text does not take the full width"
    );
}

#[test]
fn word_about_other_artwork_leaves_the_box_waiting() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(2));

    // Late word about the previous song's Artwork.
    harness.send_unavailable(1);

    assert!(harness.by_id("artwork-box").is_some());
}

#[test]
fn a_song_whose_artwork_could_not_be_had_shows_it_when_it_arrives_after_all() {
    let harness = Harness::new();
    harness.report("Teardrop", Some(1));
    harness.send_unavailable(1);

    harness.send_artwork(1, (352, 352));

    assert!(harness.artwork().is_some());
}

#[test]
fn the_control_circle_tint_is_the_one_the_accent_is_chosen_for() {
    let harness = Harness::new();

    let tint = harness.window.global::<Theme>().get_control_circle_tint();

    assert_eq!(
        tint,
        Color::from_argb_u8(CONTROL_CIRCLE_WHITE_ALPHA, 255, 255, 255)
    );
}
