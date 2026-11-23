use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::io;
use std::marker::PhantomData;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder, FramedRead, FramedWrite};

/// The largest payload a frame may carry, in bytes. Bigger data (update
/// payloads) has to be split into several messages.
pub const MAX_FRAME_LEN: usize = 8 * 1024 * 1024;

const LENGTH_PREFIX_LEN: usize = 4;

/// Why a message could not be sent or received.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("connection failed: {0}")]
    Io(#[from] io::Error),
    #[error("the connection closed in the middle of a frame")]
    Truncated,
    #[error("frame of {length} bytes exceeds the limit of {MAX_FRAME_LEN} bytes")]
    TooLarge { length: usize },
    #[error("frame does not hold a valid message: {0}")]
    Decode(postcard::Error),
    #[error("message cannot be encoded: {0}")]
    Encode(postcard::Error),
}

/// Receives messages of type `M` from a byte stream.
#[derive(Debug)]
pub struct MessageReader<R, M> {
    frames: FramedRead<R, FrameCodec>,
    message: PhantomData<fn() -> M>,
}

impl<R: AsyncRead + Unpin, M> MessageReader<R, M> {
    pub fn new(inner: R) -> Self {
        Self {
            frames: FramedRead::new(inner, FrameCodec),
            message: PhantomData,
        }
    }

    /// The next frame's payload, not yet decoded. Cancel-safe like
    /// [`recv`](Self::recv).
    pub(crate) async fn recv_frame(&mut self) -> Result<Option<BytesMut>, FrameError> {
        self.frames.next().await.transpose()
    }
}

impl<R: AsyncRead + Unpin, M: DeserializeOwned> MessageReader<R, M> {
    /// The next message, or `None` when the peer closed the stream between
    /// two messages.
    ///
    /// # Cancel safety
    ///
    /// Cancel-safe: the bytes of a partly received frame stay in the reader,
    /// so dropping this future (for example when another branch of a
    /// `select!` wins) loses nothing, and the next call carries on with the
    /// same frame.
    pub async fn recv(&mut self) -> Result<Option<M>, FrameError> {
        match self.recv_frame().await? {
            Some(payload) => decode(&payload).map(Some),
            None => Ok(None),
        }
    }
}

/// Sends messages of type `M` over a byte stream.
#[derive(Debug)]
pub struct MessageWriter<W, M> {
    frames: FramedWrite<W, FrameCodec>,
    message: PhantomData<fn(M)>,
}

impl<W: AsyncWrite + Unpin, M> MessageWriter<W, M> {
    pub fn new(inner: W) -> Self {
        Self {
            frames: FramedWrite::new(inner, FrameCodec),
            message: PhantomData,
        }
    }

    /// Sends any value as one frame. Cancel safety as for
    /// [`send`](Self::send).
    pub(crate) async fn send_frame<T: Serialize>(&mut self, value: &T) -> Result<(), FrameError> {
        self.frames.send(value).await
    }
}

impl<W: AsyncWrite + Unpin, M: Serialize> MessageWriter<W, M> {
    /// Sends `message` and flushes it to the stream.
    ///
    /// # Cancel safety
    ///
    /// Not cancel-safe: when this future is dropped before it completes,
    /// `message` may have gone out in full, may still wait in the writer to
    /// go out with the next send, or may not have been sent at all. The stream
    /// itself stays in step, so later messages arrive intact. Keep a send
    /// running to completion (in a loop of its own) rather than racing it in a
    /// `select!`.
    pub async fn send(&mut self, message: &M) -> Result<(), FrameError> {
        self.send_frame(message).await
    }
}

pub(crate) fn decode<M: DeserializeOwned>(payload: &[u8]) -> Result<M, FrameError> {
    postcard::from_bytes(payload).map_err(FrameError::Decode)
}

/// The wire format of one frame: a 4-byte big-endian payload length, then the
/// payload. It keeps no state between calls; the partial frame lives in the
/// buffer of the `FramedRead` around it, which is what makes receiving
/// cancel-safe.
///
/// tokio-util's `LengthDelimitedCodec` would do the same job, but it reports
/// a stream cut right after a length prefix as a clean end, and a cut payload
/// or an oversized frame only as an untyped `io::Error`.
#[derive(Debug)]
struct FrameCodec;

impl Decoder for FrameCodec {
    type Item = BytesMut;
    type Error = FrameError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<BytesMut>, FrameError> {
        let Some(prefix) = src.first_chunk::<LENGTH_PREFIX_LEN>() else {
            return Ok(None);
        };
        let length = usize::try_from(u32::from_be_bytes(*prefix)).unwrap_or(usize::MAX);
        if length > MAX_FRAME_LEN {
            return Err(FrameError::TooLarge { length });
        }
        let frame_len = LENGTH_PREFIX_LEN + length;
        if src.len() < frame_len {
            src.reserve(frame_len - src.len());
            return Ok(None);
        }
        let mut frame = src.split_to(frame_len);
        Ok(Some(frame.split_off(LENGTH_PREFIX_LEN)))
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<BytesMut>, FrameError> {
        match self.decode(src)? {
            Some(payload) => Ok(Some(payload)),
            None if src.is_empty() => Ok(None),
            None => Err(FrameError::Truncated),
        }
    }
}

impl<T: Serialize> Encoder<&T> for FrameCodec {
    type Error = FrameError;

    fn encode(&mut self, value: &T, dst: &mut BytesMut) -> Result<(), FrameError> {
        let payload = postcard::to_allocvec(value).map_err(FrameError::Encode)?;
        let length = payload.len();
        let prefix = u32::try_from(length)
            .ok()
            .filter(|_| length <= MAX_FRAME_LEN)
            .ok_or(FrameError::TooLarge { length })?;
        // Prefix and payload sit in one buffer, so they go out in one write
        // and share a TCP segment.
        dst.reserve(LENGTH_PREFIX_LEN + length);
        dst.extend_from_slice(&prefix.to_be_bytes());
        dst.extend_from_slice(&payload);
        Ok(())
    }
}
