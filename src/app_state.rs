use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::body::Bytes;
use sqlx::SqlitePool;

use crate::discord;

/// Thumbnails rendered, as PNGs, by their file's SHA-256 and palette.
pub type Thumbnails = Arc<Mutex<Drawn>>;

/// The thumbnails drawn last, at most `Drawn::MOST`, about 30 KB each: one
/// for each build and palette viewed would grow without end as people
/// choose palettes of their own, so the oldest go first and are drawn
/// again if asked for.
#[derive(Default)]
pub struct Drawn {
    pictures: HashMap<String, Bytes>,
    order: std::collections::VecDeque<String>,
}

impl Drawn {
    pub const MOST: usize = 1000;

    pub fn get(&self, key: &str) -> Option<Bytes> {
        self.pictures.get(key).cloned()
    }

    pub fn insert(&mut self, key: String, png: Bytes) {
        if self.pictures.insert(key.clone(), png).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > Self::MOST {
            if let Some(oldest) = self.order.pop_front() {
                self.pictures.remove(&oldest);
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// `None` when the Discord settings are missing, which turns login off.
    pub discord: Option<discord::Config>,
    pub http: reqwest::Client,
    /// Rendered thumbnails, so each file is drawn once.
    pub thumbnails: Thumbnails,
    /// What draws thumbnails, with the game's files it has read.
    pub renderer: Arc<crate::thumbnail::Renderer>,
    /// Uploads being analyzed, and those analyzed lately.
    pub analyses: crate::web::analysis::Analyses,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oldest_thumbnail_goes_first() {
        let mut drawn = Drawn::default();
        for i in 0..=Drawn::MOST {
            drawn.insert(i.to_string(), Bytes::from_static(b"png"));
        }
        assert!(drawn.get("0").is_none());
        assert!(drawn.get("1").is_some());
        assert!(drawn.get(&Drawn::MOST.to_string()).is_some());
    }
}
