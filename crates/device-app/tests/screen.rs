//! Seam 3 (Slint testing backend): what the screen shows for a state from the
//! Device daemon, and which input actions dial, buttons and touch produce.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::AppWindow;
use device_app::screen::{OPTIMISTIC_HOLD, Screen};
use i_slint_backend_testing::ElementHandle;
use protocol::local::DaemonMessage;
use protocol::{InputAction, NowPlaying, PlaybackState};
use slint::ComponentHandle;
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

    fn report(&self, playback: PlaybackState) {
        self.report_song("Teardrop", playback);
    }

    fn report_song(&self, title: &str, playback: PlaybackState) {
        self.screen.show(DaemonMessage::NowPlaying(Some(NowPlaying {
            title: title.to_owned(),
            artist: "Massive Attack".to_owned(),
            playback,
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
