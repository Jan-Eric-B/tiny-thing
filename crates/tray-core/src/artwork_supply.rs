//! Prepares the Artwork of Now Playing for the Device when the Device asks
//! for it, one preparation at a time, on a blocking thread, so the session's
//! message loop never waits for images.

use crate::artwork::PrepareError;
use crate::media::Artwork;
use protocol::{ArtworkId, TrayMessage};
use std::sync::Arc;
use tokio::task::JoinHandle;

/// What preparing an Artwork for the Device gives.
type Prepared = Result<protocol::Artwork, PrepareError>;

/// Prepares one Artwork for the Device: [`crate::artwork::prepare`], or a
/// stand-in in tests.
type Prepare = Arc<dyn Fn(&Artwork) -> Prepared + Send + Sync>;

/// The Artwork of Now Playing, prepared on request.
pub(crate) struct ArtworkSupply {
    prepare: Prepare,
    /// The Artwork Now Playing names; `None` when it names none.
    current: Option<Artwork>,
    /// How preparing `current` went, once it has.
    outcome: Option<Outcome>,
    /// Whether the Device asked for `current` and has no answer yet.
    wanted: bool,
    /// The preparation running, of `current` or of an earlier Artwork.
    running: Option<(ArtworkId, JoinHandle<Prepared>)>,
}

enum Outcome {
    Ready(protocol::Artwork),
    /// The Artwork cannot be prepared; asking again changes nothing.
    Unavailable,
}

impl Outcome {
    /// The answer to the Device for the Artwork `id`.
    fn answer(&self, id: ArtworkId) -> TrayMessage {
        match self {
            // Cheap: the images are shared, not copied.
            Self::Ready(artwork) => TrayMessage::Artwork(artwork.clone()),
            Self::Unavailable => TrayMessage::ArtworkUnavailable(id),
        }
    }
}

impl ArtworkSupply {
    /// Prepares Artwork with `prepare`.
    pub(crate) fn new(prepare: impl Fn(&Artwork) -> Prepared + Send + Sync + 'static) -> Self {
        Self {
            prepare: Arc::new(prepare),
            current: None,
            outcome: None,
            wanted: false,
            running: None,
        }
    }

    /// Now Playing names `artwork`.
    pub(crate) fn now_playing(&mut self, artwork: Option<&Artwork>) {
        if self.current.as_ref().map(Artwork::id) == artwork.map(Artwork::id) {
            return;
        }
        self.current = artwork.cloned();
        self.outcome = None;
        self.wanted = false;
    }

    /// The Device asked for the Artwork `id`; returns the answer if there is
    /// one already. Otherwise it is prepared, and [`finished`](Self::finished)
    /// gives the answer.
    pub(crate) fn wanted(&mut self, id: ArtworkId) -> Option<TrayMessage> {
        if self.current.as_ref().map(Artwork::id) != Some(id) {
            // Asked for before Now Playing moved on.
            tracing::debug!(?id, "the Device asked for Artwork no longer playing");
            return None;
        }
        if let Some(outcome) = &self.outcome {
            return Some(outcome.answer(id));
        }
        self.wanted = true;
        self.start();
        None
    }

    /// Waits until the running preparation ends; returns the answer to the
    /// Device if it waits for that Artwork. Then prepares the current
    /// Artwork if Now Playing moved on meanwhile and the Device wants it.
    /// Waits forever while nothing is prepared.
    ///
    /// Cancel-safe: dropping it before the preparation ends loses nothing.
    pub(crate) async fn finished(&mut self) -> Option<TrayMessage> {
        let Some((id, preparation)) = &mut self.running else {
            return std::future::pending().await;
        };
        let id = *id;
        let joined = preparation.await;
        self.running = None;
        let outcome = match joined {
            Ok(Ok(artwork)) => Outcome::Ready(artwork),
            Ok(Err(error)) => {
                tracing::warn!(?id, %error, "cannot prepare the Artwork for the Device");
                Outcome::Unavailable
            }
            Err(error) => {
                tracing::error!(?id, %error, "preparing the Artwork for the Device failed");
                Outcome::Unavailable
            }
        };
        let answer = if self.current.as_ref().map(Artwork::id) == Some(id) {
            let answer = std::mem::take(&mut self.wanted).then(|| outcome.answer(id));
            self.outcome = Some(outcome);
            answer
        } else {
            None
        };
        self.start();
        answer
    }

    /// Starts preparing `current` if the Device wants it, it has not been
    /// prepared yet and nothing else is being prepared.
    fn start(&mut self) {
        if !self.wanted || self.outcome.is_some() || self.running.is_some() {
            return;
        }
        let Some(artwork) = self.current.clone() else {
            return;
        };
        let id = artwork.id();
        let prepare = Arc::clone(&self.prepare);
        let preparation = tokio::task::spawn_blocking(move || prepare(&artwork));
        self.running = Some((id, preparation));
    }
}

#[cfg(test)]
mod tests {
    // Test code fails loudly on purpose.
    #![allow(clippy::unwrap_used)]

    use super::*;
    use protocol::{Image, Rgb, RgbPixels, SCREEN_HEIGHT, SCREEN_WIDTH};
    use std::io::Cursor;
    use std::sync::Mutex;
    use std::sync::mpsc;
    use std::time::Duration;

    /// How long a test waits for something that should happen at once.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// How a preparation the test lets go ends.
    enum Ending {
        Ready,
        Fails,
        Panics,
    }

    /// A stand-in for the pipeline: each preparation records its Artwork and
    /// waits until the test lets it go with an [`Ending`].
    struct Preparations {
        started: Arc<Mutex<Vec<ArtworkId>>>,
        endings: mpsc::Sender<Ending>,
    }

    impl Preparations {
        fn supply() -> (ArtworkSupply, Self) {
            let started = Arc::new(Mutex::new(Vec::new()));
            let (endings, ending) = mpsc::channel();
            let ending = Mutex::new(ending);
            let record = Arc::clone(&started);
            let supply = ArtworkSupply::new(move |artwork: &Artwork| {
                record.lock().unwrap().push(artwork.id());
                // Let go of the lock first: a panic holding it would poison it.
                let next = ending.lock().unwrap().recv().unwrap();
                match next {
                    Ending::Ready => Ok(prepared(artwork.id())),
                    Ending::Fails => Err(PrepareError::Empty {
                        width: 0,
                        height: 0,
                    }),
                    Ending::Panics => panic!("a preparation that panics"),
                }
            });
            (supply, Self { started, endings })
        }

        fn let_go(&self, ending: Ending) {
            self.endings.send(ending).unwrap();
        }

        fn started(&self) -> Vec<ArtworkId> {
            self.started.lock().unwrap().clone()
        }
    }

    /// Artwork as a media player hands it out; a new `shade`, a new image.
    fn source(shade: u8) -> Artwork {
        let image = image::RgbImage::from_pixel(2, 2, image::Rgb([shade, 0, 0]));
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        Artwork::from_encoded(encoded.into_inner()).unwrap()
    }

    fn prepared(id: ArtworkId) -> protocol::Artwork {
        let image = |width: u32, height: u32| {
            Image::encode(&RgbPixels {
                width,
                height,
                rgb: vec![0; (width * height * 3) as usize],
            })
            .unwrap()
        };
        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        protocol::Artwork::new(id, image(2, 2), image(SCREEN_WIDTH, SCREEN_HEIGHT), white).unwrap()
    }

    /// The answer of the next preparation to end, failing the test if none
    /// ends within [`PATIENCE`].
    async fn finished(supply: &mut ArtworkSupply) -> Option<TrayMessage> {
        tokio::time::timeout(PATIENCE, supply.finished())
            .await
            .expect("no preparation ended")
    }

    fn is_artwork(answer: Option<TrayMessage>, artwork: &Artwork) -> bool {
        matches!(answer, Some(TrayMessage::Artwork(prepared)) if prepared.id() == artwork.id())
    }

    #[tokio::test]
    async fn artwork_asked_for_is_prepared_and_answered() {
        let (mut supply, preparations) = Preparations::supply();
        let teardrop = source(1);
        supply.now_playing(Some(&teardrop));

        assert!(supply.wanted(teardrop.id()).is_none());
        preparations.let_go(Ending::Ready);

        assert!(is_artwork(finished(&mut supply).await, &teardrop));
    }

    #[tokio::test]
    async fn artwork_is_prepared_once_however_often_it_is_asked_for() {
        let (mut supply, preparations) = Preparations::supply();
        let teardrop = source(1);
        supply.now_playing(Some(&teardrop));
        assert!(supply.wanted(teardrop.id()).is_none());
        // Asked again while it is prepared.
        assert!(supply.wanted(teardrop.id()).is_none());
        preparations.let_go(Ending::Ready);
        assert!(is_artwork(finished(&mut supply).await, &teardrop));

        assert!(is_artwork(supply.wanted(teardrop.id()), &teardrop));
        assert_eq!(preparations.started(), [teardrop.id()]);
    }

    #[tokio::test]
    async fn artwork_no_longer_playing_is_not_answered_and_the_current_one_is_prepared_next() {
        let (mut supply, preparations) = Preparations::supply();
        let (teardrop, angel) = (source(1), source(2));
        supply.now_playing(Some(&teardrop));
        assert!(supply.wanted(teardrop.id()).is_none());
        // The next song, asked for while the first one is still prepared.
        supply.now_playing(Some(&angel));
        assert!(supply.wanted(angel.id()).is_none());

        preparations.let_go(Ending::Ready);
        assert!(finished(&mut supply).await.is_none());
        preparations.let_go(Ending::Ready);

        assert!(is_artwork(finished(&mut supply).await, &angel));
        assert_eq!(preparations.started(), [teardrop.id(), angel.id()]);
    }

    #[tokio::test]
    async fn artwork_asked_for_no_longer_playing_is_not_prepared() {
        let (mut supply, preparations) = Preparations::supply();
        let (teardrop, angel) = (source(1), source(2));
        supply.now_playing(Some(&angel));

        assert!(supply.wanted(teardrop.id()).is_none());

        assert!(preparations.started().is_empty());
    }

    #[tokio::test]
    async fn artwork_that_cannot_be_prepared_is_unavailable_and_the_next_song_gets_its_own() {
        let (mut supply, preparations) = Preparations::supply();
        let (broken, angel) = (source(1), source(2));
        supply.now_playing(Some(&broken));
        assert!(supply.wanted(broken.id()).is_none());
        preparations.let_go(Ending::Fails);
        assert!(matches!(
            finished(&mut supply).await,
            Some(TrayMessage::ArtworkUnavailable(id)) if id == broken.id()
        ));
        // Asked again: the same answer, not another try.
        assert!(matches!(
            supply.wanted(broken.id()),
            Some(TrayMessage::ArtworkUnavailable(id)) if id == broken.id()
        ));

        supply.now_playing(Some(&angel));
        assert!(supply.wanted(angel.id()).is_none());
        preparations.let_go(Ending::Ready);

        assert!(is_artwork(finished(&mut supply).await, &angel));
        assert_eq!(preparations.started(), [broken.id(), angel.id()]);
    }

    #[tokio::test]
    async fn a_preparation_that_panics_counts_as_failed_and_the_next_song_gets_its_artwork() {
        let (mut supply, preparations) = Preparations::supply();
        let (broken, angel) = (source(1), source(2));
        supply.now_playing(Some(&broken));
        assert!(supply.wanted(broken.id()).is_none());
        preparations.let_go(Ending::Panics);
        assert!(matches!(
            finished(&mut supply).await,
            Some(TrayMessage::ArtworkUnavailable(id)) if id == broken.id()
        ));

        supply.now_playing(Some(&angel));
        assert!(supply.wanted(angel.id()).is_none());
        preparations.let_go(Ending::Ready);

        assert!(is_artwork(finished(&mut supply).await, &angel));
    }

    #[tokio::test]
    async fn a_failure_for_artwork_no_longer_playing_does_not_stop_the_current_one() {
        let (mut supply, preparations) = Preparations::supply();
        let (broken, angel) = (source(1), source(2));
        supply.now_playing(Some(&broken));
        assert!(supply.wanted(broken.id()).is_none());
        supply.now_playing(Some(&angel));
        assert!(supply.wanted(angel.id()).is_none());

        preparations.let_go(Ending::Panics);
        assert!(finished(&mut supply).await.is_none());
        preparations.let_go(Ending::Ready);

        assert!(is_artwork(finished(&mut supply).await, &angel));
    }
}
