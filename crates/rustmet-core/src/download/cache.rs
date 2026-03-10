use std::path::PathBuf;

/// Simple file-based cache for downloaded GRIB2 data.
///
/// Stores files in `~/.rustmet/cache/` using a hash of the URL as the filename.
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// Create a new cache, initializing the cache directory if needed.
    pub fn new() -> Self {
        let dir = cache_dir();
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    /// Create a cache with a custom directory.
    pub fn with_dir(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    /// Get the cache file path for a given URL.
    fn cache_path(&self, url: &str) -> PathBuf {
        let hash = simple_hash(url);
        self.dir.join(hash)
    }

    /// Check if a URL is cached and return the cached bytes if so.
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.cache_path(url);
        std::fs::read(&path).ok()
    }

    /// Store bytes in the cache for a given URL.
    pub fn put(&self, url: &str, data: &[u8]) {
        let path = self.cache_path(url);
        if let Err(e) = std::fs::write(&path, data) {
            eprintln!("Warning: failed to write cache file {:?}: {}", path, e);
        }
    }

    /// Check if a URL is cached without reading the data.
    pub fn contains(&self, url: &str) -> bool {
        self.cache_path(url).exists()
    }

    /// Remove a cached entry.
    pub fn remove(&self, url: &str) {
        let path = self.cache_path(url);
        std::fs::remove_file(&path).ok();
    }

    /// Return the cache directory path.
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

/// Determine the cache directory, defaulting to ~/.rustmet/cache/
fn cache_dir() -> PathBuf {
    if let Some(home) = home_dir() {
        home.join(".rustmet").join("cache")
    } else {
        PathBuf::from(".rustmet").join("cache")
    }
}

/// Get the user's home directory.
fn home_dir() -> Option<PathBuf> {
    // Try HOME (Unix, Git Bash on Windows), then USERPROFILE (Windows)
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Simple string hash for generating cache filenames.
///
/// Produces a 16-character hex string with a .grib2 extension.
fn simple_hash(s: &str) -> String {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    format!("{:016x}.grib2", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_hash_deterministic() {
        let h1 = simple_hash("https://example.com/data.grib2");
        let h2 = simple_hash("https://example.com/data.grib2");
        assert_eq!(h1, h2);
        assert!(h1.ends_with(".grib2"));
        assert_eq!(h1.len(), 22); // 16 hex chars + ".grib2"
    }

    #[test]
    fn test_simple_hash_different_urls() {
        let h1 = simple_hash("https://example.com/a.grib2");
        let h2 = simple_hash("https://example.com/b.grib2");
        assert_ne!(h1, h2);
    }
}
