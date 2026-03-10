use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rayon::prelude::*;

/// HTTP client for downloading GRIB2 data with byte-range support.
///
/// Uses ureq (blocking HTTP) with rustls + rustcrypto for TLS.
/// Supports configurable timeouts, retry with exponential backoff,
/// and parallel chunk downloads.
pub struct DownloadClient {
    agent: ureq::Agent,
    timeout: Duration,
    max_retries: u32,
}

/// Maximum body size for full file downloads (500 MB).
const MAX_BODY_SIZE: u64 = 500 * 1024 * 1024;

/// Default timeout per request.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum number of retries.
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Backoff durations for each retry attempt.
const BACKOFF_DURATIONS: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_millis(1000),
    Duration::from_millis(2000),
];

/// Configuration for creating a DownloadClient.
pub struct DownloadConfig {
    /// Timeout per HTTP request.
    pub timeout: Duration,
    /// Maximum number of retry attempts.
    pub max_retries: u32,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

/// Check whether an error from ureq should be retried.
///
/// Retries on: connection/transport errors, 429 (rate limit),
/// 500, 502, 503, 504 (server errors).
/// Does NOT retry on: 400, 404, or other 4xx client errors.
fn is_retryable(err: &ureq::Error) -> bool {
    match err {
        ureq::Error::StatusCode(code) => {
            let c = *code;
            c == 429 || c == 500 || c == 502 || c == 503 || c == 504
        }
        // Timeout, DNS, connection reset, etc. — all retryable.
        _ => true,
    }
}

impl DownloadClient {
    /// Create a new download client with TLS configured via rustls-rustcrypto.
    ///
    /// Uses ureq's built-in TlsConfig with the rustcrypto provider and
    /// webpki root certificates (Mozilla's CA bundle).
    pub fn new() -> Result<Self, String> {
        Self::new_with_config(DownloadConfig::default())
    }

    /// Create a new download client with custom timeout and retry settings.
    pub fn new_with_config(config: DownloadConfig) -> Result<Self, String> {
        // Install the rustcrypto provider as the process-wide default.
        rustls::crypto::CryptoProvider::install_default(rustls_rustcrypto::provider()).ok();

        let crypto = Arc::new(rustls_rustcrypto::provider());

        let agent = ureq::Agent::config_builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .provider(ureq::tls::TlsProvider::Rustls)
                    .root_certs(ureq::tls::RootCerts::WebPki)
                    .unversioned_rustls_crypto_provider(crypto)
                    .build(),
            )
            .timeout_global(Some(config.timeout))
            .build()
            .new_agent();

        Ok(Self {
            agent,
            timeout: config.timeout,
            max_retries: config.max_retries,
        })
    }

    /// Execute a request-producing closure with retry and exponential backoff.
    ///
    /// `attempt_fn` is called on each attempt and must produce the final result
    /// or a ureq::Error. This avoids needing to name the ureq Response type.
    fn with_retry<T, F>(&self, url: &str, attempt_fn: F) -> Result<T, String>
    where
        F: Fn() -> Result<T, ureq::Error>,
    {
        let mut last_err = String::new();

        for attempt in 0..=self.max_retries {
            match attempt_fn() {
                Ok(val) => return Ok(val),
                Err(e) => {
                    last_err = format!("{}", e);

                    if attempt < self.max_retries && is_retryable(&e) {
                        let backoff = BACKOFF_DURATIONS
                            .get(attempt as usize)
                            .copied()
                            .unwrap_or(BACKOFF_DURATIONS[BACKOFF_DURATIONS.len() - 1]);
                        eprintln!(
                            "  Retry {}/{} for {} after {:?} ({})",
                            attempt + 1,
                            self.max_retries,
                            url,
                            backoff,
                            e
                        );
                        std::thread::sleep(backoff);
                    } else {
                        break;
                    }
                }
            }
        }

        Err(format!("HTTP request failed for {}: {}", url, last_err))
    }

    /// Download a full URL and return the response body as bytes.
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        self.with_retry(url, || {
            let mut response = self.agent.get(url).call()?;
            let data = response
                .body_mut()
                .with_config()
                .limit(MAX_BODY_SIZE)
                .read_to_vec()?;
            Ok(data)
        })
    }

    /// Download a URL and return the response body as a string (for .idx files).
    pub fn get_text(&self, url: &str) -> Result<String, String> {
        self.with_retry(url, || {
            let mut response = self.agent.get(url).call()?;
            let text = response
                .body_mut()
                .read_to_string()?;
            Ok(text)
        })
    }

    /// Download a specific byte range from a URL.
    ///
    /// Sets the `Range: bytes=start-end` header. If `end` is `u64::MAX`,
    /// the range is open-ended (`bytes=start-`).
    pub fn get_range(&self, url: &str, start: u64, end: u64) -> Result<Vec<u8>, String> {
        let range_header = if end == u64::MAX {
            format!("bytes={}-", start)
        } else {
            format!("bytes={}-{}", start, end)
        };

        self.with_retry(url, || {
            let mut response = self
                .agent
                .get(url)
                .header("Range", &range_header)
                .call()?;
            let data = response
                .body_mut()
                .with_config()
                .limit(MAX_BODY_SIZE)
                .read_to_vec()?;
            Ok(data)
        })
    }

    /// Download multiple byte ranges from a URL in parallel and concatenate the results.
    ///
    /// Each range is downloaded as a separate HTTP request with a Range header.
    /// Uses rayon to download chunks concurrently. Progress is printed to stderr.
    pub fn get_ranges(&self, url: &str, ranges: &[(u64, u64)]) -> Result<Vec<u8>, String> {
        let total = ranges.len();
        if total == 0 {
            return Ok(Vec::new());
        }

        let completed = AtomicUsize::new(0);

        // Download all chunks in parallel, preserving order.
        let results: Vec<Result<Vec<u8>, String>> = ranges
            .par_iter()
            .map(|&(start, end)| {
                let data = self.get_range(url, start, end)?;
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                eprint!("\r  Downloading chunks {}/{}...", done, total);
                Ok(data)
            })
            .collect();

        // Concatenate results in order, propagating the first error.
        let mut combined = Vec::new();
        for result in results {
            combined.extend_from_slice(&result?);
        }

        eprintln!(
            "\r  Downloaded {} chunks, {} bytes total.    ",
            total,
            combined.len()
        );

        Ok(combined)
    }
}
