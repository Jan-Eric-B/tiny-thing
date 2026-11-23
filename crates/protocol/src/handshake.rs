use crate::frame::{self, FrameError, MessageReader, MessageWriter};
use crate::version::{PROTOCOL_VERSION, ProtocolVersion, SoftwareVersion};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

/// How long the whole Handshake may take before a side gives up on it.
///
/// Over the USB network both Handshakes need milliseconds, so this only bites
/// when the peer is gone or not a TinyThing program at all. Without it, a
/// half-open connection (USB unplugged, computer asleep) would hang the
/// Handshake until TCP gives up, which can take hours.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// The first message of a connection between the Tray app and the Device
/// daemon, sent by both sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handshake {
    /// Must stay the first field in every protocol version: a peer decodes it
    /// before anything else, so that a mismatch is still recognised when the
    /// rest of the Handshake has changed shape.
    pub protocol_version: ProtocolVersion,
    /// The TinyThing version of the sending program.
    pub software_version: SoftwareVersion,
}

/// Why the Handshake did not complete.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("the peer closed the connection before sending its Handshake")]
    Closed,
    #[error("the peer did not complete the Handshake within {HANDSHAKE_TIMEOUT:?}")]
    TimedOut,
    #[error("protocol version mismatch: this side speaks {ours}, the peer speaks {theirs}")]
    ProtocolMismatch {
        ours: ProtocolVersion,
        theirs: ProtocolVersion,
    },
}

impl Handshake {
    /// The Handshake of a program at `software_version` speaking this build's
    /// protocol version.
    pub const fn current(software_version: SoftwareVersion) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            software_version,
        }
    }

    /// Sends this Handshake, then receives the peer's, as the first frame on
    /// the reader and writer that carry the connection's messages afterwards.
    ///
    /// The own Handshake always goes out first, so the peer learns this side's
    /// versions even when the exchange then fails with a mismatch. Gives up
    /// with [`HandshakeError::TimedOut`] after [`HANDSHAKE_TIMEOUT`].
    pub async fn exchange<R, W, In, Out>(
        &self,
        reader: &mut MessageReader<R, In>,
        writer: &mut MessageWriter<W, Out>,
    ) -> Result<Handshake, HandshakeError>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        tokio::time::timeout(HANDSHAKE_TIMEOUT, self.exchange_untimed(reader, writer))
            .await
            .map_err(|_| HandshakeError::TimedOut)?
    }

    async fn exchange_untimed<R, W, In, Out>(
        &self,
        reader: &mut MessageReader<R, In>,
        writer: &mut MessageWriter<W, Out>,
    ) -> Result<Handshake, HandshakeError>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        writer.send_frame(self).await?;
        let payload = reader.recv_frame().await?.ok_or(HandshakeError::Closed)?;

        let (theirs, _) =
            postcard::take_from_bytes::<ProtocolVersion>(&payload).map_err(FrameError::Decode)?;
        if theirs != self.protocol_version {
            return Err(HandshakeError::ProtocolMismatch {
                ours: self.protocol_version,
                theirs,
            });
        }
        Ok(frame::decode(&payload)?)
    }
}
