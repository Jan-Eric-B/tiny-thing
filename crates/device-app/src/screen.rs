//! The screen: shows what the Device daemon sends and turns dial, button and
//! touch input into input actions.

use crate::play_pause::PlayPause;
use crate::progress::Progress;
use crate::{AppWindow, input};
use protocol::local::DaemonMessage;
use protocol::{Artwork, ArtworkId, ImageError, InputAction, PlaybackState, Volume};
use slint::{Color, ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer, Timer, TimerMode};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

/// How often the progress bar moves on while playing. Often enough that the
/// bar moves smoothly (a pixel or less a step for a song of a few minutes)
/// and the shown second changes on time, rarely enough that the Device
/// redraws only a few small areas a second; while paused it does not redraw
/// at all.
pub const PROGRESS_TICK: Duration = Duration::from_millis(250);

/// How long a play/pause toggled on the Device, or a seek, is shown without
/// the Tray app confirming it. The Tray app answers within a fraction of a
/// second when the player follows the command; after this long it is not
/// going to.
///
/// Must outlast the Tray app's limit for a player to accept a command
/// (`CONTROL_TIMEOUT` in the Windows adapter, 2 s) plus the round trip of the
/// Now Playing that follows, or a slow but successful toggle snaps back
/// before the player's answer arrives.
pub const OPTIMISTIC_HOLD: Duration = Duration::from_secs(3);

/// How long the Volume indicator stays after the last dial turn or Volume
/// change.
pub const VOLUME_INDICATOR_HOLD: Duration = Duration::from_millis(1500);

/// The screen of one window. Lives on the UI thread.
///
/// Input on the window acts only while the `Screen` exists, so keep it for as
/// long as the window runs.
pub struct Screen {
    state: Rc<State>,
}

struct State {
    window: AppWindow,
    play_pause: RefCell<PlayPause>,
    /// Ends an optimistic play/pause that the Tray app did not confirm.
    hold: Timer,
    /// The Volume last reported; `None` while it is not known.
    volume: Cell<Option<Volume>>,
    /// Hides the Volume indicator once the Volume rests.
    volume_hold: Timer,
    /// The Artwork Now Playing names; `None` when it names none or nothing
    /// plays.
    artwork_id: Cell<Option<ArtworkId>>,
    /// The Artwork last received, ready to draw. Only one is kept: the
    /// Device daemon keeps the others.
    artwork: RefCell<Option<DrawnArtwork>>,
    /// The Artwork last reported as not to be had (or that could not be
    /// drawn); Now Playing that names it shows as having no Artwork.
    unavailable: Cell<Option<ArtworkId>>,
    /// `None` while nothing plays or Now Playing has no duration.
    progress: RefCell<Option<Progress>>,
    /// Moves the progress bar on while playing.
    progress_tick: Timer,
    /// Ends a seek on the Device that the Tray app did not confirm.
    seek_hold: Timer,
    clock: Box<dyn Fn() -> Instant>,
    send: Box<dyn Fn(InputAction)>,
}

/// An Artwork decoded for drawing.
struct DrawnArtwork {
    id: ArtworkId,
    image: Image,
    backdrop: Image,
    accent: Color,
}

impl Screen {
    /// Takes over `window`'s input: each input action goes to `send`.
    pub fn new(window: &AppWindow, send: impl Fn(InputAction) + 'static) -> Self {
        Self::with_clock(window, send, Instant::now)
    }

    /// Like [`new`](Self::new), with the progress moving on by `clock`
    /// rather than the system's monotonic clock; for tests.
    pub fn with_clock(
        window: &AppWindow,
        send: impl Fn(InputAction) + 'static,
        clock: impl Fn() -> Instant + 'static,
    ) -> Self {
        let state = Rc::new(State {
            window: window.clone_strong(),
            play_pause: RefCell::new(PlayPause::new()),
            hold: Timer::default(),
            volume: Cell::new(None),
            volume_hold: Timer::default(),
            artwork_id: Cell::new(None),
            artwork: RefCell::new(None),
            unavailable: Cell::new(None),
            progress: RefCell::new(None),
            progress_tick: Timer::default(),
            seek_hold: Timer::default(),
            clock: Box::new(clock),
            send: Box::new(send),
        });
        // The window holds only weak references, so dropping the `Screen`
        // frees it even though the window outlives it.
        let weak = Rc::downgrade(&state);
        window.on_device_input(move |device_input| {
            act(&weak, input::device_input_action(device_input));
        });
        let weak = Rc::downgrade(&state);
        window.on_touch_control(move |control| {
            act(&weak, input::touch_control_action(control));
        });
        let weak = Rc::downgrade(&state);
        window.on_seek(move |fraction| seek(&weak, fraction));
        Self { state }
    }

    /// Whether the timer that moves the progress bar on runs: only while
    /// the bar moves, so a paused, ended or empty screen costs no CPU.
    pub fn ticking(&self) -> bool {
        self.state.progress_tick.running()
    }

    /// Updates the screen for `message`.
    pub fn show(&self, message: DaemonMessage) {
        match message {
            DaemonMessage::NowPlaying(now_playing) => {
                let window = &self.state.window;
                window.set_has_now_playing(now_playing.is_some());
                self.state.artwork_id.set(
                    now_playing
                        .as_ref()
                        .and_then(|now_playing| now_playing.artwork),
                );
                self.state.show_artwork();
                let (title, artist, playback, progress) = now_playing
                    .map(|now_playing| {
                        (
                            now_playing.title,
                            now_playing.artist,
                            now_playing.playback,
                            now_playing.progress,
                        )
                    })
                    .unwrap_or((String::new(), String::new(), PlaybackState::Paused, None));
                let same_song = window.get_now_playing_title() == title.as_str()
                    && window.get_now_playing_artist() == artist.as_str();
                window.set_now_playing_title(title.into());
                window.set_now_playing_artist(artist.into());
                // The holds keep running: should this report end an
                // optimistic state, the expiry later changes nothing.
                self.state.play_pause.borrow_mut().report(playback);
                self.state.report_progress(progress, playback, same_song);
                self.state.show_play_pause();
            }
            DaemonMessage::Volume(volume) => self.state.report_volume(volume),
            DaemonMessage::Artwork(artwork) => {
                match DrawnArtwork::decode(&artwork) {
                    Ok(drawn) => {
                        self.state.artwork.replace(Some(drawn));
                    }
                    Err(error) => {
                        tracing::warn!(id = ?artwork.id(), %error, "cannot draw the Artwork");
                        self.state.unavailable.set(Some(artwork.id()));
                    }
                }
                self.state.show_artwork();
            }
            DaemonMessage::ArtworkUnavailable(id) => {
                self.state.unavailable.set(Some(id));
                self.state.show_artwork();
            }
        }
    }
}

impl DrawnArtwork {
    fn decode(artwork: &Artwork) -> Result<Self, ImageError> {
        let protocol::Rgb { r, g, b } = artwork.accent();
        Ok(Self {
            id: artwork.id(),
            image: slint_image(artwork.image())?,
            backdrop: slint_image(artwork.backdrop())?,
            accent: Color::from_rgb_u8(r, g, b),
        })
    }
}

/// `duration` as a clock shows it, in whole seconds: `5:31`, or `1:05:03`
/// from an hour on.
fn clock_time(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// `image` decoded straight into the buffer Slint draws from.
fn slint_image(image: &protocol::Image) -> Result<Image, ImageError> {
    // An Artwork's images have readable headers; were one not readable,
    // decoding it into an empty buffer reports why.
    let (width, height) = image.size().unwrap_or((0, 0));
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    image.decode_into(pixels.make_mut_bytes())?;
    Ok(Image::from_rgb8(pixels))
}

/// The user tapped the progress bar `fraction` of the way along: seeks
/// there, unless the Media session cannot seek.
fn seek(state: &Weak<State>, fraction: f32) {
    let position = state.upgrade().and_then(|state| {
        state
            .progress
            .borrow()
            .as_ref()
            .and_then(|progress| progress.seek_position(fraction))
    });
    if let Some(position) = position {
        act(state, InputAction::Seek(position));
    }
}

/// Carries out `action`: play/pause and seek show their result at once, a
/// Volume step shows the Volume indicator; every action goes to the Device
/// daemon.
fn act(state: &Weak<State>, action: InputAction) {
    let Some(state) = state.upgrade() else {
        return;
    };
    match action {
        InputAction::PlayPause => {
            state.play_pause.borrow_mut().toggle();
            state.show_play_pause();
            let weak = Rc::downgrade(&state);
            // Restarts the hold when it is running already.
            state
                .hold
                .start(TimerMode::SingleShot, OPTIMISTIC_HOLD, move || {
                    if let Some(state) = weak.upgrade() {
                        state.play_pause.borrow_mut().expire();
                        state.show_play_pause();
                    }
                });
        }
        // The indicator shows the level last reported; the Tray app reports
        // the new one a moment later. Without a known Volume there is nothing
        // to show.
        InputAction::VolumeUp | InputAction::VolumeDown => {
            if state.volume.get().is_some() {
                state.show_volume_indicator();
            }
        }
        InputAction::Seek(position) => {
            let now = state.now();
            if let Some(progress) = state.progress.borrow_mut().as_mut() {
                progress.seek(position, now);
            }
            state.show_progress();
            let weak = Rc::downgrade(&state);
            // Restarts the hold when it is running already.
            state
                .seek_hold
                .start(TimerMode::SingleShot, OPTIMISTIC_HOLD, move || {
                    if let Some(state) = weak.upgrade() {
                        let now = state.now();
                        if let Some(progress) = state.progress.borrow_mut().as_mut() {
                            progress.expire_seek(now);
                        }
                        state.show_progress();
                    }
                });
        }
        InputAction::Previous | InputAction::Next => {}
    }
    (state.send)(action);
}

impl State {
    /// Shows the Artwork Now Playing names, with its Backdrop and Accent,
    /// once it is here: never another song's Artwork, and white on black
    /// until then. Artwork that cannot be had shows as none: no box, the
    /// text across the full width.
    fn show_artwork(&self) {
        let window = &self.window;
        let wanted = self.artwork_id.get();
        let artwork = self.artwork.borrow();
        let drawn = artwork
            .as_ref()
            .filter(|artwork| Some(artwork.id) == wanted);
        let unavailable = drawn.is_none() && wanted.is_some() && self.unavailable.get() == wanted;
        window.set_has_artwork(wanted.is_some() && !unavailable);
        match drawn {
            Some(artwork) => {
                window.set_artwork(artwork.image.clone());
                window.set_backdrop(artwork.backdrop.clone());
                window.set_accent(artwork.accent);
                window.set_artwork_shown(true);
            }
            None => {
                window.set_artwork_shown(false);
                window.set_accent(Color::from_rgb_u8(255, 255, 255));
            }
        }
    }

    /// Shows the play/pause state, and the progress moving or not with it.
    fn show_play_pause(self: &Rc<Self>) {
        let shown = self.play_pause.borrow().shown();
        self.window.set_playing(shown == PlaybackState::Playing);
        let now = self.now();
        if let Some(progress) = self.progress.borrow_mut().as_mut() {
            progress.show_playback(shown, now);
        }
        self.show_progress();
    }

    fn now(&self) -> Instant {
        (self.clock)()
    }

    /// Takes in the progress of a new Now Playing with `playback`; `None`
    /// for none. A seek or toggle waiting for the player carries over only
    /// within the `same_song`. Call after the play/pause state has taken in
    /// `playback`.
    fn report_progress(
        &self,
        reported: Option<protocol::Progress>,
        playback: PlaybackState,
        same_song: bool,
    ) {
        let now = self.now();
        let shown = self.play_pause.borrow().shown();
        let mut progress = self.progress.borrow_mut();
        *progress = match (reported, progress.take()) {
            (None, _) => None,
            (Some(reported), Some(mut known)) if same_song => {
                known.report(reported, playback, shown, now);
                Some(known)
            }
            (Some(reported), _) => Some(Progress::new(reported, playback, shown, now)),
        };
    }

    /// Shows the progress as it is now, and keeps moving it on while
    /// playing.
    fn show_progress(self: &Rc<Self>) {
        let now = self.now();
        let progress = *self.progress.borrow();
        let window = &self.window;
        window.set_has_progress(progress.is_some());
        let Some(progress) = progress else {
            self.progress_tick.stop();
            return;
        };
        let (position, duration) = (progress.position(now), progress.duration());
        window.set_progress(position.as_secs_f32() / duration.as_secs_f32());
        window.set_position_text(clock_time(position).into());
        window.set_duration_text(clock_time(duration).into());
        window.set_can_seek(progress.can_seek());
        if !progress.moving(now) {
            self.progress_tick.stop();
        } else if !self.progress_tick.running() {
            let weak = Rc::downgrade(self);
            self.progress_tick
                .start(TimerMode::Repeated, PROGRESS_TICK, move || {
                    if let Some(state) = weak.upgrade() {
                        state.show_progress();
                    }
                });
        }
    }

    /// Shows `volume`. A change from a Volume known before shows the Volume
    /// indicator, whoever made it; the first report (after the Device app
    /// starts, say) does not.
    fn report_volume(self: &Rc<Self>, volume: Option<Volume>) {
        let before = self.volume.replace(volume);
        let window = &self.window;
        match volume {
            Some(volume) => {
                window.set_volume_level(i32::from(volume.level.get()));
                window.set_volume_muted(volume.muted);
                if before.is_some_and(|before| before != volume) {
                    self.show_volume_indicator();
                }
            }
            None => {
                window.set_volume_muted(false);
                window.set_volume_shown(false);
                self.volume_hold.stop();
            }
        }
    }

    /// Shows the Volume indicator, hiding it again after
    /// [`VOLUME_INDICATOR_HOLD`] unless shown again before.
    fn show_volume_indicator(self: &Rc<Self>) {
        self.window.set_volume_shown(true);
        let weak = Rc::downgrade(self);
        // Restarts the hold when it is running already.
        self.volume_hold
            .start(TimerMode::SingleShot, VOLUME_INDICATOR_HOLD, move || {
                if let Some(state) = weak.upgrade() {
                    state.window.set_volume_shown(false);
                }
            });
    }
}
