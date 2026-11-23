//! Reading the Artwork (thumbnail) of the current Media session.

use super::call::{self, CallError};
use std::time::Duration;
use tokio::time::Instant;
use tray_core::media::Artwork;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties as WindowsProperties;
use windows::Storage::Streams::{DataReader, IRandomAccessStreamWithContentType};
use windows_future::IAsyncOperation;

/// How long reading a thumbnail may take in all (opening and loading it).
/// Kept well under the Device's optimistic play/pause hold (3 s): a change
/// event that arrives during the read waits for it.
const READ_TIMEOUT: Duration = Duration::from_secs(1);

/// The largest Artwork read, in bytes. Players hand out tens to hundreds of
/// kilobytes; a larger stream is skipped rather than held in memory.
const MAX_ARTWORK_BYTES: u64 = 16 * 1024 * 1024;

/// Why a thumbnail could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Call(#[from] CallError),
    #[error("Windows handed out {loaded} of {size} bytes")]
    Truncated { loaded: u32, size: u32 },
}

impl From<windows::core::Error> for ReadError {
    fn from(error: windows::core::Error) -> Self {
        Self::Call(error.into())
    }
}

/// The Artwork of a Media session as the player encoded it; `None` when the
/// player supplies none. Takes at most [`READ_TIMEOUT`].
pub async fn read(properties: &WindowsProperties) -> Result<Option<Artwork>, ReadError> {
    let deadline = Instant::now() + READ_TIMEOUT;
    // The stream interfaces are not `Send`, so none may live across an
    // await: only the operations and the reader do.
    let open = match properties.Thumbnail() {
        Ok(reference) => reference.OpenReadAsync()?,
        // windows-rs reports a null object as an error whose code is S_OK.
        Err(error) if error.code().is_ok() => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some((reader, size)) = reader_over(finish_by(open, deadline).await?)? else {
        return Ok(None);
    };
    let loaded = finish_by(reader.LoadAsync(size)?, deadline).await?;
    if loaded < size {
        return Err(ReadError::Truncated { loaded, size });
    }
    let mut encoded = vec![0; size as usize];
    reader.ReadBytes(&mut encoded)?;
    match Artwork::from_encoded(encoded) {
        Ok(artwork) => Ok(Some(artwork)),
        // The same bytes on every read: shown as no Artwork, not retried.
        Err(error) => {
            tracing::debug!(%error, "the thumbnail is no readable image; no Artwork");
            Ok(None)
        }
    }
}

/// [`call::finish`] with the time left until `deadline`; a timeout reports
/// the whole [`READ_TIMEOUT`].
async fn finish_by<T: windows::core::RuntimeType + 'static>(
    operation: IAsyncOperation<T>,
    deadline: Instant,
) -> Result<T, CallError> {
    let left = deadline.saturating_duration_since(Instant::now());
    call::finish(operation, left)
        .await
        .map_err(|error| match error {
            CallError::TimedOut(_) => CallError::TimedOut(READ_TIMEOUT),
            error => error,
        })
}

/// A reader over all of `stream` and its size in bytes; `None` when it is
/// empty or too large. Takes the stream, so it never crosses an await.
fn reader_over(
    stream: IRandomAccessStreamWithContentType,
) -> windows::core::Result<Option<(DataReader, u32)>> {
    let size = stream.Size()?;
    if size == 0 {
        return Ok(None);
    }
    if size > MAX_ARTWORK_BYTES {
        tracing::warn!(size, "Artwork too large; not shown");
        return Ok(None);
    }
    // Fits: MAX_ARTWORK_BYTES is far below u32::MAX.
    let size = u32::try_from(size).unwrap_or(u32::MAX);
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    Ok(Some((reader, size)))
}

/// A small image as a player hands it out; each `shade` a different one.
#[cfg(test)]
pub(super) fn sample(shade: u8) -> Artwork {
    let image = image::RgbImage::from_pixel(2, 2, image::Rgb([shade, 0, 0]));
    let mut encoded = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("a PNG encodes into memory");
    Artwork::from_encoded(encoded.into_inner()).expect("a PNG is Artwork")
}
