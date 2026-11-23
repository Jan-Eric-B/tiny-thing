//! Which play/pause state the Device shows: the Tray app's word, except right
//! after the user toggled it on the Device.

use protocol::PlaybackState;

/// The play/pause state to show.
///
/// A toggle on the Device is shown at once (optimistically), before the Tray
/// app has carried it out. It ends when the Tray app reports a playback
/// different from the one it reported at the toggle, which means the player
/// acted: from then on the Device shows what the Tray app reports. A Now
/// Playing whose playback is unchanged (only the song changed, say) does not
/// end it. When the player does not act (it ignored the command, or no Tray
/// app is connected), the owner calls [`expire`](Self::expire) after a while
/// and the last reported state shows again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayPause {
    reported: PlaybackState,
    optimistic: Option<PlaybackState>,
}

impl PlayPause {
    /// Nothing reported yet: shown as paused.
    pub const fn new() -> Self {
        Self {
            reported: PlaybackState::Paused,
            optimistic: None,
        }
    }

    /// The state to show now.
    pub fn shown(&self) -> PlaybackState {
        self.optimistic.unwrap_or(self.reported)
    }

    /// The user toggled play/pause on the Device.
    pub fn toggle(&mut self) {
        self.optimistic = Some(toggled(self.shown()));
    }

    /// The Tray app reported `state`. A change from the state reported
    /// before ends any optimistic state.
    ///
    /// While a toggle is shown, the state reported before is the one
    /// reported at the toggle: a report that changed it would have ended
    /// the optimistic state already.
    pub fn report(&mut self, state: PlaybackState) {
        if state != self.reported {
            self.optimistic = None;
        }
        self.reported = state;
    }

    /// The player did not act in time: show the last reported state again.
    pub fn expire(&mut self) {
        self.optimistic = None;
    }
}

impl Default for PlayPause {
    fn default() -> Self {
        Self::new()
    }
}

/// The state a play/pause toggle leads to.
const fn toggled(state: PlaybackState) -> PlaybackState {
    match state {
        PlaybackState::Playing => PlaybackState::Paused,
        PlaybackState::Paused => PlaybackState::Playing,
    }
}
