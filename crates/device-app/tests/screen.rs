//! Seam 3 (Slint testing backend): what the screen shows for a state from the
//! Device daemon, and which input actions dial, buttons and touch produce.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::AppWindow;
use device_app::screen::{OPTIMISTIC_HOLD, Screen, VOLUME_INDICATOR_HOLD};
use i_slint_backend_testing::ElementHandle;
use protocol::local::DaemonMessage;
use protocol::{InputAction, NowPlaying, Percent, PlaybackState, Volume};
use slint::ComponentHandle;
use slint::LogicalPosition;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// A window with its screen, recording every input action it sends.
struct Harness {
    window: AppWindow,
    screen: Screen,
    sent: Rc<RefCell<Vec<InputAction>>>,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        let window = AppWindow::new().unwrap();
        let sent = Rc::new(RefCell::new(Vec::new()));
        let recorder = Rc::clone(&sent);
        let screen = Screen::new(&window, move |action| recorder.borrow_mut().push(action));
        window.show().unwrap();
        Self {
            window,
            screen,
            sent,
        }
    }

    /// Takes the input actions sent so far.
    fn sent(&self) -> Vec<InputAction> {
        self.sent.take()
    }

    fn press_key(&self, text: impl Into<slint::SharedString>) {
        let text = text.into();
        let window = self.window.window();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
    }

    fn tap(&self, label: &str) {
        let button = ElementHandle::find_by_accessible_label(&self.window, label)
            .next()
            .unwrap_or_else(|| panic!("no on-screen control labelled {label:?}"));
        button.mock_single_click(PointerEventButton::Left);
    }

    fn shows(&self, label: &str) -> bool {
        ElementHandle::find_by_accessible_label(&self.window, label)
            .next()
            .is_some()
    }

    /// The level the Volume indicator shows, `None` while it is hidden.
    fn volume_indicator(&self) -> Option<String> {
        ElementHandle::find_by_accessible_label(&self.window, "Volume")
            .next()
            .map(|indicator| indicator.accessible_value().unwrap_or_default().to_string())
    }

    fn scroll(&self, delta_y: f32) {
        self.window
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(400.0, 240.0),
                delta_x: 0.0,
                delta_y,
            });
    }

    fn report_volume(&self, level: u8, muted: bool) {
        self.screen.show(DaemonMessage::Volume(Some(Volume {
            level: Percent::try_from(level).unwrap(),
            muted,
        })));
    }

    fn report(&self, playback: PlaybackState) {
        self.report_song("Teardrop", playback);
    }

    fn report_song(&self, title: &str, playback: PlaybackState) {
        self.screen.show(DaemonMessage::NowPlaying(Some(NowPlaying {
            title: title.to_owned(),
            artist: "Massive Attack".to_owned(),
            playback,
            artwork: None,
            progress: None,
        })));
    }
}

#[test]
fn now_playing_shows_title_and_artist_until_nothing_plays() {
    let harness = Harness::new();

    harness.report(PlaybackState::Playing);
    assert!(harness.shows("Teardrop"));
    assert!(harness.shows("Massive Attack"));

    harness.screen.show(DaemonMessage::NowPlaying(None));
    assert!(!harness.shows("Teardrop"));
    assert!(!harness.shows("Massive Attack"));
}

#[test]
fn the_dial_press_and_buttons_1_to_3_send_their_actions() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    // The Simulator's stand-ins: Enter for the dial press, 1 to 3 for the
    // buttons.
    harness.press_key(Key::Return);
    harness.press_key("1");
    harness.press_key("2");
    harness.press_key("3");

    assert_eq!(
        harness.sent(),
        [
            InputAction::PlayPause,
            InputAction::Previous,
            InputAction::PlayPause,
            InputAction::Next,
        ]
    );
}

#[test]
fn a_held_down_key_acts_once() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);
    let window = harness.window.window();

    window.dispatch_event(WindowEvent::KeyPressed { text: "3".into() });
    window.dispatch_event(WindowEvent::KeyPressRepeated { text: "3".into() });
    window.dispatch_event(WindowEvent::KeyPressRepeated { text: "3".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "3".into() });

    assert_eq!(harness.sent(), [InputAction::Next]);
}

#[test]
fn the_on_screen_controls_send_their_actions() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    harness.tap("Previous");
    harness.tap("Pause");
    harness.tap("Next");

    assert_eq!(
        harness.sent(),
        [
            InputAction::Previous,
            InputAction::PlayPause,
            InputAction::Next,
        ]
    );
}

#[test]
fn a_pressed_control_shrinks_its_circle_until_released() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);
    let control = ElementHandle::find_by_accessible_label(&harness.window, "Next")
        .next()
        .unwrap();
    let circle = || {
        control
            .query_descendants()
            .match_id("ControlButton::circle")
            .find_first()
            .expect("the control has no circle")
            .size()
    };
    let resting = circle();
    let (position, size) = (control.absolute_position(), control.size());
    let center = LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    let window = harness.window.window();

    window.dispatch_event(WindowEvent::PointerMoved { position: center });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: center,
        button: PointerEventButton::Left,
    });
    let pressed = circle();
    window.dispatch_event(WindowEvent::PointerReleased {
        position: center,
        button: PointerEventButton::Left,
    });

    // Clearly smaller, still round, and back to its size once released.
    assert!(
        pressed.width <= resting.width * 0.92,
        "pressed {pressed:?}, resting {resting:?}"
    );
    assert_eq!(pressed.width, pressed.height);
    assert_eq!(circle(), resting);
    // The touch target stays the same.
    assert_eq!(control.size(), size);
}

#[test]
fn no_on_screen_controls_while_nothing_plays() {
    let harness = Harness::new();
    harness.screen.show(DaemonMessage::NowPlaying(None));

    for label in ["Previous", "Play", "Pause", "Next"] {
        assert!(!harness.shows(label), "{label} is shown");
    }
}

#[test]
fn play_pause_shows_the_new_state_at_once() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);
    assert!(harness.shows("Pause"));

    harness.tap("Pause");
    assert!(harness.shows("Play"), "paused before the Tray app answers");

    harness.press_key(Key::Return);
    assert!(harness.shows("Pause"), "playing again");
}

#[test]
fn a_now_playing_with_the_same_playback_keeps_an_optimistic_play_pause() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    harness.tap("Pause");
    // Something else changed before the player paused: here the song, later
    // also the position.
    harness.report_song("Angel", PlaybackState::Playing);

    assert!(harness.shows("Angel"));
    assert!(
        harness.shows("Play"),
        "still waiting for the player to pause"
    );

    i_slint_backend_testing::mock_elapsed_time(OPTIMISTIC_HOLD + Duration::from_millis(100));
    assert!(harness.shows("Pause"), "the player never paused");
}

#[test]
fn a_changed_playback_ends_an_optimistic_play_pause() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    harness.tap("Pause");
    harness.report(PlaybackState::Paused);
    assert!(harness.shows("Play"), "the player paused as asked");

    // Two quick toggles show playing; the player has acted on only the
    // first so far, and that is what the Device shows.
    harness.tap("Play");
    harness.tap("Pause");
    harness.report(PlaybackState::Playing);
    assert!(harness.shows("Pause"));
    harness.report(PlaybackState::Paused);
    assert!(
        harness.shows("Play"),
        "the player acted, the Device follows"
    );
}

#[test]
fn an_optimistic_play_pause_falls_back_when_no_answer_comes() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    harness.tap("Pause");
    i_slint_backend_testing::mock_elapsed_time(OPTIMISTIC_HOLD - Duration::from_millis(100));
    assert!(harness.shows("Play"), "still waiting for the Tray app");

    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(200));
    assert!(harness.shows("Pause"), "back to what the Tray app reported");
}

#[test]
fn each_toggle_waits_the_full_hold_again() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    harness.tap("Pause");
    i_slint_backend_testing::mock_elapsed_time(OPTIMISTIC_HOLD - Duration::from_millis(100));
    harness.tap("Play");
    harness.tap("Pause");
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(200));

    assert!(
        harness.shows("Play"),
        "the second toggle's hold is still running"
    );
}

#[test]
fn turning_the_dial_sends_volume_steps() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing);

    // The Simulator's stand-ins: the right and left arrow keys turn the dial
    // clockwise and counter-clockwise, and so does the mouse wheel.
    harness.press_key(Key::RightArrow);
    harness.press_key(Key::LeftArrow);
    harness.scroll(120.0);
    harness.scroll(-120.0);

    assert_eq!(
        harness.sent(),
        [
            InputAction::VolumeUp,
            InputAction::VolumeDown,
            InputAction::VolumeUp,
            InputAction::VolumeDown,
        ]
    );
}

#[test]
fn holding_a_dial_key_keeps_turning() {
    let harness = Harness::new();
    let window = harness.window.window();

    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::RightArrow.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressRepeated {
        text: Key::RightArrow.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::RightArrow.into(),
    });

    assert_eq!(
        harness.sent(),
        [InputAction::VolumeUp, InputAction::VolumeUp]
    );
}

#[test]
fn turning_the_dial_shows_the_volume_indicator_until_it_rests() {
    let harness = Harness::new();
    harness.report_volume(40, false);
    assert_eq!(harness.volume_indicator(), None, "hidden at first");

    harness.press_key(Key::RightArrow);
    assert_eq!(harness.volume_indicator().as_deref(), Some("40"));
    // The Tray app reports the new level.
    harness.report_volume(42, false);
    assert_eq!(harness.volume_indicator().as_deref(), Some("42"));

    i_slint_backend_testing::mock_elapsed_time(VOLUME_INDICATOR_HOLD - Duration::from_millis(100));
    assert!(harness.volume_indicator().is_some(), "still turning");
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(200));
    assert_eq!(harness.volume_indicator(), None, "hidden again");
}

#[test]
fn each_dial_turn_keeps_the_volume_indicator_up_for_the_full_hold_again() {
    let harness = Harness::new();
    harness.report_volume(40, false);

    harness.press_key(Key::RightArrow);
    i_slint_backend_testing::mock_elapsed_time(VOLUME_INDICATOR_HOLD - Duration::from_millis(100));
    harness.press_key(Key::RightArrow);
    i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(200));

    assert!(
        harness.volume_indicator().is_some(),
        "the second turn's hold is still running"
    );
}

#[test]
fn a_volume_change_on_the_computer_shows_the_volume_indicator_briefly() {
    let harness = Harness::new();
    harness.report_volume(40, false);
    // The same Volume again (a new Tray app session, say) is no change.
    harness.report_volume(40, false);
    assert_eq!(harness.volume_indicator(), None);

    harness.report_volume(30, false);
    assert_eq!(harness.volume_indicator().as_deref(), Some("30"));

    i_slint_backend_testing::mock_elapsed_time(VOLUME_INDICATOR_HOLD + Duration::from_millis(100));
    assert_eq!(harness.volume_indicator(), None);
}

#[test]
fn turning_the_dial_without_a_known_volume_shows_no_indicator() {
    let harness = Harness::new();
    harness.screen.show(DaemonMessage::Volume(None));

    harness.press_key(Key::RightArrow);

    assert_eq!(harness.volume_indicator(), None);
    assert_eq!(harness.sent(), [InputAction::VolumeUp], "still sent");
}

#[test]
fn muted_shows_for_as_long_as_the_computer_is_muted() {
    let harness = Harness::new();
    harness.report_volume(40, false);
    assert!(!harness.shows("Muted"));

    harness.report_volume(40, true);
    assert!(harness.shows("Muted"));
    i_slint_backend_testing::mock_elapsed_time(VOLUME_INDICATOR_HOLD + Duration::from_millis(100));
    assert!(harness.shows("Muted"), "after the Volume indicator is gone");

    harness.report_volume(40, false);
    assert!(!harness.shows("Muted"));
}

#[test]
fn losing_the_output_device_hides_the_volume() {
    let harness = Harness::new();
    harness.report_volume(40, true);
    harness.press_key(Key::RightArrow);

    harness.screen.show(DaemonMessage::Volume(None));

    assert_eq!(harness.volume_indicator(), None);
    assert!(!harness.shows("Muted"));
}
