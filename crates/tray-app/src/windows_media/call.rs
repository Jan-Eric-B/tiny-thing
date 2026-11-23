//! Waiting for a Windows async operation, with a time limit.

use std::time::Duration;
use windows::core::RuntimeType;
use windows_future::IAsyncOperation;

/// Why a Windows async operation did not deliver its result.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
    #[error("Windows did not answer within {0:?}")]
    TimedOut(Duration),
}

/// The result of `operation`, or [`CallError::TimedOut`] after `limit`. A
/// timed-out operation is cancelled in Windows: dropping the future alone
/// would leave it running there.
pub async fn finish<T: RuntimeType + 'static>(
    operation: IAsyncOperation<T>,
    limit: Duration,
) -> Result<T, CallError> {
    match tokio::time::timeout(limit, operation.clone().into_future()).await {
        Ok(result) => Ok(result?),
        Err(_elapsed) => {
            if let Err(error) = operation.Cancel() {
                tracing::debug!(%error, "cannot cancel a timed-out Windows operation");
            }
            Err(CallError::TimedOut(limit))
        }
    }
}
