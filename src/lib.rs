pub mod cache;
pub mod filter_data;
pub mod gamebanana;
pub mod img_cache;
pub mod manager;
pub mod meta_manager;
pub mod mod_file_downloader;
pub mod models;

pub use gamebanana::GameBananaClient;
pub use manager::ModManager;
pub use meta_manager::MetaManager;
pub use mod_file_downloader::{ModFileDownloadError, ModFileDownloader};
pub use models::*;

/// Download raw bytes from a URL (for images etc.)
pub fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let resp = minreq::get(url)
        .with_header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .with_timeout(20)
        .send()
        .map_err(|e| format!("HTTP error: {e}"))?;

    if resp.status_code == 200 {
        Ok(resp.as_bytes().to_vec())
    } else {
        Err(format!("HTTP {}", resp.status_code))
    }
}

/// Download image bytes with cache. Checks IMG_CACHE first,
/// falls back to network, and caches the result.
pub fn download_image(url: &str) -> Result<Vec<u8>, String> {
    if let Some(cached) = crate::img_cache::IMG_CACHE.get(url) {
        return Ok(cached);
    }
    let data = download_bytes(url)?;
    crate::img_cache::IMG_CACHE.put(url, data.clone());
    Ok(data)
}

/// GameBanana game IDs for commonly supported titles
pub mod game_ids {
    pub const GENSHIN_IMPACT: u32 = 8552;
    pub const HONKAI_STAR_RAIL: u32 = 8570;
    pub const ZENLESS_ZONE_ZERO: u32 = 9520;
    pub const WUTHERING_WAVES: u32 = 9548;
}
