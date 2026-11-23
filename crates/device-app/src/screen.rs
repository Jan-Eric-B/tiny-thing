//! The screen: shows what the Device daemon sends and turns dial, button and
//! touch input into input actions.

use crate::play_pause::PlayPause;
use crate::{AppWindow, input};
use protocol::local::DaemonMessage;
use protocol::{Artwork, ArtworkId, ImageError, InputAction, PlaybackState, Volume};
use slint::{Color, ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer, Timer, TimerMode};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

/// How long a play/pause toggled on the Device is shown without the Tray app
/// confirming it. The Tray app answers within a fraction of a second when the
/// player follows the command; after this long it is not going to.
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
        let state = Rc::new(State {
            window: window.clone_strong(),
            play_pause: RefCell::new(PlayPause::new()),
            hold: Timer::default(),
            volume: Cell::new(None),
            volume_hold: Timer::default(),
            artwork_id: Cell::new(None),
            artwork: RefCell::new(None),
            unavailable: Cell::new(None),
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
        Self { state }
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
                let (title, artist, playback) = now_playing
                    .map(|now_playing| {
                        (now_playing.title, now_playing.artist, now_playing.playback)
                    })
                    .unwrap_or((String::new(), String::new(), PlaybackState::Paused));
                window.set_now_playing_title(title.into());
                window.set_now_playing_artist(artist.into());
                // The hold keeps running: should this report end the
                // optimistic state, the expiry later changes nothing.
                self.state.play_pause.borrow_mut().report(playback);
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

/// `image` decoded straight into the buffer Slint draws from.
fn slint_image(image: &protocol::Image) -> Result<Image, ImageError> {
    // An Artwork's images have readable headers; were one not readable,
    // decoding it into an empty buffer reports why.
    let (width, height) = image.size().unwrap_or((0, 0));
    let mut pixels = SharedPixelBuffer::<Rgb8Pixel>::new(width, height);
    image.decode_into(pixels.make_mut_bytes())?;
    Ok(Image::from_rgb8(pixels))
}

/// Carries out `action`: play/pause shows its result at once, a Volume step
/// shows the Volume indicator; every action goes to the Device daemon.
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

    fn show_play_pause(&self) {
        let playing = self.play_pause.borrow().shown() == PlaybackState::Playing;
        self.window.set_playing(playing);
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
