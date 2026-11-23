use serde::{Deserialize, Serialize};
use std::fmt;

/// The version of the wire format. Both sides must speak the same one.
///
/// Bump it on every change to a message's encoding, including added fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion(pub u16);

/// The protocol version this build speaks.
///
/// 2: Now Playing carries the playback state; the Device sends input actions.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion(2);

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A TinyThing release version, `major.minor.patch` as in Semantic Versioning.
/// A pre-release or build suffix is not part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SoftwareVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SoftwareVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Builds a version from its three decimal parts as text, so that a crate
    /// can turn its package version into a constant:
    ///
    /// ```
    /// # use protocol::SoftwareVersion;
    /// const VERSION: SoftwareVersion = SoftwareVersion::from_parts(
    ///     env!("CARGO_PKG_VERSION_MAJOR"),
    ///     env!("CARGO_PKG_VERSION_MINOR"),
    ///     env!("CARGO_PKG_VERSION_PATCH"),
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if a part is not a decimal `u32`. Used in a `const` item, that
    /// panic is a compile error.
    pub const fn from_parts(major: &str, minor: &str, patch: &str) -> Self {
        Self::new(part(major), part(minor), part(patch))
    }
}

const fn part(text: &str) -> u32 {
    match u32::from_str_radix(text, 10) {
        Ok(value) => value,
        Err(_) => panic!("a version part must be a decimal u32"),
    }
}

impl fmt::Display for SoftwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
