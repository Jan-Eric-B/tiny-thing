//! When to read the current Media session's thumbnail again, and which
//! Artwork goes with Now Playing meanwhile. Pure, so the rules are tested
//! without Windows media players.
//!
//! Reading the thumbnail is the slow part of a read, so it stays off the
//! path of play/pause and title changes: Now Playing goes out first with the
//! Artwork known for the song (none for a new song), and again once a fresh
//! thumbnail is read.

use tray_core::media::Artwork;

/// The text a Media session shows with its thumbnail, as Windows reports
/// it. While it stays the same, so does the thumbnail, unless the player
/// reports changed media properties (some players, like TIDAL, send the new
/// title first and the new image a moment later).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album_title: String,
}

/// What to do about the Artwork for a read of the current Media session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// The thumbnail is as read last: publish this Artwork, read nothing.
    Reuse(Option<Artwork>),
    /// Publish `meanwhile` at once, then read the thumbnail.
    Read { meanwhile: Option<Artwork> },
}

/// The Artwork last read for the followed Media session.
#[derive(Debug, Default)]
pub struct KnownArtwork {
    /// `None` until a thumbnail read finishes for the session.
    song: Option<Song>,
    artwork: Option<Artwork>,
    /// Whether the player may have changed its thumbnail since the last
    /// read, or the last read failed.
    stale: bool,
}

impl KnownArtwork {
    /// Forgets everything: for another Media session, or none.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    /// The player reported changed media properties, so the thumbnail may
    /// have changed even if the text has not.
    pub fn properties_changed(&mut self) {
        self.stale = true;
    }

    /// What to do about the Artwork when the session shows `song`.
    pub fn plan(&self, song: &Song) -> Plan {
        if self.song.as_ref() != Some(song) {
            return Plan::Read { meanwhile: None };
        }
        if self.stale {
            Plan::Read {
                meanwhile: self.artwork.clone(),
            }
        } else {
            Plan::Reuse(self.artwork.clone())
        }
    }

    /// Takes the result of reading the thumbnail while the session showed
    /// `song`, and returns the Artwork to publish. A failed read keeps the
    /// Artwork known for the same song (none for a new song) rather than
    /// letting it flicker away, and leaves the thumbnail to be read again on
    /// the next wake-up.
    pub fn read_finished<E>(
        &mut self,
        song: Song,
        read: Result<Option<Artwork>, E>,
    ) -> Option<Artwork> {
        match read {
            Ok(artwork) => {
                *self = Self {
                    song: Some(song),
                    artwork,
                    stale: false,
                };
            }
            Err(_) => {
                if self.song.as_ref() != Some(&song) {
                    self.song = Some(song);
                    self.artwork = None;
                }
                self.stale = true;
            }
        }
        self.artwork.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str) -> Song {
        Song {
            title: title.to_owned(),
            artist: "Massive Attack".to_owned(),
            album_artist: "Massive Attack".to_owned(),
            album_title: "Mezzanine".to_owned(),
        }
    }

    fn artwork(content: u8) -> Artwork {
        super::super::artwork::sample(content)
    }

    /// Known Artwork after a successful read of `artwork` for `title`.
    fn known(title: &str, artwork: Option<Artwork>) -> KnownArtwork {
        let mut known = KnownArtwork::default();
        known.read_finished::<()>(song(title), Ok(artwork));
        known
    }

    #[test]
    fn the_first_song_is_read_without_artwork_meanwhile() {
        assert_eq!(
            KnownArtwork::default().plan(&song("Teardrop")),
            Plan::Read { meanwhile: None }
        );
    }

    #[test]
    fn an_unchanged_song_reuses_its_artwork_without_reading() {
        // A play/pause wakes the adapter without changing the properties.
        let known = known("Teardrop", Some(artwork(1)));

        assert_eq!(known.plan(&song("Teardrop")), Plan::Reuse(Some(artwork(1))));
    }

    #[test]
    fn an_unchanged_song_without_artwork_reads_nothing() {
        assert_eq!(
            known("Teardrop", None).plan(&song("Teardrop")),
            Plan::Reuse(None)
        );
    }

    #[test]
    fn a_new_song_is_read_without_the_previous_artwork_meanwhile() {
        let known = known("Teardrop", Some(artwork(1)));

        assert_eq!(known.plan(&song("Angel")), Plan::Read { meanwhile: None });
    }

    #[test]
    fn any_change_in_the_text_counts_as_a_new_song() {
        let known = known("Teardrop", Some(artwork(1)));
        let changes = [
            Song {
                artist: "Elizabeth Fraser".to_owned(),
                ..song("Teardrop")
            },
            Song {
                album_artist: "Various Artists".to_owned(),
                ..song("Teardrop")
            },
            Song {
                album_title: "Collected".to_owned(),
                ..song("Teardrop")
            },
        ];

        for changed in changes {
            assert_eq!(
                known.plan(&changed),
                Plan::Read { meanwhile: None },
                "{changed:?}"
            );
        }
    }

    #[test]
    fn changed_properties_read_again_and_keep_the_artwork_meanwhile() {
        // TIDAL: the new title comes first, the new image a moment later
        // under the same title.
        let mut known = known("Angel", Some(artwork(1)));
        known.properties_changed();

        assert_eq!(
            known.plan(&song("Angel")),
            Plan::Read {
                meanwhile: Some(artwork(1))
            }
        );
        assert_eq!(
            known.read_finished::<()>(song("Angel"), Ok(Some(artwork(2)))),
            Some(artwork(2))
        );
        assert_eq!(known.plan(&song("Angel")), Plan::Reuse(Some(artwork(2))));
    }

    #[test]
    fn a_failed_read_keeps_the_artwork_of_the_same_song() {
        let mut known = known("Teardrop", Some(artwork(1)));
        known.properties_changed();

        assert_eq!(
            known.read_finished(song("Teardrop"), Err("timed out")),
            Some(artwork(1))
        );
    }

    #[test]
    fn a_failed_read_for_a_new_song_has_no_artwork() {
        let mut known = known("Teardrop", Some(artwork(1)));

        assert_eq!(known.read_finished(song("Angel"), Err("timed out")), None);
    }

    #[test]
    fn after_a_failed_read_the_next_wake_up_reads_again() {
        let mut known = known("Teardrop", Some(artwork(1)));
        known.properties_changed();
        known.read_finished(song("Teardrop"), Err("timed out"));
        assert_eq!(
            known.plan(&song("Teardrop")),
            Plan::Read {
                meanwhile: Some(artwork(1))
            }
        );

        let mut known = KnownArtwork::default();
        known.read_finished(song("Angel"), Err("timed out"));
        assert_eq!(known.plan(&song("Angel")), Plan::Read { meanwhile: None });
    }

    #[test]
    fn a_forgotten_session_reads_again() {
        let mut known = known("Teardrop", Some(artwork(1)));
        known.forget();

        assert_eq!(
            known.plan(&song("Teardrop")),
            Plan::Read { meanwhile: None }
        );
    }
}
