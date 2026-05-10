//! Cover art module.

pub mod cache;

use {
    libadwaita::{glib::Bytes, gtk::gdk::Texture},
    reqwest::get,
    tokio::runtime::Runtime,
    tracing::warn,
};

/// Converts raw image bytes into a GDK texture.
pub fn bytes_to_texture(bytes: Vec<u8>) -> Option<Texture> {
    match Texture::from_bytes(&Bytes::from_owned(bytes)) {
        Ok(texture) => Some(texture),
        Err(e) => {
            warn!(error = %e, "Failed to convert bytes to texture");
            None
        }
    }
}

/// Fetches image bytes from a URL using a tokio runtime.
pub fn fetch_image_bytes(url: &str) -> Option<Vec<u8>> {
    if url.is_empty() || !url.starts_with("http") {
        return None;
    }
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            warn!(error = %e, url = %url, "Failed to create tokio runtime for cover fetch");
            return None;
        }
    };
    rt.block_on(async {
        let response = match get(url).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, url = %url, "Failed to fetch image");
                return None;
            }
        };
        match response.bytes().await {
            Ok(b) => Some(b.to_vec()),
            Err(e) => {
                warn!(error = %e, url = %url, "Failed to read image bytes");
                None
            }
        }
    })
}
