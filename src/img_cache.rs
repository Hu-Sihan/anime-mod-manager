use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Instant;

/// Unified image cache: 256 entries, 300s TTL, keyed by URL string.
pub static IMG_CACHE: std::sync::LazyLock<ImageCache> =
    std::sync::LazyLock::new(|| ImageCache::new(256, 300));

struct Inner {
    order: VecDeque<String>,
    map: HashMap<String, (Vec<u8>, Instant)>,
}

pub struct ImageCache {
    max_entries: usize,
    ttl_secs: u64,
    inner: Mutex<Inner>,
}

impl ImageCache {
    fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            max_entries,
            ttl_secs,
            inner: Mutex::new(Inner {
                order: VecDeque::new(),
                map: HashMap::new(),
            }),
        }
    }

    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let mut inner = self.inner.lock().unwrap();
        if let Some((data, inserted_at)) = inner.map.get(url) {
            if inserted_at.elapsed().as_secs() < self.ttl_secs {
                return Some(data.clone());
            }
            inner.order.retain(|u| u != url);
            inner.map.remove(url);
        }
        None
    }

    pub fn put(&self, url: &str, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.map.contains_key(url) {
            return;
        }
        while inner.order.len() >= self.max_entries {
            if let Some(old) = inner.order.pop_front() {
                inner.map.remove(&old);
            }
        }
        inner.order.push_back(url.to_string());
        inner.map.insert(url.to_string(), (data, Instant::now()));
    }
}
