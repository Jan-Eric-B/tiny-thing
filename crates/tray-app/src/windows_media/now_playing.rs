//! Turns what Windows reports about the current Media session into Now
//! Playing. Pure, so the rules are tested without Windows media players.

use tray_core::media::{Artwork, NowPlaying, PlaybackState, Progress};
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus;

/// What the current Media session reports about what it plays, as read from
/// Windows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaProperties {
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub playback_status: PlaybackStatus,
    /// `None` when the session supplies none.
    pub artwork: Option<Artwork>,
    /// `None` when the session reports no duration.
    pub progress: Option<Progress>,
}

/// Now Playing for the Media session Windows marks as current, given its
/// properties; `current` is `None` when Windows has no current session.
///
/// A session without a title counts as nothing playing: players keep a
/// session open between songs or after stopping, with the text cleared.
/// Players that leave the artist empty often fill in the album artist.
pub fn now_playing(current: Option<MediaProperties>) -> Option<NowPlaying> {
    let properties = current?;
    let title = properties.title.trim();
    if title.is_empty() {
        return None;
    }
    let artist = match properties.artist.trim() {
        "" => properties.album_artist.trim(),
        artist => artist,
    };
    Some(NowPlaying {
        title: title.to_owned(),
        artist: artist.to_owned(),
        playback: playback_state(properties.playback_status),
        artwork: properties.artwork,
        progress: properties.progress,
    })
}

/// Only a session that Windows reports as playing counts as playing. Every
/// other status (paused, stopped, opened, closed, changing between songs)
/// shows as paused, so the Device offers to play.
pub fn playback_state(status: PlaybackStatus) -> PlaybackState {
    if status == PlaybackStatus::Playing {
        PlaybackState::Playing
    } else {
        PlaybackState::Paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties(title: &str, artist: &str, album_artist: &str) -> MediaProperties {
        MediaProperties {
            title: title.to_owned(),
            artist: artist.to_owned(),
            album_artist: album_artist.to_owned(),
            playback_status: PlaybackStatus::Playing,
            artwork: None,
            progress: None,
        }
    }

    fn song(title: &str, artist: &str) -> Option<NowPlaying> {
        Some(NowPlaying {
            title: title.to_owned(),
            artist: artist.to_owned(),
            playback: PlaybackState::Playing,
            artwork: None,
            progress: None,
        })
    }

    fn playback_for(status: PlaybackStatus) -> Option<PlaybackState> {
        let properties = MediaProperties {
            playback_status: status,
            ..properties("Teardrop", "Massive Attack", "")
        };
        now_playing(Some(properties)).map(|now_playing| now_playing.playback)
    }

    #[test]
    fn no_current_session_is_nothing_playing() {
        assert_eq!(now_playing(None), None);
    }

    #[test]
    fn title_and_artist_of_the_current_session_are_now_playing() {
        assert_eq!(
            now_playing(Some(properties("Teardrop", "Massive Attack", ""))),
            song("Teardrop", "Massive Attack")
        );
    }

    #[test]
    fn the_artwork_of_the_session_goes_with_now_playing() {
        let artwork = super::super::artwork::sample(1);
        let properties = MediaProperties {
            artwork: Some(artwork.clone()),
            ..properties("Teardrop", "Massive Attack", "")
        };

        assert_eq!(
            now_playing(Some(properties)).and_then(|now_playing| now_playing.artwork),
            Some(artwork)
        );
    }

    #[test]
    fn a_session_without_a_title_is_nothing_playing() {
        assert_eq!(
            now_playing(Some(properties("", "Massive Attack", "Massive Attack"))),
            None
        );
    }

    #[test]
    fn a_blank_title_counts_as_no_title() {
        assert_eq!(
            now_playing(Some(properties(" \t ", "Massive Attack", ""))),
            None
        );
    }

    #[test]
    fn the_album_artist_stands_in_for_a_missing_artist() {
        assert_eq!(
            now_playing(Some(properties("Teardrop", " ", "Massive Attack"))),
            song("Teardrop", "Massive Attack")
        );
    }

    #[test]
    fn a_title_without_any_artist_is_still_now_playing() {
        assert_eq!(
            now_playing(Some(properties("Lecture 3", "", ""))),
            song("Lecture 3", "")
        );
    }

    #[test]
    fn surrounding_whitespace_is_dropped() {
        assert_eq!(
            now_playing(Some(properties(" Teardrop\n", " Massive Attack ", ""))),
            song("Teardrop", "Massive Attack")
        );
    }

    #[test]
    fn a_playing_session_is_playing() {
        assert_eq!(
            playback_for(PlaybackStatus::Playing),
            Some(PlaybackState::Playing)
        );
    }

    #[test]
    fn a_paused_session_is_paused() {
        assert_eq!(
            playback_for(PlaybackStatus::Paused),
            Some(PlaybackState::Paused)
        );
    }

    #[test]
    fn every_status_other_than_playing_shows_as_paused() {
        for status in [
            PlaybackStatus::Closed,
            PlaybackStatus::Opened,
            PlaybackStatus::Changing,
            PlaybackStatus::Stopped,
        ] {
            assert_eq!(
                playback_for(status),
                Some(PlaybackState::Paused),
                "{status:?}"
            );
        }
    }
}
