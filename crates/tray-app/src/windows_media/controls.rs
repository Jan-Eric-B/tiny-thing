//! Transport controls for the Media session Windows marks as current.

use super::call::{self, CallError};
use super::current_session;
use std::time::Duration;
use tray_core::media::ControlError;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as WindowsSession,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
};
use windows_future::IAsyncOperation;

/// How long a player may take to accept a command. Players answer within
/// milliseconds; a slower one counts as failed so later input is not held up.
///
/// The Device app's `OPTIMISTIC_HOLD` (3 s) must outlast this plus the round
/// trip of the Now Playing that follows; raise both together.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

/// Sends a command, started by `command`, to the Media session that is
/// current right now. The session answers whether it accepted the command;
/// what it then does shows up as a change of Now Playing.
pub async fn send(
    manager: &SessionManager,
    command: impl FnOnce(&WindowsSession) -> windows::core::Result<IAsyncOperation<bool>>,
) -> Result<(), ControlError> {
    let session = current_session(manager)
        .map_err(platform)?
        .ok_or(ControlError::NoMediaSession)?;
    let operation = command(&session).map_err(platform)?;
    match call::finish(operation, CONTROL_TIMEOUT).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ControlError::Refused),
        Err(CallError::TimedOut(_)) => Err(ControlError::TimedOut),
        Err(CallError::Windows(error)) => Err(platform(error)),
    }
}

fn platform(error: windows::core::Error) -> ControlError {
    ControlError::Platform(Box::new(error))
}
