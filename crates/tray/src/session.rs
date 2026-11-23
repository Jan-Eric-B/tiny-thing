//! One TinyThing session between the Tray app and the Device daemon: from the
//! Handshake until either side closes the connection.

use crate::TRAY_APP_VERSION;
use crate::media::{MediaSource, NowPlaying};
use protocol::{
    DeviceMessage, FrameError, Handshake, HandshakeError, MessageReader, MessageWriter, TrayMessage,
};
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::watch;

/// Why no session could be started.
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("cannot reach the Device daemon: {0}")]
    Io(#[from] io::Error),
    #[error("Handshake with the Device daemon failed: {0}")]
    Handshake(#[from] HandshakeError),
}

/// Why a running session ended.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The connection to the Device daemon is gone. Whether a lost connection
    /// shows up as a clean close, a reset or a broken pipe depends on the
    /// operating system and on timing (Windows resets a connection where
    /// Linux often closes it), so all of them end up here.
    #[error("the connection to the Device daemon is gone")]
    DeviceGone,
    /// The connection is still there, but a message could not be sent or the
    /// Device daemon sent one that cannot be read.
    #[error(transparent)]
    Frame(FrameError),
}

impl From<FrameError> for SessionError {
    fn from(error: FrameError) -> Self {
        match error {
            FrameError::Truncated => Self::DeviceGone,
            FrameError::Io(io_error) if is_connection_lost(&io_error) => Self::DeviceGone,
            other => Self::Frame(other),
        }
    }
}

fn is_connection_lost(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
    )
}

/// A connection to the Device daemon whose Handshake has completed.
#[derive(Debug)]
pub struct Session {
    device_handshake: Handshake,
    reader: MessageReader<OwnedReadHalf, DeviceMessage>,
    writer: MessageWriter<OwnedWriteHalf, TrayMessage>,
}

impl Session {
    /// Connects to the Device daemon at `device` and runs the Handshake.
    ///
    /// A protocol version mismatch is reported as
    /// [`HandshakeError::ProtocolMismatch`], a Device daemon that does not
    /// answer in time as [`HandshakeError::TimedOut`].
    pub async fn connect(device: SocketAddr) -> Result<Self, ConnectError> {
        let (read, write) = TcpStream::connect(device).await?.into_split();
        let mut reader = MessageReader::new(read);
        let mut writer = MessageWriter::new(write);
        let device_handshake = Handshake::current(TRAY_APP_VERSION)
            .exchange(&mut reader, &mut writer)
            .await?;
        tracing::info!(
            %device,
            device_version = %device_handshake.software_version,
            "Handshake with the Device daemon complete"
        );
        Ok(Self {
            device_handshake,
            reader,
            writer,
        })
    }

    /// The versions the Device daemon stated in the Handshake.
    pub fn device_handshake(&self) -> &Handshake {
        &self.device_handshake
    }

    /// Forwards Now Playing from `media` to the Device until the session ends.
    ///
    /// Subscribes to `media` at once, so the returned future does not borrow
    /// it and can be spawned. Ends with `Ok(())` when `media` stops, and with
    /// [`SessionError::DeviceGone`] when the connection to the Device daemon
    /// is lost.
    pub fn run(
        self,
        media: &dyn MediaSource,
    ) -> impl Future<Output = Result<(), SessionError>> + Send + use<> {
        let now_playing = media.now_playing();
        let Self {
            mut reader,
            mut writer,
            ..
        } = self;
        async move {
            tokio::select! {
                result = send_to_device(&mut writer, now_playing) => result,
                result = receive_from_device(&mut reader) => result,
            }
        }
    }
}

/// Sends the current Now Playing at once, then every change.
async fn send_to_device(
    writer: &mut MessageWriter<OwnedWriteHalf, TrayMessage>,
    mut now_playing: watch::Receiver<Option<NowPlaying>>,
) -> Result<(), SessionError> {
    loop {
        let message = TrayMessage::NowPlaying(
            now_playing
                .borrow_and_update()
                .as_ref()
                .map(protocol::NowPlaying::from),
        );
        writer.send(&message).await?;
        if now_playing.changed().await.is_err() {
            return Ok(());
        }
    }
}

/// Handles what the Device daemon sends. That is nothing yet:
/// `DeviceMessage` gains variants with the input actions.
#[expect(
    clippy::never_loop,
    reason = "DeviceMessage has no variants yet; this loops once it has"
)]
async fn receive_from_device(
    reader: &mut MessageReader<OwnedReadHalf, DeviceMessage>,
) -> Result<(), SessionError> {
    while let Some(message) = reader.recv().await? {
        match message {}
    }
    Err(SessionError::DeviceGone)
}
