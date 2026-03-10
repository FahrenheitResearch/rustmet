use std::sync::Arc;

/// HTTP client for downloading GRIB2 data with byte-range support.
///
/// Uses ureq (blocking HTTP) with rustls + rustcrypto for TLS.
pub struct DownloadClient {
    agent: ureq::Agent,
}

/// Maximum body size for full file downloads (500 MB).
const MAX_BODY_SIZE: u64 = 500 * 1024 * 1024;

impl DownloadClient {
    /// Create a new download client with TLS configured via rustls-rustcrypto.
    ///
    /// Uses ureq's built-in TlsConfig with the rustcrypto provider and
    /// webpki root certificates (Mozilla's CA bundle).
    pub fn new() -> Result<Self, String> {
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
            .build()
            .new_agent();

        Ok(Self { agent })
    }

    /// Download a full URL and return the response body as bytes.
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("HTTP GET failed for {}: {}", url, e))?;

        let data = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_SIZE)
            .read_to_vec()
            .map_err(|e| format!("Failed to read response body from {}: {}", url, e))?;

        Ok(data)
    }

    /// Download a URL and return the response body as a string (for .idx files).
    pub fn get_text(&self, url: &str) -> Result<String, String> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("HTTP GET failed for {}: {}", url, e))?;

        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("Failed to read response text from {}: {}", url, e))?;

        Ok(text)
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

        let mut response = self
            .agent
            .get(url)
            .header("Range", &range_header)
            .call()
            .map_err(|e| format!("HTTP range GET failed for {} ({}): {}", url, range_header, e))?;

        let data = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_SIZE)
            .read_to_vec()
            .map_err(|e| format!("Failed to read range response from {}: {}", url, e))?;

        Ok(data)
    }

    /// Download multiple byte ranges from a URL and concatenate the results.
    ///
    /// Each range is downloaded as a separate HTTP request with a Range header.
    /// Progress is printed to stderr.
    pub fn get_ranges(&self, url: &str, ranges: &[(u64, u64)]) -> Result<Vec<u8>, String> {
        let mut combined = Vec::new();
        let total = ranges.len();

        for (i, &(start, end)) in ranges.iter().enumerate() {
            eprint!(
                "\r  Downloading chunk {}/{} (offset {})...",
                i + 1,
                total,
                start
            );

            let data = self.get_range(url, start, end)?;
            combined.extend_from_slice(&data);
        }

        if total > 0 {
            eprintln!(
                "\r  Downloaded {} chunks, {} bytes total.    ",
                total,
                combined.len()
            );
        }

        Ok(combined)
    }
}
