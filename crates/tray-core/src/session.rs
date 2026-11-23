//! One TinyThing session between the Tray app and the Device daemon: from the
//! Handshake until either side closes the connection.

use crate::TRAY_APP_VERSION;
use crate::media::{ControlError, MediaSource, NowPlaying, TransportControl};
use protocol::{
    DeviceMessage, FrameError, Handshake, HandshakeError, InputAction, MessageReader,
    MessageWriter, TrayMessage,
};
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, watch};

/// How many input actions from the Device may wait while a transport control
/// runs. More would only replay stale presses once a slow player answers.
const CONTROL_QUEUE: usize = 1;

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

    /// Forwards Now Playing from `media` to the Device and the Device's input
    /// actions to `media`'s transport controls, until the session ends.
    ///
    /// Ends with `Ok(())` when `media` stops, and with
    /// [`SessionError::DeviceGone`] when the connection to the Device daemon
    /// is lost. A transport control that fails is logged and does not end the
    /// session: the next Now Playing shows the Device what actually happened.
    ///
    /// Transport controls run one after the other, apart from reading the
    /// Device, so a slow player does not hold up the Device's other messages.
    /// While one runs, one more input action may wait; further ones are
    /// dropped, since a press is meant for now, not for whenever the
    /// player is done.
    pub async fn run<M: MediaSource>(self, media: &M) -> Result<(), SessionError> {
        let Self {
            mut reader,
            mut writer,
            ..
        } = self;
        let (controls, pending) = mpsc::channel(CONTROL_QUEUE);
        tokio::select! {
            // In this order on every poll, so `carry_out` takes an action
            // handed over by `receive_from_device` before that reads on.
            biased;
            // `pending` closes only once `receive_from_device` has dropped
            // its sender, which means it has ended, and that ends the session
            // first.
            () = carry_out(media, pending) => Err(SessionError::DeviceGone),
            result = receive_from_device(&mut reader, controls) => result,
            result = send_to_device(&mut writer, media.now_playing()) => result,
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

/// Passes each input action from the Device on to [`carry_out`], dropping
/// it when [`CONTROL_QUEUE`] actions are waiting already.
async fn receive_from_device(
    reader: &mut MessageReader<OwnedReadHalf, DeviceMessage>,
    controls: mpsc::Sender<InputAction>,
) -> Result<(), SessionError> {
    while let Some(message) = reader.recv().await? {
        match message {
            DeviceMessage::Input(action) => {
                tracing::debug!(?action, "input action from the Device");
                if controls.try_send(action).is_err() {
                    tracing::debug!(
                        ?action,
                        "the player is still busy with an earlier input action; dropped"
                    );
                }
                // `carry_out` runs in this task: give it the chance (it is
                // polled first) to take the action before the next message is
                // read, so presses that arrive in one read count as one after
                // the other, not as presses during a control still running.
                tokio::task::yield_now().await;
            }
        }
    }
    Err(SessionError::DeviceGone)
}

/// Carries out the input actions in `pending` on `media`, one after the
/// other, until `pending` closes.
async fn carry_out<M: MediaSource>(media: &M, mut pending: mpsc::Receiver<InputAction>) {
    while let Some(action) = pending.recv().await {
        control(media, action).await;
    }
}

async fn control<M: MediaSource>(media: &M, action: InputAction) {
    match media.control(TransportControl::from(action)).await {
        Ok(()) => {}
        // Every press while nothing plays ends up here; nothing is wrong.
        Err(error @ ControlError::NoMediaSession) => {
            tracing::debug!(?action, %error, "input action from the Device not carried out");
        }
        Err(error) => {
            tracing::warn!(?action, %error, "cannot carry out an input action from the Device");
        }
    }
}
