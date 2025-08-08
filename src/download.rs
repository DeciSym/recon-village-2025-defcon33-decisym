use anyhow::{Context, Result};
use arti_client::{IsolationToken, StreamPrefs, TorClient, TorClientConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;
use tor_rtcompat::PreferredRuntime;
use tracing::{debug, info};

fn parse_chunked_body(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        // Find the end of the chunk size line
        let line_end = data[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .context("Invalid chunked encoding: no CRLF after chunk size")?;

        // Parse the chunk size (in hex)
        let size_str = std::str::from_utf8(&data[pos..pos + line_end])
            .context("Invalid chunk size encoding")?;
        let chunk_size =
            usize::from_str_radix(size_str.trim(), 16).context("Invalid chunk size hex")?;

        // Move past the size line and CRLF
        pos += line_end + 2;

        // If chunk size is 0, we've reached the end
        if chunk_size == 0 {
            break;
        }

        // Read the chunk data
        if pos + chunk_size > data.len() {
            anyhow::bail!("Incomplete chunk data");
        }
        result.extend_from_slice(&data[pos..pos + chunk_size]);

        // Move past the chunk data and the trailing CRLF
        pos += chunk_size + 2;
    }

    Ok(result)
}

fn extract_filename_from_headers(headers: &str) -> Option<String> {
    // Look for Content-Disposition header
    for line in headers.lines() {
        if line.to_lowercase().starts_with("content-disposition:") {
            // Extract filename from header like: Content-Disposition: attachment; filename="example.txt"
            if let Some(filename_part) = line.split("filename=").nth(1) {
                let filename = filename_part
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !filename.is_empty() {
                    return Some(filename.to_string());
                }
            }
        }
    }
    None
}

fn extract_filename_from_url(url: &url::Url, default_filename: &str) -> String {
    // Get the last segment of the path
    let path = url.path();
    let filename = path.split('/').next_back().unwrap_or("download");

    // If no filename or just a slash, use the default
    if filename.is_empty() || filename == "/" {
        default_filename.to_string()
    } else {
        filename.to_string()
    }
}

pub struct TorDownloader {
    client: Arc<TorClient<PreferredRuntime>>,
    rate_limit_delay: Duration,
    user_agent: String,
    max_redirects: u32,
    insecure: bool,
    buffer_size: usize,
    default_filename: String,
    isolation_token: IsolationToken, // Single isolation token for the entire session
}

impl TorDownloader {
    /// Creates a new `TorDownloader` instance with a bootstrapped Tor client.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The Tor client fails to initialize
    /// - The Tor client fails to bootstrap
    pub async fn new() -> Result<Self> {
        info!("Initializing Tor client...");

        let config = TorClientConfig::default();

        // Try to create and bootstrap with retries
        let mut attempts = 0;
        let max_attempts = 3;

        let client = loop {
            attempts += 1;
            info!("Tor bootstrap attempt {} of {}", attempts, max_attempts);

            match TorClient::create_bootstrapped(config.clone()).await {
                Ok(client) => break client,
                Err(e) => {
                    if attempts >= max_attempts {
                        return Err(e).context(
                            "Failed to create and bootstrap Tor client after multiple attempts",
                        );
                    }
                    info!(
                        "Bootstrap attempt {} failed, retrying in 5 seconds...",
                        attempts
                    );
                    sleep(Duration::from_secs(5)).await;
                }
            }
        };

        info!("Tor client bootstrapped successfully");

        // Create a single isolation token for this session
        let isolation_token = IsolationToken::new();
        info!("Created session isolation token for circuit reuse");

        Ok(Self {
            client: Arc::new(client),
            rate_limit_delay: Duration::from_secs(1),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36".to_string(),
            max_redirects: 5,
            insecure: false,
            buffer_size: 8192,
            default_filename: "index.html".to_string(),
            isolation_token,
        })
    }

    pub fn set_rate_limit_delay(&mut self, seconds: u64) {
        self.rate_limit_delay = Duration::from_secs(seconds);
    }

    pub fn set_user_agent(&mut self, user_agent: &str) {
        self.user_agent = user_agent.to_string();
    }

    pub fn set_max_redirects(&mut self, max_redirects: u32) {
        self.max_redirects = max_redirects;
    }

    pub fn set_insecure(&mut self, insecure: bool) {
        self.insecure = insecure;
    }

    pub fn set_buffer_size(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
    }

    pub fn set_default_filename(&mut self, default_filename: &str) {
        self.default_filename = default_filename.to_string();
    }

    /// Get the SOCKS port for browser configuration
    /// Note: Arti doesn't expose a SOCKS proxy - this returns 0 to indicate no proxy
    pub fn get_socks_port(&self) -> u16 {
        // Arti doesn't provide SOCKS proxy functionality
        // Return 0 to indicate no proxy available
        0
    }

    /// Get a reference to the Tor client for creating a SOCKS bridge
    pub fn tor_client(&self) -> Arc<TorClient<PreferredRuntime>> {
        Arc::clone(&self.client)
    }

    /// Downloads a file from the given URL through Tor.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to download
    ///
    /// # Returns
    ///
    /// Returns the filename where the content was saved.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is invalid
    /// - The connection fails
    /// - The server returns an error status
    /// - File I/O operations fail
    /// - TLS certificate validation fails (unless insecure mode is enabled)
    ///
    /// # Panics
    ///
    /// This function will panic if the rate limit delay is set to a value that
    /// causes an overflow when calculating sleep duration.
    pub async fn download_file(&self, url: &str) -> Result<String> {
        let mut current_url = url.to_string();
        let mut redirects = 0;
        loop {
            if redirects >= self.max_redirects {
                anyhow::bail!("Too many redirects");
            }

            info!("Starting download from: {}", current_url);

            // Respect rate limit
            sleep(self.rate_limit_delay).await;

            let parsed_url = url::Url::parse(&current_url).context("Failed to parse URL")?;
            let host = parsed_url.host_str().context("URL must have a host")?;
            let port = parsed_url.port_or_known_default().unwrap_or(443);

            info!("Connecting to {}:{} through Tor...", host, port);

            // Connect through Tor using the session's isolation token
            // This reuses the same circuit for all connections in this download session
            let mut prefs = StreamPrefs::new();
            prefs.set_isolation(self.isolation_token.clone());

            debug!(
                "Reusing session circuit for connection to {}:{}",
                host, port
            );

            let stream = self
                .client
                .connect_with_prefs((host, port), &prefs)
                .await
                .context("Failed to connect through Tor")?;

            // For HTTPS, we need to use TLS
            if parsed_url.scheme() == "https" {
                use tokio_native_tls::TlsConnector;
                let tls = TlsConnector::from(
                    native_tls::TlsConnector::builder()
                        .danger_accept_invalid_certs(self.insecure)
                        .build()
                        .context("Failed to build TLS connector")?,
                );

                let mut stream = tls
                    .connect(host, stream)
                    .await
                    .context("Failed to establish TLS connection")?;

                // Send HTTP request with configured User-Agent
                let request = format!(
                    "GET {} HTTP/1.1\r\n\
                 Host: {}\r\n\
                 User-Agent: {}\r\n\
                 Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7\r\n\
                 Accept-Language: en-US,en;q=0.9\r\n\
                 Connection: close\r\n\
                 Upgrade-Insecure-Requests: 1\r\n\
                 Sec-Fetch-Dest: document\r\n\
                 Sec-Fetch-Mode: navigate\r\n\
                 Sec-Fetch-Site: none\r\n\
                 Sec-Fetch-User: ?1\r\n\
                 \r\n",
                    if parsed_url.path().is_empty() {
                        "/"
                    } else {
                        parsed_url.path()
                    },
                    host,
                    self.user_agent
                );

                info!("Sending request with Chrome User-Agent");
                stream
                    .write_all(request.as_bytes())
                    .await
                    .context("Failed to send HTTPS request")?;

                stream.flush().await.context("Failed to flush stream")?;

                // Read response with a buffer
                let mut response = Vec::new();
                let mut buffer = vec![0u8; self.buffer_size];
                loop {
                    match stream.read(&mut buffer).await {
                        Ok(0) => break, // EOF
                        Ok(n) => response.extend_from_slice(&buffer[..n]),
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Err(e) => return Err(e).context("Failed to read response")?,
                    }
                }

                // Parse HTTP response
                let response_str = String::from_utf8_lossy(&response);
                info!("Response length: {} bytes", response.len());

                if let Some(body_start) = response_str.find("\r\n\r\n") {
                    let headers = &response_str[..body_start];
                    let raw_body = &response[body_start + 4..];

                    let status_line = headers.lines().next().unwrap_or("Unknown");
                    info!("Response status: {}", status_line);

                    // Check for redirects in the status line
                    if status_line.contains(" 301 ")
                        || status_line.contains(" 302 ")
                        || status_line.contains(" 303 ")
                        || status_line.contains(" 307 ")
                        || status_line.contains(" 308 ")
                    {
                        // Extract Location header
                        let mut found_location = false;
                        for line in headers.lines() {
                            if line.to_lowercase().starts_with("location:") {
                                let new_url = line
                                    .split(':')
                                    .skip(1)
                                    .collect::<Vec<_>>()
                                    .join(":")
                                    .trim()
                                    .to_string();
                                info!("Following redirect to: {}", new_url);

                                // Handle relative URLs
                                let redirect_url = if new_url.starts_with("http://")
                                    || new_url.starts_with("https://")
                                {
                                    new_url
                                } else if new_url.starts_with('/') {
                                    format!(
                                        "{}://{}{}",
                                        parsed_url.scheme(),
                                        parsed_url.host_str().unwrap_or(""),
                                        new_url
                                    )
                                } else {
                                    let base_path =
                                        parsed_url.path().rsplit_once('/').map_or("", |x| x.0);
                                    format!(
                                        "{}://{}{}/{}",
                                        parsed_url.scheme(),
                                        parsed_url.host_str().unwrap_or(""),
                                        base_path,
                                        new_url
                                    )
                                };

                                current_url = redirect_url;
                                redirects += 1;
                                found_location = true;
                                break;
                            }
                        }
                        if !found_location {
                            anyhow::bail!("Redirect response without Location header");
                        }
                        continue; // Continue to next iteration of the loop
                    }

                    // Check for rate limiting
                    if status_line.contains(" 429 ") {
                        info!("Rate limited (429 Too Many Requests)");

                        // Look for Retry-After header
                        let mut retry_after_seconds = 60u64; // Default to 60 seconds

                        for line in headers.lines() {
                            if line.to_lowercase().starts_with("retry-after:") {
                                let value = line.split(':').nth(1).unwrap_or("").trim();

                                // Try to parse as seconds (integer)
                                if let Ok(seconds) = value.parse::<u64>() {
                                    retry_after_seconds = seconds;
                                    info!("Server requests retry after {} seconds", seconds);
                                } else {
                                    // Could be an HTTP date, but for simplicity we'll use default
                                    info!("Retry-After header present but using default wait time");
                                }
                                break;
                            }
                        }

                        info!("Waiting {} seconds before retry...", retry_after_seconds);
                        sleep(Duration::from_secs(retry_after_seconds)).await;

                        // Continue to retry the request
                        continue;
                    }

                    if !headers.contains("200 OK") && !headers.contains("HTTP/2 200") {
                        anyhow::bail!("HTTP request failed: {}", status_line);
                    }

                    // Check if response is chunked
                    let body = if headers
                        .to_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        info!("Response uses chunked encoding");
                        parse_chunked_body(raw_body)?
                    } else {
                        raw_body.to_vec()
                    };

                    info!("Body length: {} bytes", body.len());

                    // Determine filename
                    let filename = extract_filename_from_headers(headers).unwrap_or_else(|| {
                        extract_filename_from_url(&parsed_url, &self.default_filename)
                    });

                    info!("Saving to filename: {}", filename);

                    // Write body to file
                    let mut file = File::create(&filename)
                        .await
                        .context("Failed to create output file")?;

                    file.write_all(&body)
                        .await
                        .context("Failed to write to output file")?;

                    info!("Download completed successfully");
                    return Ok(filename);
                }

                // No HTTP response body delimiter found
                info!("No HTTP response body delimiter found");
                info!(
                    "First 200 chars of response: {}",
                    &response_str.chars().take(200).collect::<String>()
                );
                anyhow::bail!("Invalid HTTP response");
            }

            // Only HTTPS is supported
            anyhow::bail!("Only HTTPS is supported in this implementation");
        } // End of loop
    }

    /// Downloads from a web service (API endpoint) through Tor with custom headers and body.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to send the request to
    /// * `method` - HTTP method (GET, POST, etc.)
    /// * `headers` - Additional headers to include
    /// * `body` - Optional request body
    ///
    /// # Returns
    ///
    /// Returns the response body and a suggested filename.
    pub async fn download_web_service(
        &self,
        url: &str,
        method: &str,
        headers: &[String],
        body: Option<&str>,
    ) -> Result<(Vec<u8>, String)> {
        info!("Starting web service request to: {}", url);
        info!("Method: {}", method);

        // Respect rate limit
        sleep(self.rate_limit_delay).await;

        let parsed_url = url::Url::parse(url).context("Failed to parse URL")?;
        let host = parsed_url.host_str().context("URL must have a host")?;
        let port = parsed_url.port_or_known_default().unwrap_or(443);

        info!("Connecting to {}:{} through Tor...", host, port);

        // Connect through Tor using the session's isolation token
        let mut prefs = StreamPrefs::new();
        prefs.set_isolation(self.isolation_token.clone());

        debug!(
            "Reusing session circuit for web service connection to {}:{}",
            host, port
        );

        let stream = self
            .client
            .connect_with_prefs((host, port), &prefs)
            .await
            .context("Failed to connect through Tor")?;

        // For HTTPS, we need to use TLS
        if parsed_url.scheme() == "https" {
            use tokio_native_tls::TlsConnector;
            let tls = TlsConnector::from(
                native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(self.insecure)
                    .build()
                    .context("Failed to build TLS connector")?,
            );

            let mut stream = tls
                .connect(host, stream)
                .await
                .context("Failed to establish TLS connection")?;

            // Build the request
            let path = if parsed_url.path().is_empty() {
                "/"
            } else {
                parsed_url.path()
            };

            let mut request = format!(
                "{} {} HTTP/1.1\r\n\
                 Host: {}\r\n\
                 User-Agent: {}\r\n",
                method.to_uppercase(),
                path,
                host,
                self.user_agent
            );

            // Add custom headers
            for header in headers {
                request.push_str(header);
                request.push_str("\r\n");
            }

            // Add Content-Length if we have a body
            if let Some(body_content) = body {
                request.push_str(&format!("Content-Length: {}\r\n", body_content.len()));
            }

            // End headers
            request.push_str("\r\n");

            // Add body if present
            if let Some(body_content) = body {
                request.push_str(body_content);
            }

            info!(
                "Sending {} request with {} custom headers",
                method,
                headers.len()
            );
            stream
                .write_all(request.as_bytes())
                .await
                .context("Failed to send HTTPS request")?;

            stream.flush().await.context("Failed to flush stream")?;

            // Read response with a buffer
            let mut response = Vec::new();
            let mut buffer = vec![0u8; self.buffer_size];
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => response.extend_from_slice(&buffer[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(e).context("Failed to read response")?,
                }
            }

            // Parse HTTP response
            let response_str = String::from_utf8_lossy(&response);

            // Find status line
            let lines: Vec<&str> = response_str.lines().collect();
            if lines.is_empty() {
                anyhow::bail!("Empty response");
            }

            let status_line = lines[0];
            let status_parts: Vec<&str> = status_line.split_whitespace().collect();
            if status_parts.len() < 2 {
                anyhow::bail!("Invalid HTTP status line");
            }

            let status_code: u16 = status_parts[1]
                .parse()
                .context("Failed to parse status code")?;

            info!("Response status: {}", status_code);

            if status_code >= 400 {
                anyhow::bail!("HTTP error: {}", status_code);
            }

            // Find the headers/body separator
            if let Some(separator_pos) = response.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&response[..separator_pos])
                    .context("Invalid UTF-8 in headers")?;
                let raw_body = &response[separator_pos + 4..];

                // Check if response is chunked
                let body = if headers
                    .to_lowercase()
                    .contains("transfer-encoding: chunked")
                {
                    info!("Response uses chunked encoding");
                    parse_chunked_body(raw_body)?
                } else {
                    raw_body.to_vec()
                };

                info!("Response body length: {} bytes", body.len());

                // Determine filename based on content type or URL
                let filename = if headers
                    .to_lowercase()
                    .contains("content-type: application/json")
                {
                    "response.json".to_string()
                } else if headers.to_lowercase().contains("content-type: text/csv") {
                    "response.csv".to_string()
                } else if headers
                    .to_lowercase()
                    .contains("content-type: application/sparql-results+json")
                {
                    "response.json".to_string()
                } else {
                    extract_filename_from_headers(headers)
                        .unwrap_or_else(|| "response.txt".to_string())
                };

                return Ok((body, filename));
            }

            anyhow::bail!("Invalid HTTP response: no body delimiter found");
        }

        // Only HTTPS is supported
        anyhow::bail!("Only HTTPS is supported for web services");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tor_client_creation() {
        let result = TorDownloader::new().await;
        assert!(result.is_ok());
    }
}
