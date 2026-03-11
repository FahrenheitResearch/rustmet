// cache.rs — Simple file-based cache for bandwidth savings
//
// Bandwidth impact: Eliminates redundant requests entirely
// - NWS /points/{lat},{lon} cached 24h (grid doesn't change)
// - Forecasts cached 1h
// - Alert state cached 5min
// - Station lookups cached forever (static data)

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct DiskCache {
    cache_dir: PathBuf,
}

impl DiskCache {
    /// Create a new DiskCache. Creates the cache directory if it doesn't exist.
    pub fn new() -> Self {
        let cache_dir = if let Some(home) = home_dir() {
            home.join(".wx-lite").join("cache")
        } else {
            PathBuf::from(".wx-lite").join("cache")
        };
        let _ = fs::create_dir_all(&cache_dir);
        Self { cache_dir }
    }

    /// Get a cached value if it exists and hasn't expired.
    /// `max_age_secs` of 0 means never expire.
    pub fn get(&self, key: &str, max_age_secs: u64) -> Option<String> {
        let data_path = self.cache_dir.join(format!("{}.json", key));
        let meta_path = self.cache_dir.join(format!("{}.meta", key));

        // Read the metadata (timestamp)
        let meta_contents = fs::read_to_string(&meta_path).ok()?;
        let cached_time: u64 = meta_contents.trim().parse().ok()?;

        // Check expiry (max_age_secs == 0 means never expire)
        if max_age_secs > 0 {
            let now = now_epoch_secs();
            if now.saturating_sub(cached_time) > max_age_secs {
                return None;
            }
        }

        // Read and return the cached data
        fs::read_to_string(&data_path).ok()
    }

    /// Write a value to the cache with the current timestamp.
    pub fn set(&self, key: &str, value: &str) {
        let data_path = self.cache_dir.join(format!("{}.json", key));
        let meta_path = self.cache_dir.join(format!("{}.meta", key));

        let now = now_epoch_secs();
        let _ = fs::write(&data_path, value);
        let _ = fs::write(&meta_path, now.to_string());
    }

    /// Create a deterministic cache key from parts.
    /// Sanitizes parts to be filesystem-safe.
    pub fn cache_key(parts: &[&str]) -> String {
        parts
            .iter()
            .map(|p| {
                p.chars()
                    .map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '_' })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("__")
    }
}

/// Get the current time as seconds since UNIX epoch.
fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Cross-platform home directory detection.
fn home_dir() -> Option<PathBuf> {
    // Try HOME first (Unix, Git Bash on Windows), then USERPROFILE (Windows native)
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_simple() {
        let key = DiskCache::cache_key(&["points", "35.2", "-97.4"]);
        assert_eq!(key, "points__35.2__-97.4");
    }

    #[test]
    fn test_cache_key_sanitizes() {
        let key = DiskCache::cache_key(&["forecast", "35.2,-97.4", "hourly"]);
        assert_eq!(key, "forecast__35.2_-97.4__hourly");
    }

    #[test]
    fn test_set_and_get() {
        let cache = DiskCache::new();
        let key = DiskCache::cache_key(&["test", "unit"]);
        cache.set(&key, r#"{"test": true}"#);

        let result = cache.get(&key, 3600);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), r#"{"test": true}"#);

        // Clean up
        let _ = std::fs::remove_file(cache.cache_dir.join(format!("{}.json", key)));
        let _ = std::fs::remove_file(cache.cache_dir.join(format!("{}.meta", key)));
    }

    #[test]
    fn test_expired_returns_none() {
        let cache = DiskCache::new();
        let key = DiskCache::cache_key(&["test", "expired"]);

        // Write with a timestamp far in the past by directly writing meta
        let data_path = cache.cache_dir.join(format!("{}.json", key));
        let meta_path = cache.cache_dir.join(format!("{}.meta", key));
        let _ = std::fs::write(&data_path, "old");
        let _ = std::fs::write(&meta_path, "1000000000"); // year 2001

        let result = cache.get(&key, 60); // 60s max age
        assert!(result.is_none());

        // Clean up
        let _ = std::fs::remove_file(&data_path);
        let _ = std::fs::remove_file(&meta_path);
    }
}
