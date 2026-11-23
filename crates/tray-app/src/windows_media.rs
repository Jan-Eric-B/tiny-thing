//! The Windows [`MediaSource`]: Now Playing from the system media transport
//! controls (`Windows.Media.Control`).
//!
//! Event driven: Windows wakes the adapter when the current Media session
//! changes, when sessions come or go, and when the current session's media
//! properties or playback info change. Each wake-up reads the current session
//! once; wake-ups that arrive during a read fold into one more read. Nothing
//! polls while reads succeed, so an idle Tray app does no work here. After a
//! failed read the adapter also reads again on a timer ([`retry_delay`]), so
//! a stale Now Playing does not wait for the next event.
//!
//! The transport controls go to whichever Media session is current when they
//! are called ([`controls`]).
//!
//! The Artwork (thumbnail) is read as the player encoded it ([`artwork`]),
//! and only when the session or its media properties changed: a play/pause
//! reuses the Artwork read last. Now Playing goes out before the thumbnail
//! is read and again once it is ([`known_artwork`]), so the thumbnail never
//! holds up play/pause or the title. The Tray core prepares the Artwork for
//! the Device.

mod artwork;
mod call;
mod controls;
mod known_artwork;
mod now_playing;
mod subscription;

use crate::retry::retry_delay;
use call::CallError;
use known_artwork::{KnownArtwork, Plan, Song};
use now_playing::MediaProperties;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use subscription::{Subscription, mark_and_wake, wake};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;
use tray_core::media::{ControlError, MediaSource, NowPlaying, TransportControl};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as WindowsSession,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as WindowsProperties,
};
use windows::core::{IUnknown, Interface};

/// How long reading a Media session's properties may take. A read that
/// takes longer is cancelled and counts as failed.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Why the Windows media sessions cannot be followed at all.
#[derive(Debug, thiserror::Error)]
#[error("cannot follow the Windows media sessions: {0}")]
pub struct StartError(#[from] windows::core::Error);

/// Now Playing of the Media session Windows marks as current, and its
/// transport controls.
pub struct WindowsMediaSource {
    manager: SessionManager,
    now_playing: watch::Receiver<Option<NowPlaying>>,
    follower: JoinHandle<()>,
}

impl WindowsMediaSource {
    /// Opens the Windows media session manager and starts following its
    /// current Media session. Must run inside a Tokio runtime.
    pub async fn start() -> Result<Self, StartError> {
        let manager = SessionManager::RequestAsync()?.await?;
        let changed = Arc::new(Notify::new());
        let manager_events = [
            Subscription::register(
                "CurrentSessionChanged",
                &manager,
                |manager| manager.CurrentSessionChanged(&wake(&changed)),
                |manager, token| manager.RemoveCurrentSessionChanged(token),
            )?,
            Subscription::register(
                "SessionsChanged",
                &manager,
                |manager| manager.SessionsChanged(&wake(&changed)),
                |manager, token| manager.RemoveSessionsChanged(token),
            )?,
        ];
        let (sender, now_playing) = watch::channel(None);
        let follower = Follower {
            manager: manager.clone(),
            changed,
            properties_changed: Arc::new(AtomicBool::new(false)),
            _manager_events: manager_events,
            followed: None,
            known_artwork: KnownArtwork::default(),
            now_playing: sender,
        };
        Ok(Self {
            manager,
            now_playing,
            follower: tokio::spawn(follower.run()),
        })
    }

    /// Stops following and waits until the event handlers are removed from
    /// Windows. Dropping the source stops following too, but without waiting.
    pub async fn stop(mut self) {
        self.follower.abort();
        // The follower never ends on its own, so this only reports the abort.
        let _cancelled = (&mut self.follower).await;
    }
}

impl Drop for WindowsMediaSource {
    fn drop(&mut self) {
        // Dropping the follower removes its event handlers and closes the
        // Now Playing channel.
        self.follower.abort();
    }
}

impl MediaSource for WindowsMediaSource {
    fn now_playing(&self) -> watch::Receiver<Option<NowPlaying>> {
        self.now_playing.clone()
    }

    async fn control(&self, control: TransportControl) -> Result<(), ControlError> {
        let command = match control {
            TransportControl::TogglePlayPause => WindowsSession::TryTogglePlayPauseAsync,
            TransportControl::Previous => WindowsSession::TrySkipPreviousAsync,
            TransportControl::Next => WindowsSession::TrySkipNextAsync,
        };
        controls::send(&self.manager, command).await
    }
}

/// Follows the current Media session and publishes its Now Playing.
struct Follower {
    manager: SessionManager,
    changed: Arc<Notify>,
    /// Set by `MediaPropertiesChanged`: the thumbnail may have changed.
    properties_changed: Arc<AtomicBool>,
    _manager_events: [Subscription; 2],
    followed: Option<FollowedSession>,
    known_artwork: KnownArtwork,
    now_playing: watch::Sender<Option<NowPlaying>>,
}

/// The current Media session, with handlers on its change events.
struct FollowedSession {
    session: WindowsSession,
    _events: [Subscription; 2],
}

impl Follower {
    async fn run(mut self) {
        let mut failures: u32 = 0;
        loop {
            match self.refresh().await {
                Ok(()) if failures > 0 => {
                    tracing::info!(failures, "read Now Playing from Windows again");
                    failures = 0;
                }
                Ok(()) => {}
                Err(error) => {
                    failures = failures.saturating_add(1);
                    if failures == 1 {
                        tracing::warn!(%error, "cannot read Now Playing from Windows; retrying");
                    } else {
                        tracing::debug!(%error, failures, "cannot read Now Playing from Windows");
                    }
                }
            }
            wait_for_next_read(&self.changed, failures).await;
        }
    }

    /// Reads the current Media session and publishes its Now Playing if it
    /// differs from what was published last. On a failed read the last Now
    /// Playing stays until a read succeeds.
    ///
    /// When the thumbnail has to be read, Now Playing goes out first with
    /// the Artwork known for the song (none for a new song), then again with
    /// the Artwork read. A thumbnail that cannot be read does not fail the
    /// read: the song keeps its known Artwork, and the next wake-up reads
    /// the thumbnail again.
    async fn refresh(&mut self) -> Result<(), CallError> {
        let current = current_session(&self.manager)?;
        self.follow(current.as_ref());
        if self.properties_changed.swap(false, Ordering::AcqRel) {
            self.known_artwork.properties_changed();
        }
        let Some(session) = current else {
            self.publish(None);
            return Ok(());
        };
        let read = read_properties(&session).await?;
        let (meanwhile, read_thumbnail) = match self.known_artwork.plan(&read.song) {
            Plan::Reuse(artwork) => (artwork, false),
            Plan::Read { meanwhile } => (meanwhile, true),
        };
        let now_playing = now_playing::now_playing(Some(MediaProperties {
            artwork: meanwhile,
            ..read.text
        }));
        self.publish(now_playing.clone());
        let Some(now_playing) = now_playing.filter(|_| read_thumbnail) else {
            return Ok(());
        };
        let thumbnail = artwork::read(&read.properties).await;
        if let Err(error) = &thumbnail {
            tracing::debug!(%error, "cannot read the Artwork");
        }
        let artwork = self.known_artwork.read_finished(read.song, thumbnail);
        self.publish(Some(NowPlaying {
            artwork,
            ..now_playing
        }));
        Ok(())
    }

    /// Publishes `now_playing` if it differs from what was published last.
    fn publish(&self, now_playing: Option<NowPlaying>) {
        self.now_playing.send_if_modified(|published| {
            if *published == now_playing {
                return false;
            }
            tracing::debug!(?now_playing, "Now Playing changed");
            *published = now_playing;
            true
        });
    }

    /// Moves the session event handlers over to `current` if it is not the
    /// session followed so far.
    fn follow(&mut self, current: Option<&WindowsSession>) {
        let unchanged = match (&self.followed, current) {
            (None, None) => true,
            (Some(followed), Some(current)) => same_object(&followed.session, current),
            _ => false,
        };
        if unchanged {
            return;
        }
        self.followed = None;
        self.known_artwork.forget();
        let Some(session) = current else {
            tracing::info!("no current Media session");
            return;
        };
        let app = session
            .SourceAppUserModelId()
            .map(|app| app.to_string_lossy())
            .unwrap_or_default();
        match FollowedSession::subscribe(session, &self.changed, &self.properties_changed) {
            Ok(followed) => {
                tracing::info!(%app, "following the current Media session");
                self.followed = Some(followed);
            }
            Err(error) => {
                // Now Playing is still read on every other change event.
                tracing::warn!(%app, %error, "cannot watch the current Media session");
            }
        }
    }
}

impl FollowedSession {
    fn subscribe(
        session: &WindowsSession,
        changed: &Arc<Notify>,
        properties_changed: &Arc<AtomicBool>,
    ) -> windows::core::Result<Self> {
        Ok(Self {
            session: session.clone(),
            _events: [
                Subscription::register(
                    "MediaPropertiesChanged",
                    session,
                    |session| {
                        session.MediaPropertiesChanged(&mark_and_wake(properties_changed, changed))
                    },
                    |session, token| session.RemoveMediaPropertiesChanged(token),
                )?,
                Subscription::register(
                    "PlaybackInfoChanged",
                    session,
                    |session| session.PlaybackInfoChanged(&wake(changed)),
                    |session, token| session.RemovePlaybackInfoChanged(token),
                )?,
            ],
        })
    }
}

/// Waits for a change event and, after failed reads, at most for the retry
/// delay. A change during the refresh leaves a permit, so it is not lost.
async fn wait_for_next_read(changed: &Notify, failures: u32) {
    match retry_delay(failures) {
        None => changed.notified().await,
        Some(delay) => tokio::select! {
            () = changed.notified() => {}
            () = tokio::time::sleep(delay) => {}
        },
    }
}

/// Whether `a` and `b` are the same Windows object, by COM identity (the
/// `IUnknown` pointer). If Windows cannot tell, they count as different,
/// which only moves the event handlers over once more.
fn same_object(a: &WindowsSession, b: &WindowsSession) -> bool {
    let identity =
        |session: &WindowsSession| session.cast::<IUnknown>().map(|unknown| unknown.as_raw());
    match (identity(a), identity(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The Media session Windows marks as current; `None` when there is none.
fn current_session(manager: &SessionManager) -> windows::core::Result<Option<WindowsSession>> {
    match manager.GetCurrentSession() {
        Ok(session) => Ok(Some(session)),
        // windows-rs reports a null object as an error whose code is S_OK.
        Err(error) if error.code().is_ok() => Ok(None),
        Err(error) => Err(error),
    }
}

/// What a read of the current Media session found, before its thumbnail is
/// read.
struct SessionRead {
    /// Holds the thumbnail reference.
    properties: WindowsProperties,
    /// Without Artwork.
    text: MediaProperties,
    song: Song,
}

/// Reads what the current Media session reports, except its thumbnail.
async fn read_properties(session: &WindowsSession) -> Result<SessionRead, CallError> {
    let properties = call::finish(session.TryGetMediaPropertiesAsync()?, READ_TIMEOUT).await?;
    let song = Song {
        title: properties.Title()?.to_string_lossy(),
        artist: properties.Artist()?.to_string_lossy(),
        album_artist: properties.AlbumArtist()?.to_string_lossy(),
        album_title: properties.AlbumTitle()?.to_string_lossy(),
    };
    let text = MediaProperties {
        title: song.title.clone(),
        artist: song.artist.clone(),
        album_artist: song.album_artist.clone(),
        playback_status: session.GetPlaybackInfo()?.PlaybackStatus()?,
        artwork: None,
    };
    Ok(SessionRead {
        properties,
        text,
        song,
    })
}
