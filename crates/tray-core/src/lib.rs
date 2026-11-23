//! The Tray core: all logic of the Tray app, with no UI and no platform code.
//!
//! The operating system reaches the Tray core only through the interfaces in
//! [`media`] and [`volume`]; the Device is reached through a [`Session`] with the Device
//! daemon (ADR-0003: the Device listens, the Tray app connects).

pub mod media;
mod session;
pub mod volume;

pub use session::{ConnectError, Session, SessionError};

use protocol::SoftwareVersion;

/// The version of this Tray app, stated in the Handshake.
pub const TRAY_APP_VERSION: SoftwareVersion = SoftwareVersion::from_parts(
    env!("CARGO_PKG_VERSION_MAJOR"),
    env!("CARGO_PKG_VERSION_MINOR"),
    env!("CARGO_PKG_VERSION_PATCH"),
);
