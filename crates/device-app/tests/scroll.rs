//! Seam 3 (Slint testing backend): in the Simulator the mouse wheel turns the
//! dial one click per wheel notch, however finely a touchpad or a
//! smooth-scrolling mouse splits the notch up.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::AppWindow;
use device_app::screen::Screen;
use protocol::InputAction;
use slint::ComponentHandle;
use slint::LogicalPosition;
use slint::platform::WindowEvent;
use std::cell::RefCell;
use std::rc::Rc;

/// What the winit backend makes of one wheel notch, in logical pixels.
const NOTCH: f32 = 60.0;

/// Scrolls `notches` wheel notches (negative is towards you) in `events`
/// equal scroll events, and returns the input actions they sent.
fn scroll(notches: f32, events: u16) -> Vec<InputAction> {
    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().unwrap();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let recorder = Rc::clone(&sent);
    let _screen = Screen::new(&window, move |action| recorder.borrow_mut().push(action));
    window.show().unwrap();

    let delta_y = notches * NOTCH / f32::from(events);
    for _ in 0..events {
        window
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: LogicalPosition::new(400.0, 240.0),
                delta_x: 0.0,
                delta_y,
            });
    }
    sent.take()
}

// Each test scrolls once: the testing backend can be set up only once per
// thread, and every test runs on a thread of its own.

#[test]
fn a_notch_away_from_you_split_into_small_scroll_events_is_one_volume_step_up() {
    assert_eq!(scroll(1.0, 10), [InputAction::VolumeUp]);
}

#[test]
fn a_notch_towards_you_split_into_small_scroll_events_is_one_volume_step_down() {
    assert_eq!(scroll(-1.0, 10), [InputAction::VolumeDown]);
}

#[test]
fn several_notches_of_small_scroll_events_are_as_many_volume_steps() {
    assert_eq!(scroll(3.0, 30), [InputAction::VolumeUp; 3]);
}

#[test]
fn less_than_a_notch_is_no_volume_step() {
    assert!(scroll(0.9, 9).is_empty());
}
