//! The last few Artworks the Tray app sent, so a song played again shows its
//! Artwork without sending it again.

use crate::ARTWORK_CACHE_LEN;
use protocol::{Artwork, ArtworkId};
use std::collections::VecDeque;

/// At most [`ARTWORK_CACHE_LEN`] Artworks; the one used longest ago goes
/// first.
#[derive(Debug, Default)]
pub(crate) struct ArtworkCache {
    /// Least recently used first.
    artworks: VecDeque<Artwork>,
}

impl ArtworkCache {
    /// The Artwork with `id`, if kept; it counts as used now. Cheap: the
    /// images are shared, not copied.
    pub(crate) fn get(&mut self, id: ArtworkId) -> Option<Artwork> {
        let index = self
            .artworks
            .iter()
            .position(|artwork| artwork.id() == id)?;
        let artwork = self.artworks.remove(index)?;
        self.artworks.push_back(artwork.clone());
        Some(artwork)
    }

    /// Keeps `artwork` as the one used last, dropping the one used longest
    /// ago when full.
    pub(crate) fn insert(&mut self, artwork: Artwork) {
        self.artworks.retain(|kept| kept.id() != artwork.id());
        self.artworks.push_back(artwork);
        while self.artworks.len() > ARTWORK_CACHE_LEN {
            self.artworks.pop_front();
        }
    }
}
