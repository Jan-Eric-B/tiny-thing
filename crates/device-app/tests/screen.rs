//! Seam 3 (Slint testing backend): what the screen shows for a state from the
//! Device daemon.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::{AppWindow, screen};
use i_slint_backend_testing::ElementHandle;
use protocol::NowPlaying;
use protocol::local::DaemonMessage;

fn visible_texts(window: &AppWindow, text: &str) -> usize {
    ElementHandle::find_by_accessible_label(window, text).count()
}

#[test]
fn now_playing_shows_title_and_artist_until_nothing_plays() {
    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().unwrap();

    screen::show(
        &window,
        DaemonMessage::NowPlaying(Some(NowPlaying {
            title: "Teardrop".to_owned(),
            artist: "Massive Attack".to_owned(),
        })),
    );
    assert_eq!(visible_texts(&window, "Teardrop"), 1);
    assert_eq!(visible_texts(&window, "Massive Attack"), 1);

    screen::show(&window, DaemonMessage::NowPlaying(None));
    assert_eq!(visible_texts(&window, "Teardrop"), 0);
    assert_eq!(visible_texts(&window, "Massive Attack"), 0);
}
