//! HTTP tool for making HTTP requests

use serde::{Deserialize, Serialize};
use std::time::Duration;

use cloudcoder_core::CloudCoderError;

/// HTTP method
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// HTTP tool input schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpToolInput {
    /// URL to request
    pub url: String,
    /// HTTP method (default: GET)
    #[serde(default)]
    pub method: HttpMethod,
    /// Request headers
    pub headers: Option<std::collections::HashMap<String, String>>,
    /// Request body
    pub body: Option<String>,
    /// Query parameters
    pub query: Option<std::collections::HashMap<String, String>>,
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Follow redirects
    #[serde(default = "default_follow_redirects")]
    pub follow_redirects: bool,
    /// Verify TLS certificates
    #[serde(default = "default_verify_tls")]
    pub verify_tls: bool,
}

fn default_follow_redirects() -> bool { true }
fn default_verify_tls() -> bool { true }

/// HTTP tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpToolOutput {
    /// HTTP status code
    pub status_code: u16,
    /// Response headers
    pub headers: std::collections::HashMap<String, String>,
    /// Response body
    pub body: Option<String>,
    /// Response time in milliseconds
    pub duration_ms: u64,
    /// Whether the request succeeded (2xx status)
    pub success: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// HTTP tool for making HTTP requests
pub struct HttpTool {
    default_timeout_ms: u64,
}

impl HttpTool {
    pub fn new() -> Self {
        Self {
            default_timeout_ms: 30_000, // 30 seconds
        }
    }

    pub fn name(&self) -> &str {
        "HttpTool"
    }

    pub fn description(&self) -> &str {
        "Make HTTP requests with customizable headers, body, and timeout"
    }

    pub async fn execute(&self, input: HttpToolInput) -> Result<HttpToolOutput, CloudCoderError> {
        let start = std::time::Instant::now();
        let timeout_ms = input.timeout_ms.unwrap_or(self.default_timeout_ms);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(if input.follow_redirects {
                reqwest::redirect::Policy::default()
            } else {
                reqwest::redirect::Policy::none()
            })
            .danger_accept_invalid_certs(!input.verify_tls)
            .build()
            .map_err(|e| CloudCoderError::Api(format!("Failed to create HTTP client: {}", e)))?;

        // Build URL with query parameters
        let mut url = reqwest::Url::parse(&input.url)
            .map_err(|e| CloudCoderError::Api(format!("Invalid URL: {}", e)))?;

        if let Some(query) = &input.query {
            for (key, value) in query {
                url.query_pairs_mut().append_pair(key, value);
            }
        }

        // Build request
        let mut request = match input.method {
            HttpMethod::Get => client.get(url),
            HttpMethod::Post => client.post(url),
            HttpMethod::Put => client.put(url),
            HttpMethod::Patch => client.patch(url),
            HttpMethod::Delete => client.delete(url),
            HttpMethod::Head => client.head(url),
            HttpMethod::Options => client.request(reqwest::Method::OPTIONS, url),
        };

        // Add headers
        if let Some(headers) = &input.headers {
            for (key, value) in headers {
                request = request.header(key, value);
            }
        }

        // Add body
        if let Some(body) = &input.body {
            request = request.body(body.clone());
        }

        // Execute request
        let response = request
            .send()
            .await
            .map_err(|e| CloudCoderError::Api(format!("Request failed: {}", e)))?;

        let status_code = response.status().as_u16();
        let success = response.status().is_success();

        let response_headers: std::collections::HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body = response
            .text()
            .await
            .map_err(|e| CloudCoderError::Api(format!("Failed to read response: {}", e)))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(HttpToolOutput {
            status_code,
            headers: response_headers,
            body: Some(body),
            duration_ms,
            success,
            error: None,
        })
    }
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::tools::Tool for HttpTool {
    fn name(&self) -> &str {
        "HttpTool"
    }

    fn description(&self) -> &str {
        "Make HTTP requests with customizable headers, body, and timeout"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_request() {
        let tool = HttpTool::new();
        let result = tool.execute(HttpToolInput {
            url: "https://httpbin.org/get".to_string(),
            method: HttpMethod::Get,
            headers: None,
            body: None,
            query: Some([("test".to_string(), "value".to_string())].into_iter().collect()),
            timeout_ms: Some(10000),
            follow_redirects: true,
            verify_tls: true,
        }).await;

        // May fail if no network, so we just check it doesn't panic
        if let Ok(output) = result {
            assert_eq!(output.status_code, 200);
        }
    }
}