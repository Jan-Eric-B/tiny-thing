//! Wires the Tray core to Windows: Now Playing from the Windows
//! `MediaSource` goes through a Tray core [`Session`] to the Device.

use crate::windows_media::{StartError, WindowsMediaSource};
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tray_core::{ConnectError, Session, SessionError};

/// Why forwarding Now Playing stopped before Quit.
///
/// There is no reconnecting yet: each of these needs a restart of the Tray
/// app.
#[derive(Debug, thiserror::Error)]
pub enum Stopped {
    #[error(transparent)]
    Media(StartError),
    #[error("cannot start a session with the Device daemon: {0}")]
    Connect(ConnectError),
    #[error("the session with the Device daemon ended: {0}")]
    SessionEnded(SessionError),
    #[error("Now Playing from Windows stopped")]
    MediaEnded,
}

impl Stopped {
    /// A short reason for the tray icon's tooltip.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Media(_) => "cannot read Now Playing from Windows",
            Self::Connect(ConnectError::Io(_)) => "Device not reachable",
            Self::Connect(ConnectError::Handshake(_)) => "Handshake with the Device failed",
            Self::SessionEnded(_) => "connection to the Device lost",
            Self::MediaEnded => "Now Playing from Windows stopped",
        }
    }
}

/// Forwards Now Playing to the Device daemon at `device` until `quit`
/// resolves (`Ok`, also when its sender is dropped) or forwarding fails.
/// Either way the Windows event handlers are removed before it returns.
pub async fn forward_now_playing(
    device: SocketAddr,
    quit: oneshot::Receiver<()>,
) -> Result<(), Stopped> {
    let media = WindowsMediaSource::start().await.map_err(Stopped::Media)?;
    let outcome = tokio::select! {
        stopped = forward(device, &media) => Err(stopped),
        _quit = quit => Ok(()),
    };
    media.stop().await;
    outcome
}

/// Connects to the Device daemon and forwards Now Playing until that fails.
async fn forward(device: SocketAddr, media: &WindowsMediaSource) -> Stopped {
    let session = match Session::connect(device).await {
        Ok(session) => session,
        Err(error) => return Stopped::Connect(error),
    };
    tracing::info!(%device, "forwarding Now Playing to the Device daemon");
    match session.run(media).await {
        Ok(()) => Stopped::MediaEnded,
        Err(error) => Stopped::SessionEnded(error),
    }
}
