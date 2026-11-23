//! Everything that travels between the Tray app and the Device daemon, and
//! between the Device daemon and the Device app.
//!
//! Callers send and receive typed messages with [`MessageWriter`] and
//! [`MessageReader`], never bytes. On the wire every message is one frame: a
//! 4-byte big-endian payload length followed by the postcard encoding of the
//! message.
//!
//! A connection between the Tray app and the Device daemon starts with the
//! [`Handshake`], exchanged over the same reader and writer that carry the
//! messages afterwards; then the Tray app sends [`TrayMessage`]s and the
//! Device daemon sends [`DeviceMessage`]s. The Device daemon and the Device
//! app talk over a local socket with the messages in [`local`].

mod frame;
mod handshake;
pub mod local;
mod message;
mod version;

pub use frame::{FrameError, MAX_FRAME_LEN, MessageReader, MessageWriter};
pub use handshake::{HANDSHAKE_TIMEOUT, Handshake, HandshakeError};
pub use message::{DeviceMessage, NowPlaying, TrayMessage};
pub use version::{PROTOCOL_VERSION, ProtocolVersion, SoftwareVersion};

/// The TCP port the Device daemon listens on for the Tray app.
pub const DEVICE_PORT: u16 = 47_800;

/// The loopback port the Device daemon listens on for the Device app.
pub const LOCAL_APP_PORT: u16 = 47_801;
