//! The Device daemon: listens for the Tray app (ADR-0003), runs the Handshake,
//! passes what the Tray app sends on to the Device app over a local socket,
//! and the Device app's input actions back to the Tray app.
//!
//! It keeps the latest state itself, so a Device app that starts or restarts
//! later is brought up to date at once (ADR-0004: the Device app may come and
//! go, the Device daemon stays).

mod app_link;
mod newest_connection;
mod tray_link;

use protocol::{InputAction, NowPlaying, SoftwareVersion};
use std::io;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};

/// How many input actions may wait for the Tray app link before the oldest
/// are dropped. Far more than anyone presses while one message goes out.
const INPUT_QUEUE: usize = 32;

/// The version of this Device daemon, stated in the Handshake.
pub const DEVICE_DAEMON_VERSION: SoftwareVersion = SoftwareVersion::from_parts(
    env!("CARGO_PKG_VERSION_MAJOR"),
    env!("CARGO_PKG_VERSION_MINOR"),
    env!("CARGO_PKG_VERSION_PATCH"),
);

/// Where the Device daemon listens.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// For the Tray app: the Device's USB network interface, or loopback
    /// when the Device app runs as the Simulator.
    pub tray_address: SocketAddr,
    /// For the Device app: always a loopback address.
    pub app_address: SocketAddr,
}

/// A Device daemon whose sockets are bound but not yet serving.
#[derive(Debug)]
pub struct DeviceDaemon {
    tray_listener: TcpListener,
    app_listener: TcpListener,
}

impl DeviceDaemon {
    pub async fn bind(config: &DaemonConfig) -> io::Result<Self> {
        Ok(Self {
            tray_listener: TcpListener::bind(config.tray_address).await?,
            app_listener: TcpListener::bind(config.app_address).await?,
        })
    }

    /// The address the Tray app connects to.
    pub fn tray_address(&self) -> io::Result<SocketAddr> {
        self.tray_listener.local_addr()
    }

    /// The address the Device app connects to.
    pub fn app_address(&self) -> io::Result<SocketAddr> {
        self.app_listener.local_addr()
    }

    /// Serves the Tray app and the Device app. Runs until the task is dropped:
    /// failed connections are logged, and a new connection from either
    /// replaces the one before it.
    ///
    /// Now Playing is kept and handed to every new Device app connection.
    /// Input actions are not kept: one that arrives while no Tray app is
    /// connected is dropped, so a later connection does not replay stale
    /// presses.
    pub async fn run(self) {
        let (now_playing, _) = watch::channel::<Option<NowPlaying>>(None);
        let app_view = now_playing.subscribe();
        let (inputs, _) = broadcast::channel::<InputAction>(INPUT_QUEUE);
        tokio::join!(
            tray_link::serve(self.tray_listener, now_playing, inputs.clone()),
            app_link::serve(self.app_listener, app_view, inputs),
        );
    }
}
