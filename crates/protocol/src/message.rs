use serde::{Deserialize, Serialize};

/// What the operating system reports as Now Playing, as far as the Device
/// needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playback: PlaybackState,
}

/// Whether Now Playing is playing or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlaybackState {
    Playing,
    /// Anything that is not playing: paused, stopped, or between songs.
    Paused,
}

/// What a user asks for on the Device, whichever input they used (dial,
/// button or touch). The Device app sends it to the Device daemon, which
/// passes it on to the Tray app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputAction {
    /// Play if paused, pause if playing.
    PlayPause,
    /// Go to the previous song.
    Previous,
    /// Go to the next song.
    Next,
}

/// A message from the Tray app to the Device daemon, after the Handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayMessage {
    /// The current Now Playing; `None` when nothing plays.
    NowPlaying(Option<NowPlaying>),
}

/// A message from the Device daemon to the Tray app, after the Handshake.
///
/// Device status follows in later protocol versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceMessage {
    /// The user asked for this on the Device.
    Input(InputAction),
}
