//! Turns messages from the Device daemon into what the screen shows.

use crate::AppWindow;
use protocol::local::DaemonMessage;

/// Updates `window` for `message`. Must run on the UI thread.
pub fn show(window: &AppWindow, message: DaemonMessage) {
    match message {
        DaemonMessage::NowPlaying(now_playing) => {
            let (title, artist) = now_playing
                .map(|now_playing| (now_playing.title, now_playing.artist))
                .unwrap_or_default();
            window.set_now_playing_title(title.into());
            window.set_now_playing_artist(artist.into());
        }
    }
}
