//! HTTP client module with retry mechanisms and WeChat API integration.
//!
//! This module provides secure HTTP client functionality with:
//! - Request size limits to prevent DoS attacks
//! - Timeout configuration for reliability
//! - Retry mechanisms with exponential backoff
//! - Safe download limits for external content

use crate::config::{Config, RetryConfig, SecurityConfig};
use crate::error::{Result, WeChatError};
use crate::traits::HttpClient;
use reqwest::{Client, Response, multipart};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

// Note: RetryConfig and SecurityConfig are re-exported from config module for backward compatibility

/// HTTP client wrapper for WeChat API calls with automatic retry and token management.
#[derive(Debug, Clone)]
pub struct WeChatHttpClient {
    client: Client,
    config: Config,
}

impl WeChatHttpClient {
    /// Creates a new WeChat HTTP client.
    pub fn new() -> Result<Self> {
        Self::with_config(Config::default())
    }

    /// Creates a new client with custom configuration.
    pub fn with_config(config: Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.request_timeout())
            .connect_timeout(config.connect_timeout())
            .user_agent(&config.http.user_agent)
            .build()?;

        Ok(Self { client, config })
    }

    /// Creates a new client with custom retry configuration (legacy).
    pub fn with_retry_config(retry_config: RetryConfig) -> Result<Self> {
        let config = Config {
            retry: retry_config,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Creates a new client with custom security configuration (legacy).
    pub fn with_security_config(security_config: SecurityConfig) -> Result<Self> {
        let config = Config {
            security: security_config,
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Makes a GET request with access token.
    pub async fn get_with_token(&self, endpoint: &str, access_token: &str) -> Result<Response> {
        let url = format!(
            "{}{}?access_token={}",
            self.config.http.base_url, endpoint, access_token
        );
        self.execute_with_retry(|| self.client.get(&url).send())
            .await
    }

    /// Makes a POST request with JSON body and access token.
    pub async fn post_json_with_token<T: Serialize>(
        &self,
        endpoint: &str,
        access_token: &str,
        body: &T,
    ) -> Result<Response> {
        let url = format!(
            "{}{}?access_token={}",
            self.config.http.base_url, endpoint, access_token
        );
        self.execute_with_retry(|| self.client.post(&url).json(body).send())
            .await
    }

    /// Uploads a file using multipart form data with size validation.
    pub async fn upload_file(
        &self,
        endpoint: &str,
        access_token: &str,
        field_name: &str,
        file_data: Vec<u8>,
        filename: &str,
    ) -> Result<Response> {
        // Validate file size
        crate::utils::validate_file_size(
            file_data.len() as u64,
            self.config.security.max_upload_size,
            "upload",
        )
        .map_err(WeChatError::config_error)?;

        // Sanitize filename for security
        let safe_filename = crate::utils::sanitize_filename(filename);
        let url = format!(
            "{}{}?access_token={}",
            self.config.http.base_url, endpoint, access_token
        );

        // Guess MIME type from safe filename
        let mime_type = mime_guess::from_path(&safe_filename)
            .first_or_octet_stream()
            .to_string();

        // Clone data for each retry attempt
        let field_name = field_name.to_string();
        let url = url.clone();
        let client = self.client.clone();

        self.execute_with_retry(move || {
            let part = multipart::Part::bytes(file_data.clone())
                .file_name(safe_filename.clone())
                .mime_str(&mime_type)
                .unwrap();
            let form = multipart::Form::new().part(field_name.clone(), part);
            client.post(&url).multipart(form).send()
        })
        .await
    }

    /// Uploads a permanent material (for cover images) with size validation.
    pub async fn upload_material(
        &self,
        access_token: &str,
        material_type: &str,
        file_data: Vec<u8>,
        filename: &str,
    ) -> Result<Response> {
        // Validate file size
        crate::utils::validate_file_size(
            file_data.len() as u64,
            self.config.security.max_upload_size,
            "material",
        )
        .map_err(WeChatError::config_error)?;

        // Sanitize filename for security
        let safe_filename = crate::utils::sanitize_filename(filename);
        let url = format!(
            "{}{}?access_token={}&type={}",
            self.config.http.base_url,
            "/cgi-bin/material/add_material",
            access_token,
            material_type
        );

        // Guess MIME type from safe filename
        let mime_type = mime_guess::from_path(&safe_filename)
            .first_or_octet_stream()
            .to_string();

        // Clone data for each retry attempt
        let url = url.clone();
        let client = self.client.clone();

        self.execute_with_retry(move || {
            let part = multipart::Part::bytes(file_data.clone())
                .file_name(safe_filename.clone())
                .mime_str(&mime_type)
                .unwrap();

            let form = multipart::Form::new().part("media", part);

            client.post(&url).multipart(form).send()
        })
        .await
    }

    /// Executes a request with intelligent retry logic.
    async fn execute_with_retry<F, Fut>(&self, mut operation: F) -> Result<Response>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<Response, reqwest::Error>>,
    {
        let mut last_error = None;
        let mut consecutive_failures = 0;

        for attempt in 1..=self.config.retry.max_attempts {
            match operation().await {
                Ok(response) => {
                    // Check for WeChat API errors in successful HTTP responses
                    if response.status().is_success() {
                        return Ok(response);
                    } else {
                        // Convert HTTP error to WeChatError
                        let status = response.status();
                        let error_text = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        let error = WeChatError::Internal {
                            message: format!("HTTP {status}: {error_text}"),
                        };

                        // Use error-specific retry logic
                        let max_retries = error.max_retries().min(self.config.retry.max_attempts);
                        if attempt >= max_retries || !error.is_retryable() {
                            return Err(error);
                        }

                        consecutive_failures += 1;
                        last_error = Some(error);
                    }
                }
                Err(e) => {
                    let error = WeChatError::Network {
                        message: e.to_string(),
                    };

                    // Use error-specific retry logic
                    let max_retries = error.max_retries().min(self.config.retry.max_attempts);
                    if attempt >= max_retries || !error.is_retryable() {
                        return Err(error);
                    }

                    consecutive_failures += 1;
                    last_error = Some(error);
                }
            }

            // Wait before retry with intelligent backoff
            if attempt < self.config.retry.max_attempts {
                // Get delay from the last error or use base delay
                let base_delay = last_error
                    .as_ref()
                    .map(|e| e.retry_delay())
                    .unwrap_or(self.config.retry_base_delay());

                // Add jitter to prevent thundering herd
                let actual_delay = if self.config.retry.enable_jitter {
                    let jitter = fastrand::u64(0..=base_delay.as_millis() as u64 / 4);
                    base_delay + Duration::from_millis(jitter)
                } else {
                    base_delay
                };

                // Exponential backoff for consecutive failures
                let backoff_multiplier = (consecutive_failures as f64).min(4.0);
                let final_delay = std::cmp::min(
                    Duration::from_millis(
                        (actual_delay.as_millis() as f64
                            * self.config.retry.backoff_factor.powf(backoff_multiplier))
                            as u64,
                    ),
                    self.config.retry_max_delay(),
                );

                warn!(
                    "Request failed (attempt {}/{}), retrying in {:?} (consecutive failures: {})",
                    attempt, self.config.retry.max_attempts, final_delay, consecutive_failures
                );

                sleep(final_delay).await;
            }
        }

        Err(last_error.unwrap_or_else(|| WeChatError::Internal {
            message: "Retry loop completed without error".to_string(),
        }))
    }

    /// Downloads content from a URL.
    pub async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .execute_with_retry(|| self.client.get(url).send())
            .await?;

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Downloads content from a URL with size limits and streaming.
    pub async fn download_with_limit(&self, url: &str, max_size: u64) -> Result<Vec<u8>> {
        // Use the smaller of provided max_size or security config max
        let effective_max_size = max_size.min(self.config.security.max_download_size);
        use futures::StreamExt;

        let response = self
            .execute_with_retry(|| self.client.get(url).send())
            .await?;

        // Check content length if available
        if let Some(content_length) = response.content_length()
            && content_length > effective_max_size
        {
            return Err(WeChatError::ImageUpload {
                path: url.to_string(),
                reason: format!(
                    "Content too large: {content_length} bytes (max: {effective_max_size} bytes)"
                ),
            });
        }

        let mut downloaded_size = 0u64;
        let mut data = Vec::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            downloaded_size += chunk.len() as u64;

            if downloaded_size > effective_max_size {
                return Err(WeChatError::ImageUpload {
                    path: url.to_string(),
                    reason: format!(
                        "Content too large during download: {downloaded_size} bytes (max: {effective_max_size} bytes)"
                    ),
                });
            }

            data.extend_from_slice(&chunk);
        }

        debug!("Downloaded {downloaded_size} bytes from {url}");
        Ok(data)
    }
}

// Implement the HttpClient trait for WeChatHttpClient
#[async_trait::async_trait]
impl HttpClient for WeChatHttpClient {
    async fn get_with_token(&self, endpoint: &str, token: &str) -> Result<reqwest::Response> {
        self.get_with_token(endpoint, token).await
    }

    async fn post_json_with_token<T: serde::Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        token: &str,
        body: &T,
    ) -> Result<reqwest::Response> {
        self.post_json_with_token(endpoint, token, body).await
    }

    async fn upload_file(
        &self,
        endpoint: &str,
        token: &str,
        field_name: &str,
        file_data: Vec<u8>,
        filename: &str,
    ) -> Result<reqwest::Response> {
        self.upload_file(endpoint, token, field_name, file_data, filename)
            .await
    }

    async fn download_with_limit(&self, url: &str, max_size: u64) -> Result<Vec<u8>> {
        self.download_with_limit(url, max_size).await
    }
}

/// Standard WeChat API response structure.
#[derive(Debug, Deserialize, Serialize)]
pub struct WeChatResponse<T> {
    /// Error code (0 for success)
    #[serde(default)]
    pub errcode: i32,
    /// Error message
    #[serde(default)]
    pub errmsg: String,
    /// Response data (flattened)
    #[serde(flatten)]
    pub data: Option<T>,
}

impl<T: std::fmt::Debug> WeChatResponse<T> {
    /// Converts the response to a Result, checking for API errors.
    pub fn into_result(self) -> Result<T> {
        if self.errcode == 0 {
            self.data.ok_or_else(|| WeChatError::Internal {
                message: format!(
                    "Missing response data. errcode: {}, errmsg: {}",
                    self.errcode, self.errmsg
                ),
            })
        } else {
            Err(WeChatError::from_api_response(self.errcode, self.errmsg))
        }
    }
}

/// Access token response from WeChat API.
#[derive(Debug, Deserialize, Serialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub expires_in: u64,
}

/// Image upload response from WeChat API (uploadimg endpoint).
#[derive(Debug, Deserialize, Serialize)]
pub struct ImageUploadResponse {
    pub url: String,
}

/// Material upload response from WeChat API (for permanent materials like cover images).
#[derive(Debug, Deserialize, Serialize)]
pub struct MaterialUploadResponse {
    pub media_id: String,
    pub url: String,
}

/// Draft creation response from WeChat API.
#[derive(Debug, Deserialize, Serialize)]
pub struct DraftResponse {
    pub media_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_client_creation() {
        let client = WeChatHttpClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert_eq!(config.backoff_factor, 2.0);
    }

    #[test]
    fn test_wechat_response_success() {
        let response: WeChatResponse<AccessTokenResponse> = WeChatResponse {
            errcode: 0,
            errmsg: "ok".to_string(),
            data: Some(AccessTokenResponse {
                access_token: "test_token".to_string(),
                expires_in: 7200,
            }),
        };

        let result = response.into_result();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().access_token, "test_token");
    }

    #[test]
    fn test_wechat_response_error() {
        let response: WeChatResponse<AccessTokenResponse> = WeChatResponse {
            errcode: 40001,
            errmsg: "invalid credential".to_string(),
            data: None,
        };

        let result = response.into_result();
        assert!(result.is_err());

        if let Err(WeChatError::WeChatApi { code, message }) = result {
            assert_eq!(code, 40001);
            assert_eq!(message, "invalid credential");
        } else {
            panic!("Expected WeChatApi error");
        }
    }
}
