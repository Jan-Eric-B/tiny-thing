//! Seam 3 (Slint testing backend), with a fake clock: the progress of Now
//! Playing moves on by itself while playing and stays still while paused,
//! also while a play/pause toggled on the Device waits for the player, and a
//! tap on the progress bar seeks there.

// Test code fails loudly on purpose.
#![allow(clippy::unwrap_used)]

use device_app::AppWindow;
use device_app::screen::{OPTIMISTIC_HOLD, Screen};
use i_slint_backend_testing::ElementHandle;
use protocol::local::DaemonMessage;
use protocol::{InputAction, MAX_DURATION, NowPlaying, PlaybackRate, PlaybackState, Progress};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// "Teardrop" is 5:31 long.
const TEARDROP: Duration = Duration::from_secs(331);

/// `position` seconds into a song of `duration`, at normal speed, which can
/// seek.
fn progress(position: u64, duration: Duration) -> Progress {
    Progress::new(
        Duration::from_secs(position),
        duration,
        PlaybackRate::NORMAL,
        true,
    )
    .unwrap()
}

/// A window with its screen on a fake clock, recording every input action
/// it sends.
struct Harness {
    window: AppWindow,
    screen: Screen,
    sent: Rc<RefCell<Vec<InputAction>>>,
    /// How far the fake clock has moved.
    elapsed: Rc<Cell<Duration>>,
}

impl Harness {
    fn new() -> Self {
        i_slint_backend_testing::init_no_event_loop();
        let window = AppWindow::new().unwrap();
        let sent = Rc::new(RefCell::new(Vec::new()));
        let recorder = Rc::clone(&sent);
        let elapsed = Rc::new(Cell::new(Duration::ZERO));
        let start = Instant::now();
        let clock = Rc::clone(&elapsed);
        let screen = Screen::with_clock(
            &window,
            move |action| recorder.borrow_mut().push(action),
            move || start + clock.get(),
        );
        window.show().unwrap();
        Self {
            window,
            screen,
            sent,
            elapsed,
        }
    }

    /// Moves the fake clock and Slint's timers on by `duration`.
    fn advance(&self, duration: Duration) {
        self.elapsed.set(self.elapsed.get() + duration);
        i_slint_backend_testing::mock_elapsed_time(duration);
    }

    fn sent(&self) -> Vec<InputAction> {
        self.sent.take()
    }

    /// Reports "Teardrop" at `position` seconds, which can seek.
    fn report(&self, playback: PlaybackState, position: u64) {
        self.report_progress(playback, Some(progress(position, TEARDROP)));
    }

    fn report_progress(&self, playback: PlaybackState, progress: Option<Progress>) {
        self.report_song("Teardrop", playback, progress);
    }

    fn report_song(&self, title: &str, playback: PlaybackState, progress: Option<Progress>) {
        self.screen.show(DaemonMessage::NowPlaying(Some(NowPlaying {
            title: title.to_owned(),
            artist: "Massive Attack".to_owned(),
            playback,
            artwork: None,
            progress,
        })));
    }

    fn bar(&self) -> Option<ElementHandle> {
        ElementHandle::find_by_accessible_label(&self.window, "Progress").next()
    }

    /// The position the progress bar shows, as on screen; `None` while
    /// there is no progress bar.
    fn position(&self) -> Option<String> {
        self.bar()
            .map(|bar| bar.accessible_value().unwrap_or_default().to_string())
    }

    /// How much of the bar is filled, from 0 to 1.
    fn filled(&self) -> f32 {
        self.window.get_progress()
    }

    fn shows(&self, text: &str) -> bool {
        ElementHandle::find_by_accessible_label(&self.window, text)
            .next()
            .is_some()
    }

    /// Taps the progress bar at `fraction` of its width.
    fn tap_bar(&self, fraction: f32) {
        let bar = self.bar().expect("no progress bar");
        let (position, size) = (bar.absolute_position(), bar.size());
        let point = LogicalPosition::new(
            position.x + size.width * fraction,
            position.y + size.height / 2.0,
        );
        let window = self.window.window();
        window.dispatch_event(WindowEvent::PointerMoved { position: point });
        window.dispatch_event(WindowEvent::PointerPressed {
            position: point,
            button: PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerReleased {
            position: point,
            button: PointerEventButton::Left,
        });
    }

    fn tap(&self, label: &str) {
        ElementHandle::find_by_accessible_label(&self.window, label)
            .next()
            .unwrap_or_else(|| panic!("no on-screen control labelled {label:?}"))
            .mock_single_click(PointerEventButton::Left);
    }
}

#[test]
fn position_and_duration_show_with_now_playing() {
    let harness = Harness::new();

    harness.report(PlaybackState::Paused, 40);

    assert_eq!(harness.position().as_deref(), Some("0:40"));
    assert!(harness.shows("0:40"));
    assert!(harness.shows("5:31"));
    assert!((harness.filled() - 40.0 / 331.0).abs() < 1e-4);
}

#[test]
fn the_progress_moves_on_by_itself_while_playing() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    // Smoothly: the bar moves within a second, not only when the shown
    // second changes.
    harness.advance(Duration::from_millis(500));
    assert_eq!(harness.position().as_deref(), Some("0:40"));
    assert!((harness.filled() - 40.5 / 331.0).abs() < 1e-3);

    harness.advance(Duration::from_millis(1600));
    assert_eq!(harness.position().as_deref(), Some("0:42"));
}

#[test]
fn the_progress_stays_still_while_paused() {
    let harness = Harness::new();
    harness.report(PlaybackState::Paused, 40);
    let filled = harness.filled();

    harness.advance(Duration::from_secs(10));

    assert_eq!(harness.position().as_deref(), Some("0:40"));
    assert!((harness.filled() - filled).abs() < f32::EPSILON);
}

#[test]
fn the_progress_stops_at_the_end_of_the_song() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 329);

    harness.advance(Duration::from_secs(10));

    assert_eq!(harness.position().as_deref(), Some("5:31"));
    assert!((harness.filled() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn a_new_position_from_the_tray_app_replaces_the_one_moved_on_to() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);
    harness.advance(Duration::from_secs(5));

    harness.report(PlaybackState::Playing, 120);

    assert_eq!(harness.position().as_deref(), Some("2:00"));
}

#[test]
fn a_pause_toggled_on_the_device_stops_the_progress_at_once() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    harness.tap("Pause");
    harness.advance(Duration::from_secs(2));

    assert_eq!(harness.position().as_deref(), Some("0:40"));
}

#[test]
fn a_play_toggled_on_the_device_moves_the_progress_at_once() {
    let harness = Harness::new();
    harness.report(PlaybackState::Paused, 40);
    harness.advance(Duration::from_secs(5));

    harness.tap("Play");
    harness.advance(Duration::from_secs(2));

    // From where it was paused.
    assert_eq!(harness.position().as_deref(), Some("0:42"));
}

#[test]
fn a_pause_the_player_ignored_shows_where_the_song_has_got_to() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    harness.tap("Pause");
    harness.advance(OPTIMISTIC_HOLD + Duration::from_millis(100));

    // The player played on all along.
    assert_eq!(harness.position().as_deref(), Some("0:43"));
    harness.advance(Duration::from_secs(1));
    assert_eq!(harness.position().as_deref(), Some("0:44"));
}

#[test]
fn nothing_playing_shows_no_progress_bar() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    harness.screen.show(DaemonMessage::NowPlaying(None));

    assert_eq!(harness.position(), None);
    assert!(!harness.shows("5:31"));
}

#[test]
fn a_song_without_a_duration_shows_no_progress_bar() {
    let harness = Harness::new();

    // A live stream.
    harness.report_progress(PlaybackState::Playing, None);

    assert!(harness.shows("Teardrop"));
    assert_eq!(harness.position(), None);
}

#[test]
fn long_media_shows_hours() {
    let harness = Harness::new();

    harness.report_progress(
        PlaybackState::Paused,
        Some(progress(3903, Duration::from_secs(7200))),
    );

    assert_eq!(harness.position().as_deref(), Some("1:05:03"));
    assert!(harness.shows("2:00:00"));
}

#[test]
fn tapping_the_progress_bar_seeks_to_the_matching_position() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    harness.tap_bar(0.25);

    assert_eq!(
        harness.sent(),
        [InputAction::Seek(Duration::from_millis(82_750))]
    );
    // Shown at once, and moving on from there.
    assert_eq!(harness.position().as_deref(), Some("1:22"));
    harness.advance(Duration::from_secs(1));
    assert_eq!(harness.position().as_deref(), Some("1:23"));
}

#[test]
fn a_media_session_that_cannot_seek_ignores_a_tap_on_the_progress_bar() {
    let harness = Harness::new();
    harness.report_progress(
        PlaybackState::Paused,
        Some(
            Progress::new(
                Duration::from_secs(40),
                TEARDROP,
                PlaybackRate::NORMAL,
                false,
            )
            .unwrap(),
        ),
    );

    harness.tap_bar(0.25);

    assert_eq!(harness.sent(), []);
    assert_eq!(harness.position().as_deref(), Some("0:40"));
}

#[test]
fn the_next_position_from_the_tray_app_corrects_a_seek() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);
    harness.tap_bar(0.25);

    // The player went to a slightly different place.
    harness.report(PlaybackState::Playing, 80);

    assert_eq!(harness.position().as_deref(), Some("1:20"));
}

#[test]
fn a_now_playing_from_before_the_player_seeked_keeps_the_seek_shown() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);
    harness.tap_bar(0.25);

    // Something else changed first, with the position from before the seek.
    harness.advance(Duration::from_secs(1));
    harness.report(PlaybackState::Playing, 41);
    assert_eq!(harness.position().as_deref(), Some("1:23"));

    // The player never seeked: back to where it plays.
    harness.advance(OPTIMISTIC_HOLD);
    assert_eq!(harness.position().as_deref(), Some("0:44"));
}

#[test]
fn the_progress_moves_at_the_players_rate() {
    let harness = Harness::new();
    let podcast = Progress::new(
        Duration::from_secs(40),
        TEARDROP,
        PlaybackRate::from_f64(1.5).unwrap(),
        true,
    )
    .unwrap();
    harness.report_progress(PlaybackState::Playing, Some(podcast));

    harness.advance(Duration::from_secs(10));

    assert_eq!(harness.position().as_deref(), Some("0:55"));
}

#[test]
fn a_now_playing_from_before_the_player_followed_a_play_keeps_the_progress_moving() {
    let harness = Harness::new();
    harness.report(PlaybackState::Paused, 40);
    harness.tap("Play");
    harness.advance(Duration::from_millis(1500));

    // Something else changed first, still paused at the same position.
    harness.report(PlaybackState::Paused, 40);

    assert!((harness.filled() - 41.5 / 331.0).abs() < 1e-3);
    harness.advance(Duration::from_secs(1));
    assert_eq!(harness.position().as_deref(), Some("0:42"));
}

#[test]
fn a_seek_and_a_pause_on_the_device_both_show_at_once() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    harness.tap_bar(0.25);
    harness.tap("Pause");
    harness.advance(Duration::from_secs(2));
    assert_eq!(harness.position().as_deref(), Some("1:22"));

    // Something else changed first, with the position and playback from
    // before.
    harness.report(PlaybackState::Playing, 42);
    assert_eq!(harness.position().as_deref(), Some("1:22"));

    // The player did both.
    harness.report(PlaybackState::Paused, 83);
    harness.advance(Duration::from_secs(2));
    assert_eq!(harness.position().as_deref(), Some("1:23"));
}

#[test]
fn a_new_song_while_a_seek_waits_shows_the_new_songs_position() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);
    harness.tap_bar(0.25);
    harness.advance(Duration::from_secs(1));

    // Just as long, and by chance about where the old song would be.
    harness.report_song(
        "Angel",
        PlaybackState::Playing,
        Some(progress(41, TEARDROP)),
    );

    assert_eq!(harness.position().as_deref(), Some("0:41"));
}

#[test]
fn a_tap_that_is_not_a_number_seeks_nowhere() {
    let harness = Harness::new();
    harness.report(PlaybackState::Playing, 40);

    // What a bar of no width reports.
    harness.window.invoke_seek(f32::NAN);

    assert_eq!(harness.sent(), []);
    assert_eq!(harness.position().as_deref(), Some("0:40"));
}

#[test]
fn a_tap_at_the_end_of_the_longest_song_seeks_to_its_end() {
    let harness = Harness::new();
    harness.report_progress(PlaybackState::Paused, Some(progress(0, MAX_DURATION)));

    harness.window.invoke_seek(1.0);

    assert_eq!(harness.sent(), [InputAction::Seek(MAX_DURATION)]);
}

#[test]
fn the_progress_timer_runs_only_while_the_bar_moves() {
    let harness = Harness::new();

    harness.report(PlaybackState::Playing, 40);
    assert!(harness.screen.ticking());

    harness.report(PlaybackState::Paused, 40);
    assert!(!harness.screen.ticking());

    harness.report(PlaybackState::Playing, 329);
    harness.advance(Duration::from_secs(3));
    assert!(!harness.screen.ticking(), "still ticking at the end");

    harness.report(PlaybackState::Playing, 40);
    harness.screen.show(DaemonMessage::NowPlaying(None));
    assert!(
        !harness.screen.ticking(),
        "still ticking with nothing playing"
    );
}
