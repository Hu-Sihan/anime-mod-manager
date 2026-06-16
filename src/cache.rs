use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::gamebanana::GameBananaClient;
use crate::models::ModCard;

const API_PAGE_SIZE: u32 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheManifest {
    total_cards: usize,
    synced_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheFile {
    manifest: CacheManifest,
    cards: Vec<ModCard>,
}

pub struct MetadataCache {
    pub cards: Vec<ModCard>,
    manifest: CacheManifest,
    _path: std::path::PathBuf,
}

impl MetadataCache {
    /// Load cache from disk, or return None if not found
    pub fn load(dir: impl AsRef<Path>) -> Option<Self> {
        let path = dir.as_ref().join("cards.json");
        let data = fs::read_to_string(&path).ok()?;
        let cache: CacheFile = serde_json::from_str(&data).ok()?;
        Some(Self {
            cards: cache.cards,
            manifest: cache.manifest,
            _path: path,
        })
    }

    /// Total number of cached cards
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// Get card by global index
    pub fn get(&self, index: usize) -> Option<&ModCard> {
        self.cards.get(index)
    }

    /// Total mods according to last sync
    pub fn total_mods(&self) -> usize {
        self.manifest.total_cards
    }

    /// Hash of the first API page — used to detect staleness
    /// Fetch page 1 from API, return the first mod's ID (for freshness check)
    fn fetch_page1_first_id(client: &GameBananaClient) -> Option<u64> {
        eprintln!("FETCH_PAGE1 requesting...");
        let list = client.list_mods(1, API_PAGE_SIZE, None).ok()?;
        let first = list.records.first()?;
        eprintln!("FETCH_PAGE1 first_id={}", first.id);
        Some(first.id)
    }

    /// Check if the remote data has changed since last sync.
    /// Compares page 1's first mod ID with the cached first mod ID.
    pub fn is_stale(&self, client: &GameBananaClient) -> Option<bool> {
        let remote_id = Self::fetch_page1_first_id(client)?;
        let local_id = self.cards.first()?.id;
        eprintln!("IS_STALE remote_first_id={remote_id} local_first_id={local_id}");
        Some(remote_id != local_id)
    }

    /// Download metadata from GameBanana, incrementally updating if `existing` is provided.
    /// Stops early when API data matches existing cache (no more changes).
    /// Returns progress via callbacks.
    /// - `on_progress`: (pages_done, total_estimated_pages)
    /// - `on_page`: (debug_string) for each page, optional
    pub fn sync(
        client: &GameBananaClient,
        dir: impl AsRef<Path>,
        filter_r18: bool,
        existing: Option<&MetadataCache>,
        on_progress: &dyn Fn(usize, usize),
        on_page: Option<&dyn Fn(String)>,
    ) -> anyhow::Result<Self> {
        let path = dir.as_ref().join("cards.json");
        fs::create_dir_all(dir.as_ref()).ok();

        let mut cards: Vec<ModCard> = existing.map(|c| c.cards.clone()).unwrap_or_default();
        let start_page = if cards.is_empty() { 1 } else { 1 };
        #[allow(unused_assignments)]
        let mut total_mods = existing.map_or(cards.len(), MetadataCache::total_mods);
        let mut page: u32 = start_page;
        let mut fetched = HashSet::new();

        loop {
            if fetched.contains(&page) {
                page += 1;
                continue;
            }
            fetched.insert(page);

            match client.list_mods(page, API_PAGE_SIZE, None) {
                Ok(list) => {
                    total_mods = list.metadata.total as usize;
                    let raw_count = list.records.len();
                    let new: Vec<ModCard> = list
                        .records
                        .into_iter()
                        .filter(|r| !filter_r18 || !r.has_content_ratings)
                        .map(ModCard::from)
                        .collect();
                    let count = new.len();

                    // Log per-page info before consuming `new`
                    if let Some(ref cb) = on_page {
                        let mut info = format!(
                            "page={page} raw={raw_count} filtered={count} 累计{}条",
                            cards.len()
                        );
                        for (i, c) in new.iter().take(3).enumerate() {
                            info.push_str(&format!("\n  new[{i}] id={} name=\"{}\"", c.id, c.name));
                        }
                        cb(info);
                    }

                    // For each card in `new`, check if it matches cache[0].
                    let matched_pos = cards.first().and_then(|c0| {
                        new.iter()
                            .position(|nc| nc.id == c0.id && nc.name == c0.name)
                    });

                    if let Some(pos) = matched_pos {
                        let mut idx = 0;
                        let mut v = new.into_iter();
                        // Insert new cards before overlap point
                        while idx < pos {
                            if let Some(c) = v.next() {
                                cards.insert(idx, c);
                                idx += 1;
                            } else {
                                break;
                            }
                        }
                        // Replace overlapping cards
                        while let Some(c) = v.next() {
                            if idx < cards.len() {
                                cards[idx] = c;
                            } else {
                                cards.push(c);
                            }
                            idx += 1;
                        }
                        break;
                    } else {
                        cards.extend(new);
                    }
                    page += 1;

                    let estimated = (total_mods as f64 / API_PAGE_SIZE as f64).ceil() as usize;
                    on_progress(page as usize - 1, estimated);

                    if list.api_exhausted || count == 0 || matched_pos.is_some() {
                        break;
                    }
                }
                Err(_) => {
                    // Retry after a short pause
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
            }
        }

        let manifest = CacheManifest {
            total_cards: total_mods,
            synced_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        let cache_file = CacheFile {
            manifest: manifest.clone(),
            cards: cards.clone(),
        };
        fs::write(&path, serde_json::to_string_pretty(&cache_file)?)?;

        Ok(Self {
            cards,
            manifest,
            _path: path,
        })
    }
}
