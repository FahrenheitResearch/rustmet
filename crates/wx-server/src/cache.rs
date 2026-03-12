//! In-memory tile cache for the wx-server tile server.
//!
//! Caches rendered PNG tiles so that multiple tile requests for the same
//! model/var/level/fhour combination don't each trigger a separate GRIB2
//! download from NOMADS. A single HRRR CAPE field at zoom 5 can produce
//! ~30 tiles; without caching each would re-download 1-3 MB of GRIB2 data.
//!
//! # Usage
//!
//! ```rust,ignore
//! let cache = Arc::new(TileCache::new(512, 300)); // 512 MB, 5 min TTL
//!
//! // In tile handler:
//! if let Some(png) = cache.get("hrrr", "CAPE", "surface", 0, 5, 8, 12).await {
//!     return png_response(png, true); // cache hit
//! }
//! // ... generate tile ...
//! cache.put("hrrr", "CAPE", "surface", 0, 5, 8, 12, png_bytes).await;
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cached entry holding rendered tile PNG bytes.
struct CacheEntry {
    data: Vec<u8>,
    created: Instant,
    size_bytes: usize,
}

/// In-memory tile cache keyed by `"model/var/level/fHH/z/x/y"`.
///
/// Thread-safe via `tokio::sync::RwLock`; designed to be wrapped in `Arc`
/// and shared as axum state.
pub struct TileCache {
    tiles: RwLock<HashMap<String, CacheEntry>>,
    max_size: usize,
    ttl: Duration,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl TileCache {
    /// Create a new cache with the given size limit (in megabytes) and TTL (in seconds).
    pub fn new(max_size_mb: usize, ttl_seconds: u64) -> Self {
        Self {
            tiles: RwLock::new(HashMap::new()),
            max_size: max_size_mb * 1024 * 1024,
            ttl: Duration::from_secs(ttl_seconds),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Build the string key used for cache lookups.
    fn key(model: &str, var: &str, level: &str, fhour: u32, z: u32, x: u32, y: u32) -> String {
        format!("{}/{}/{}/f{:02}/{}/{}/{}", model, var, level, fhour, z, x, y)
    }

    /// Look up a cached tile. Returns `None` if absent or expired.
    pub async fn get(
        &self,
        model: &str,
        var: &str,
        level: &str,
        fhour: u32,
        z: u32,
        x: u32,
        y: u32,
    ) -> Option<Vec<u8>> {
        let key = Self::key(model, var, level, fhour, z, x, y);
        let tiles = self.tiles.read().await;
        if let Some(entry) = tiles.get(&key) {
            if entry.created.elapsed() < self.ttl {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.data.clone());
            }
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Insert a rendered tile into the cache, evicting expired entries if
    /// the cache is near capacity.
    pub async fn put(
        &self,
        model: &str,
        var: &str,
        level: &str,
        fhour: u32,
        z: u32,
        x: u32,
        y: u32,
        data: Vec<u8>,
    ) {
        let key = Self::key(model, var, level, fhour, z, x, y);
        let size = data.len();
        let mut tiles = self.tiles.write().await;

        // Evict expired entries when approaching capacity.
        let total_size: usize = tiles.values().map(|e| e.size_bytes).sum();
        if total_size + size > self.max_size {
            let ttl = self.ttl;
            tiles.retain(|_, e| e.created.elapsed() < ttl);

            // If still over capacity after expiry eviction, drop oldest entries
            // until we have room.
            let mut current: usize = tiles.values().map(|e| e.size_bytes).sum();
            if current + size > self.max_size {
                let mut entries: Vec<(String, Instant, usize)> = tiles
                    .iter()
                    .map(|(k, e)| (k.clone(), e.created, e.size_bytes))
                    .collect();
                entries.sort_by_key(|(_, created, _)| *created);

                for (old_key, _, old_size) in entries {
                    if current + size <= self.max_size {
                        break;
                    }
                    tiles.remove(&old_key);
                    current -= old_size;
                }
            }
        }

        tiles.insert(
            key,
            CacheEntry {
                data,
                created: Instant::now(),
                size_bytes: size,
            },
        );
    }

    /// Invalidate all tiles matching a specific model run, e.g. when a new
    /// model run becomes available. Pass `None` to skip a filter field.
    pub async fn invalidate(
        &self,
        model: Option<&str>,
        var: Option<&str>,
        level: Option<&str>,
    ) -> usize {
        let mut tiles = self.tiles.write().await;
        let before = tiles.len();
        tiles.retain(|key, _| {
            let parts: Vec<&str> = key.splitn(4, '/').collect();
            if parts.len() < 3 {
                return true;
            }
            let dominated = model.map_or(false, |m| parts[0] == m)
                && var.map_or(true, |v| parts[1] == v)
                && level.map_or(true, |l| parts[2] == l);
            !dominated
        });
        before - tiles.len()
    }

    /// Return current cache statistics.
    pub async fn stats(&self) -> CacheStats {
        let tiles = self.tiles.read().await;
        let total_bytes: usize = tiles.values().map(|e| e.size_bytes).sum();
        let ttl = self.ttl;
        let expired = tiles
            .values()
            .filter(|e| e.created.elapsed() >= ttl)
            .count();
        CacheStats {
            entries: tiles.len(),
            total_bytes,
            max_bytes: self.max_size,
            expired_entries: expired,
            ttl_seconds: self.ttl.as_secs(),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
        }
    }

    /// Remove all entries from the cache.
    pub async fn clear(&self) {
        self.tiles.write().await.clear();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }
}

/// Snapshot of cache statistics, serializable to JSON for the `/api/cache/stats` endpoint.
#[derive(serde::Serialize, Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub total_bytes: usize,
    pub max_bytes: usize,
    pub expired_entries: usize,
    pub ttl_seconds: u64,
    pub hits: u64,
    pub misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_put_get() {
        let cache = TileCache::new(1, 60); // 1 MB, 60s TTL
        let data = vec![0x89, 0x50, 0x4E, 0x47]; // fake PNG header

        cache.put("hrrr", "CAPE", "surface", 0, 5, 8, 12, data.clone()).await;
        let got = cache.get("hrrr", "CAPE", "surface", 0, 5, 8, 12).await;
        assert_eq!(got, Some(data));

        // Different tile coords -> miss
        let miss = cache.get("hrrr", "CAPE", "surface", 0, 5, 9, 12).await;
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn ttl_expiry() {
        let cache = TileCache::new(1, 0); // 0s TTL -> immediately expired
        cache.put("hrrr", "T2", "2m", 1, 3, 2, 1, vec![1, 2, 3]).await;

        // Should miss because TTL is 0
        let got = cache.get("hrrr", "T2", "2m", 1, 3, 2, 1).await;
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn stats_and_clear() {
        let cache = TileCache::new(10, 300);
        cache.put("gfs", "TMP", "500mb", 6, 4, 1, 1, vec![0; 100]).await;
        cache.put("gfs", "TMP", "500mb", 6, 4, 1, 2, vec![0; 200]).await;

        let s = cache.stats().await;
        assert_eq!(s.entries, 2);
        assert_eq!(s.total_bytes, 300);

        cache.clear().await;
        let s = cache.stats().await;
        assert_eq!(s.entries, 0);
        assert_eq!(s.total_bytes, 0);
    }

    #[tokio::test]
    async fn eviction_under_pressure() {
        // 1 KB max cache
        let cache = TileCache::new(0, 300); // 0 MB = 0 bytes max
        // This will force LRU eviction on every insert since max is 0
        cache.put("hrrr", "CAPE", "sfc", 0, 5, 0, 0, vec![0; 100]).await;
        // The entry should still be inserted (we evict *before* inserting)
        let s = cache.stats().await;
        // With 0 max, the old entry gets evicted but new one is still inserted
        assert_eq!(s.entries, 1);
    }

    #[tokio::test]
    async fn hit_miss_counters() {
        let cache = TileCache::new(1, 300);
        cache.put("hrrr", "CAPE", "sfc", 0, 5, 0, 0, vec![1]).await;

        let _ = cache.get("hrrr", "CAPE", "sfc", 0, 5, 0, 0).await; // hit
        let _ = cache.get("hrrr", "CAPE", "sfc", 0, 5, 1, 0).await; // miss
        let _ = cache.get("hrrr", "CAPE", "sfc", 0, 5, 0, 0).await; // hit

        let s = cache.stats().await;
        assert_eq!(s.hits, 2);
        assert_eq!(s.misses, 1);
    }

    #[tokio::test]
    async fn invalidate_by_model() {
        let cache = TileCache::new(10, 300);
        cache.put("hrrr", "CAPE", "sfc", 0, 5, 0, 0, vec![1]).await;
        cache.put("hrrr", "T2", "2m", 0, 5, 0, 0, vec![2]).await;
        cache.put("gfs", "CAPE", "sfc", 0, 5, 0, 0, vec![3]).await;

        let removed = cache.invalidate(Some("hrrr"), None, None).await;
        assert_eq!(removed, 2);

        let s = cache.stats().await;
        assert_eq!(s.entries, 1); // only GFS remains
    }
}
