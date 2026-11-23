//! The Device daemon: listens for the Tray app (ADR-0003), runs the Handshake,
//! passes what the Tray app sends on to the Device app over a local socket,
//! and the Device app's input actions back to the Tray app.
//!
//! It keeps the latest state itself, so a Device app that starts or restarts
//! later is brought up to date at once (ADR-0004: the Device app may come and
//! go, the Device daemon stays). It also keeps the last few Artworks and asks
//! the Tray app only for Artwork it does not have.

mod app_link;
mod artwork_cache;
mod newest_connection;
mod tray_link;

use artwork_cache::ArtworkCache;
use protocol::{Artwork, ArtworkId, InputAction, NowPlaying, SoftwareVersion, Volume};
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};

/// How many input actions may wait for the Tray app link before the oldest
/// are dropped. Far more than anyone presses while one message goes out.
const INPUT_QUEUE: usize = 32;

/// How many Artworks the Device daemon keeps. An Artwork takes at most about
/// 2 MB (a Backdrop of the screen's size and an Artwork of the Artwork box's
/// size, as badly compressed as QOI gets; real covers take a few hundred
/// kilobytes), so these and the one shown take at most about 10 MB.
pub const ARTWORK_CACHE_LEN: usize = 4;

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
    /// Now Playing, its Artwork and the Volume are kept and handed to every
    /// new Device app connection. The last [`ARTWORK_CACHE_LEN`] Artworks are
    /// kept across Tray app connections; the Tray app is asked only for
    /// Artwork not among them.
    /// Input actions are not kept: one that arrives while no Tray app is
    /// connected is dropped, so a later connection does not replay stale
    /// presses.
    pub async fn run(self) {
        let state = State {
            now_playing: watch::Sender::new(None),
            volume: watch::Sender::new(None),
            artwork: watch::Sender::new(None),
            artworks: Arc::default(),
        };
        let app_view = state.subscribe();
        let (inputs, _) = broadcast::channel::<InputAction>(INPUT_QUEUE);
        tokio::join!(
            tray_link::serve(self.tray_listener, state, inputs.clone()),
            app_link::serve(self.app_listener, app_view, inputs),
        );
    }
}

/// The latest state the Tray app reported, kept for the Device app.
#[derive(Debug, Clone)]
struct State {
    now_playing: watch::Sender<Option<NowPlaying>>,
    volume: watch::Sender<Option<Volume>>,
    /// The Artwork of Now Playing once it is here, or word that it cannot
    /// be had; may still be about an earlier Now Playing, which the Device
    /// app ignores.
    artwork: watch::Sender<Option<ArtworkNews>>,
    artworks: Arc<Mutex<ArtworkCache>>,
}

/// A view of [`State`] that follows its changes.
#[derive(Debug, Clone)]
struct StateView {
    now_playing: watch::Receiver<Option<NowPlaying>>,
    volume: watch::Receiver<Option<Volume>>,
    artwork: watch::Receiver<Option<ArtworkNews>>,
}

/// What the Device daemon knows about the Artwork of Now Playing.
#[derive(Debug, Clone)]
enum ArtworkNews {
    Here(Artwork),
    /// The Tray app cannot make it.
    Unavailable(ArtworkId),
}

impl ArtworkNews {
    fn id(&self) -> ArtworkId {
        match self {
            Self::Here(artwork) => artwork.id(),
            Self::Unavailable(id) => *id,
        }
    }

    fn is_here(&self) -> bool {
        matches!(self, Self::Here(_))
    }
}

impl State {
    fn subscribe(&self) -> StateView {
        StateView {
            now_playing: self.now_playing.subscribe(),
            volume: self.volume.subscribe(),
            artwork: self.artwork.subscribe(),
        }
    }

    fn artworks(&self) -> MutexGuard<'_, ArtworkCache> {
        // The cache stays consistent even if a holder panicked.
        self.artworks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hands `news` to the Device app, unless it has it already.
    fn tell(&self, news: ArtworkNews) {
        self.artwork.send_if_modified(|told| {
            let known = told
                .as_ref()
                .is_some_and(|told| told.id() == news.id() && told.is_here() == news.is_here());
            if known {
                return false;
            }
            *told = Some(news);
            true
        });
    }
}
