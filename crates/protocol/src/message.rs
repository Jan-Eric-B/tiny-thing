use crate::{Artwork, ArtworkId, Percent, Progress};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// What the operating system reports as Now Playing, as far as the Device
/// needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playback: PlaybackState,
    /// The Artwork that goes with it; `None` when the Media session supplies
    /// none. The Device asks for Artwork it does not have
    /// ([`DeviceMessage::MissingArtwork`]).
    pub artwork: Option<ArtworkId>,
    /// Where playback is in the song; `None` when the Media session reports
    /// no duration (a live stream, say), and the Device shows no progress.
    pub progress: Option<Progress>,
}

impl NowPlaying {
    /// Now Playing as it is `elapsed` after this one was sent: the position
    /// moved on while playing ([`Progress::position_after`]).
    #[must_use]
    pub fn aged(mut self, elapsed: Duration) -> Self {
        self.progress = self
            .progress
            .map(|progress| progress.after(elapsed, self.playback));
        self
    }
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
    /// Turn the Volume up by one step (one click of the dial clockwise).
    /// The Tray app decides how large a step is.
    VolumeUp,
    /// Turn the Volume down by one step (one click of the dial
    /// counter-clockwise).
    VolumeDown,
    /// Go to this position in the song. Absolute, so a seek lands where the
    /// user tapped however far the song has moved on meanwhile, and a seek
    /// that arrives twice lands in the same place.
    Seek(Duration),
}

/// The computer's Volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Volume {
    /// The level.
    pub level: Percent,
    /// Whether the computer is muted. The level stays what it was.
    pub muted: bool,
}

/// A message from the Tray app to the Device daemon, after the Handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrayMessage {
    /// The current Now Playing; `None` when nothing plays.
    NowPlaying(Option<NowPlaying>),
    /// The current Volume; `None` when the computer has no output device.
    Volume(Option<Volume>),
    /// The Artwork the Device asked for. Sent only on request, so an image
    /// the Device keeps is not sent again.
    Artwork(Artwork),
    /// The Artwork the Device asked for cannot be made (the image the Media
    /// session supplied cannot be read). The Device shows the song without
    /// Artwork rather than waiting for it.
    ArtworkUnavailable(ArtworkId),
}

/// A message from the Device daemon to the Tray app, after the Handshake.
///
/// Device status follows in later protocol versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceMessage {
    /// The user asked for this on the Device.
    Input(InputAction),
    /// The Device does not have the Artwork of the current Now Playing.
    MissingArtwork(ArtworkId),
}
