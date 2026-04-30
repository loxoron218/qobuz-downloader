//! Cover art texture cache with on-demand loading.

use std::{collections::HashMap, sync::Arc};

use {
    async_channel::Sender,
    libadwaita::{gio::spawn_blocking, glib::Bytes, gtk::gdk::Texture},
    parking_lot::Mutex,
    reqwest::get,
    tokio::runtime::Runtime,
    tracing::warn,
};

/// In-memory cache of cover art textures keyed by URL.
#[derive(Clone)]
pub struct CoverArtCache {
    /// Cached textures mapped by URL.
    textures: Arc<Mutex<HashMap<String, Option<Texture>>>>,
}

impl CoverArtCache {
    /// Creates a new empty cache.
    pub fn new() -> Self {
        Self {
            textures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns a cached texture if available.
    pub fn get(&self, url: &str) -> Option<Texture> {
        let textures = self.textures.lock();
        match textures.get(url) {
            Some(Some(texture)) => Some(texture.clone()),
            _ => None,
        }
    }

    /// Starts loading a cover art image in the background.
    ///
    /// If the URL is already cached or currently loading, this is a no-op.
    /// On completion, the result is sent via `sender` as `(url, Option<Texture>)`.
    pub fn start_load(&self, url: String, sender: Sender<(String, Option<Texture>)>) {
        let mut textures = self.textures.lock();
        if textures.contains_key(&url) {
            return;
        }
        textures.insert(url.clone(), None);
        drop(textures);

        let textures = Arc::clone(&self.textures);
        spawn_blocking(move || load_and_cache_texture(url, &sender, &textures));
    }
}

/// Loads and caches a cover art texture, then sends the result via the channel.
fn load_and_cache_texture(
    url: String,
    sender: &Sender<(String, Option<Texture>)>,
    textures: &Arc<Mutex<HashMap<String, Option<Texture>>>>,
) {
    let texture = fetch_texture(&url);
    textures.lock().insert(url.clone(), texture.clone());
    if let Err(err) = sender.send_blocking((url, texture)) {
        warn!(error = %err, "Failed to send cover art result");
    }
}

/// Fetches an image from a URL and converts it to a GDK texture.
fn fetch_texture(url: &str) -> Option<Texture> {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            warn!(error = %e, url = %url, "Failed to create tokio runtime for cover art fetch");
            return None;
        }
    };
    rt.block_on(async {
        let response = match get(url).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, url = %url, "Failed to fetch cover art image");
                return None;
            }
        };
        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, url = %url, "Failed to read cover art response bytes");
                return None;
            }
        };
        let glib_bytes = Bytes::from_owned(bytes);
        match Texture::from_bytes(&glib_bytes) {
            Ok(texture) => Some(texture),
            Err(e) => {
                warn!(error = %e, url = %url, "Failed to create texture from cover art bytes");
                None
            }
        }
    })
}
