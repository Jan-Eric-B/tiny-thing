//! Messages over the local socket between the Device daemon and the Device app.
//!
//! They use the same framing as the connection to the Tray app, but no
//! Handshake: both programs run on the same Device.

use crate::NowPlaying;
use serde::{Deserialize, Serialize};

/// A message from the Device daemon to the Device app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonMessage {
    /// The current Now Playing; `None` when nothing plays.
    NowPlaying(Option<NowPlaying>),
}

/// A message from the Device app to the Device daemon.
///
/// The Device app sends nothing yet; input actions follow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMessage {}
