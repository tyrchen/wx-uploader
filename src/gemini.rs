//! Gemini API integration for cover image generation
//!
//! This module provides Google Gemini API integration for generating article cover
//! images using Gemini's native image generation capabilities.

use crate::error::{Error, Result};
use crate::output::{FORMATTER, FilePathFormatter};
use base64::Engine;
use google_cloud_auth::credentials::{CacheableResource, Credentials};
use reqwest::Client;
use serde_json::json;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::OnceCell;
use tracing::{info, warn};

const API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const MAX_RETRIES: u32 = 5;
const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

// Share the SDK's token cache and refresh task across articles and model aliases.
static ADC_CREDENTIALS: LazyLock<Arc<OnceCell<Credentials>>> =
    LazyLock::new(|| Arc::new(OnceCell::new()));

#[derive(Clone)]
enum GeminiAuth {
    ApiKey(String),
    Adc(Arc<OnceCell<Credentials>>),
}

impl std::fmt::Debug for GeminiAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey(_) => f.write_str("ApiKey([REDACTED])"),
            Self::Adc(_) => f.write_str("Adc"),
        }
    }
}

/// Gemini API client for generating cover images
#[derive(Clone, Debug)]
pub struct GeminiClient {
    auth: GeminiAuth,
    http_client: Client,
    model_id: &'static str,
    vertex_project: Option<String>,
    vertex_location: String,
}

impl GeminiClient {
    /// Creates a new Gemini client with the provided API key and model ID
    pub fn new(api_key: String, model_id: &'static str) -> Self {
        Self::with_auth(GeminiAuth::ApiKey(api_key), model_id)
    }

    /// Uses Vertex AI with automatically refreshed Application Default Credentials.
    /// The resource project comes from GOOGLE_CLOUD_PROJECT / GCLOUD_PROJECT,
    /// falling back to the ADC quota project. The default location is global.
    pub fn from_adc(model_id: &'static str) -> Self {
        Self::with_auth(GeminiAuth::Adc(Arc::clone(&ADC_CREDENTIALS)), model_id)
    }

    fn with_auth(auth: GeminiAuth, model_id: &'static str) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("failed to create HTTP client");

        Self {
            auth,
            http_client,
            model_id,
            vertex_project: nonempty_env("GOOGLE_CLOUD_PROJECT")
                .or_else(|| nonempty_env("GCLOUD_PROJECT")),
            vertex_location: nonempty_env("GOOGLE_CLOUD_LOCATION")
                .unwrap_or_else(|| "global".into()),
        }
    }

    fn request_url(&self, headers: &reqwest::header::HeaderMap) -> Result<String> {
        if matches!(self.auth, GeminiAuth::ApiKey(_)) {
            return Ok(format!(
                "{}/{}:generateContent",
                API_BASE_URL, self.model_id
            ));
        }

        let project = self.vertex_project.as_deref().or_else(|| {
            headers.get("x-goog-user-project").and_then(|value| value.to_str().ok())
        }).filter(|value| !value.trim().is_empty()).ok_or_else(|| {
            Error::config("Vertex AI requires a project. Set GOOGLE_CLOUD_PROJECT or run gcloud auth application-default set-quota-project PROJECT_ID.")
        })?;
        let location = &self.vertex_location;
        if !location
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || location.is_empty()
        {
            return Err(Error::config("Invalid GOOGLE_CLOUD_LOCATION"));
        }
        if !project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '.' | '_'))
        {
            return Err(Error::config("Invalid Vertex AI project ID"));
        }
        let host = if location == "global" {
            "aiplatform.googleapis.com".to_owned()
        } else {
            format!("{location}-aiplatform.googleapis.com")
        };
        Ok(format!(
            "https://{host}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
            self.model_id
        ))
    }

    async fn auth_headers(&self) -> Result<reqwest::header::HeaderMap> {
        match &self.auth {
            GeminiAuth::ApiKey(api_key) => {
                let mut value = reqwest::header::HeaderValue::from_str(api_key)
                    .map_err(|_| Error::gemini("Invalid GEMINI_API_KEY header value"))?;
                value.set_sensitive(true);
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert("x-goog-api-key", value);
                Ok(headers)
            }
            GeminiAuth::Adc(cache) => {
                let credentials = cache
                    .get_or_try_init(|| async {
                        google_cloud_auth::credentials::Builder::default()
                            .with_scopes(["https://www.googleapis.com/auth/cloud-platform"])
                            .build()
                            .map_err(|e| Error::gemini(format!(
                                "Failed to load ADC: {e}. Run gcloud auth application-default login or set GOOGLE_APPLICATION_CREDENTIALS."
                            )))
                    })
                    .await?;

                // Ask the SDK on every attempt so it can refresh expired tokens.
                // Keep all headers, including x-goog-user-project for quota billing.
                match credentials.headers(http::Extensions::new()).await.map_err(|e| {
                    Error::gemini(format!(
                        "Failed to obtain ADC access token: {e}. Check ADC login and quota project configuration."
                    ))
                })? {
                    CacheableResource::New { mut data, .. } => {
                        if let Some(value) = data.get_mut(reqwest::header::AUTHORIZATION) {
                            value.set_sensitive(true);
                        }
                        Ok(data)
                    }
                    CacheableResource::NotModified => Err(Error::gemini(
                        "ADC returned no authentication headers without a cache tag",
                    )),
                }
            }
        }
    }

    /// Generates a cover image from a complete prompt and saves it to the target path
    async fn generate_cover(&self, prompt: &str, target_path: &Path) -> Result<()> {
        info!(
            "Generating cover image with {} model: {}",
            if matches!(self.auth, GeminiAuth::Adc(_)) {
                "Vertex AI"
            } else {
                "Gemini Developer API"
            },
            self.model_id
        );
        println!(
            "{}",
            FORMATTER.format_image_prompt(&format!("Gemini ({}): 微信封面生成", self.model_id))
        );

        let image_data = self.generate_image(prompt).await?;

        // Ensure parent directory exists
        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let image_data = crate::image_utils::encode_for_path(image_data, target_path)?;
        tokio::fs::write(target_path, &image_data).await?;
        println!("{}", FORMATTER.format_image_saved(target_path));

        Ok(())
    }

    /// Generates from a complete image prompt, saving with an auto-generated filename
    pub async fn generate_cover_image(
        &self,
        prompt: &str,
        file_path: &Path,
        base_filename: &str,
    ) -> Result<String> {
        let cover_filename = format!(
            "{}_cover_{}.png",
            base_filename,
            uuid::Uuid::new_v4().simple()
        );
        let cover_path = file_path
            .parent()
            .ok_or_else(|| Error::generic("Failed to get parent directory"))?
            .join(&cover_filename);

        self.generate_cover(prompt, &cover_path).await?;

        Ok(cover_filename)
    }

    /// Generates from a complete image prompt to a specific path
    pub async fn generate_cover_image_to_path(
        &self,
        prompt: &str,
        _markdown_file_path: &Path,
        target_cover_path: &Path,
    ) -> Result<()> {
        println!("{}", FORMATTER.format_target_path(target_cover_path));
        self.generate_cover(prompt, target_cover_path).await
    }

    /// Sends generateContent request and extracts image bytes, with retry
    async fn generate_image(&self, prompt: &str) -> Result<Vec<u8>> {
        for attempt in 1..=2u32 {
            let response = self.generate_content(prompt).await?;

            if let Some(image_data) = extract_image_data(&response) {
                return Ok(image_data);
            }

            if attempt < 2 {
                warn!("Gemini returned text-only response, retrying...");
                continue;
            }
        }

        Err(Error::gemini("No image in response after retry"))
    }

    /// Sends a generateContent request with retry/backoff on transient errors
    async fn generate_content(&self, prompt: &str) -> Result<serde_json::Value> {
        self.generate_content_at(None, prompt).await
    }

    async fn generate_content_at(
        &self,
        endpoint: Option<&str>,
        prompt: &str,
    ) -> Result<serde_json::Value> {
        let request_body = json!({
            "contents": [{
                "parts": [{"text": prompt}],
                "role": "user"
            }],
            "generationConfig": {
                "responseModalities": ["TEXT", "IMAGE"],
                "imageConfig": {
                    "aspectRatio": "21:9",
                    "imageSize": "1K"
                }
            }
        });

        let mut backoff = INITIAL_BACKOFF;

        for attempt in 1..=MAX_RETRIES {
            let headers = self.auth_headers().await?;
            let url = match endpoint {
                Some(url) => url.to_owned(),
                None => self.request_url(&headers)?,
            };
            info!("Gemini request endpoint: {}", url);
            let resp = self
                .http_client
                .post(&url)
                .headers(headers)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| Error::gemini(format!("HTTP request failed: {}", e)))?;

            let status = resp.status();

            if status.is_success() {
                let body: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| Error::gemini(format!("Failed to parse response: {}", e)))?;

                // Check for API-level error in response body
                if let Some(err) = body.get("error") {
                    let message = err["message"].as_str().unwrap_or("Unknown error");
                    return Err(Error::gemini(format!("API error: {}", message)));
                }

                return Ok(body);
            }

            let status_code = status.as_u16();

            // Parse Retry-After header before consuming body
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs);

            let body = resp.text().await.unwrap_or_default();

            // Non-retryable errors
            if status_code == 400 || status_code == 403 {
                return Err(Error::gemini(format!(
                    "API request failed ({}): {}",
                    status_code, body
                )));
            }

            // Retryable errors: 429, 5xx
            if (status_code == 429 || status_code >= 500) && attempt < MAX_RETRIES {
                let wait = retry_after.unwrap_or(backoff);
                warn!(
                    status = status_code,
                    attempt,
                    wait_secs = wait.as_secs(),
                    "Gemini retryable error, backing off"
                );
                tokio::time::sleep(wait).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }

            return Err(Error::gemini(format!(
                "API request failed ({}): {}",
                status_code, body
            )));
        }

        Err(Error::gemini("Rate limit exhausted after max retries"))
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Extract the first image from a Gemini generateContent response
fn extract_image_data(response: &serde_json::Value) -> Option<Vec<u8>> {
    let candidates = response.get("candidates")?.as_array()?;

    for candidate in candidates {
        let parts = candidate.get("content")?.get("parts")?.as_array()?;

        for part in parts {
            if let Some(inline_data) = part.get("inlineData") {
                let data_str = inline_data.get("data")?.as_str()?;
                match base64::engine::general_purpose::STANDARD.decode(data_str) {
                    Ok(bytes) => return Some(bytes),
                    Err(e) => {
                        warn!("Failed to decode base64 image data: {}", e);
                        continue;
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_cloud_auth::credentials::{CredentialsProvider, EntityTag};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_gemini_client_creation() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-nano-banana-2.1");
        assert!(matches!(&client.auth, GeminiAuth::ApiKey(key) if key == "test-key"));
        assert!(!format!("{client:?}").contains("test-key"));
        assert_eq!(client.model_id, "gemini-nano-banana-2.1");
    }

    #[test]
    fn test_adc_cache_is_shared_across_models() {
        let first = GeminiClient::from_adc("gemini-nano-banana-2.1");
        let second = GeminiClient::from_adc("gemini-3-pro-image-preview");
        let (GeminiAuth::Adc(first), GeminiAuth::Adc(second)) = (&first.auth, &second.auth) else {
            panic!("expected ADC authentication");
        };
        assert!(Arc::ptr_eq(first, second));
    }

    #[test]
    fn test_adc_routes_to_vertex_using_quota_project() {
        let mut client = GeminiClient::from_adc("gemini-nano-banana-2.1");
        client.vertex_project = None;
        client.vertex_location = "global".into();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-goog-user-project", "credit-project".parse().unwrap());
        assert_eq!(
            client.request_url(&headers).unwrap(),
            "https://aiplatform.googleapis.com/v1/projects/credit-project/locations/global/publishers/google/models/gemini-nano-banana-2.1:generateContent"
        );

        client.vertex_project = Some("resource-project".into());
        client.vertex_location = "us-central1".into();
        assert_eq!(
            client.request_url(&headers).unwrap(),
            "https://us-central1-aiplatform.googleapis.com/v1/projects/resource-project/locations/us-central1/publishers/google/models/gemini-nano-banana-2.1:generateContent"
        );
    }

    #[test]
    fn test_vertex_requires_project_and_valid_location() {
        let mut client = GeminiClient::from_adc("gemini-nano-banana-2.1");
        client.vertex_project = None;
        let headers = reqwest::header::HeaderMap::new();
        let error = client.request_url(&headers).unwrap_err().to_string();
        assert!(error.contains("GOOGLE_CLOUD_PROJECT"));
        client.vertex_project = Some("test-project".into());
        client.vertex_location = "invalid/path".into();
        assert!(client.request_url(&headers).is_err());
    }

    #[test]
    fn test_explicit_api_key_routes_to_developer_api() {
        let mut client = GeminiClient::new("test-key".into(), "gemini-nano-banana-2.1");
        client.vertex_project = Some("unused-project".into());
        assert_eq!(
            client
                .request_url(&reqwest::header::HeaderMap::new())
                .unwrap(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-nano-banana-2.1:generateContent"
        );
    }

    #[tokio::test]
    async fn test_api_key_headers() {
        let client = GeminiClient::new("test-key".into(), "gemini-nano-banana-2.1");
        let headers = client.auth_headers().await.unwrap();
        assert_eq!(headers["x-goog-api-key"], "test-key");
        assert!(headers["x-goog-api-key"].is_sensitive());
        assert!(!headers.contains_key("authorization"));
        assert!(!headers.contains_key("x-goog-user-project"));

        let invalid = GeminiClient::new("invalid\nkey".into(), "gemini-nano-banana-2.1");
        assert!(invalid.auth_headers().await.is_err());
    }

    #[derive(Debug)]
    struct RotatingCredentials(Arc<AtomicUsize>);

    impl CredentialsProvider for RotatingCredentials {
        async fn headers(
            &self,
            _extensions: http::Extensions,
        ) -> std::result::Result<
            CacheableResource<http::HeaderMap>,
            google_cloud_auth::errors::CredentialsError,
        > {
            let generation = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            let mut data = http::HeaderMap::new();
            data.insert(
                "authorization",
                format!("Bearer short-lived-{generation}").parse().unwrap(),
            );
            data.insert("x-goog-user-project", "test-project".parse().unwrap());
            Ok(CacheableResource::New {
                entity_tag: EntityTag::new(),
                data,
            })
        }

        async fn universe_domain(&self) -> Option<String> {
            None
        }
    }

    #[tokio::test]
    async fn test_retry_uses_updated_adc_token_and_quota_project() {
        // Simulate the SDK refreshing a token between requests, without real ADC.
        let calls = Arc::new(AtomicUsize::new(0));
        let credentials = Credentials::from(RotatingCredentials(Arc::clone(&calls)));
        let client = GeminiClient::with_auth(
            GeminiAuth::Adc(Arc::new(OnceCell::new_with(Some(credentials)))),
            "gemini-nano-banana-2.1",
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/v1/projects/test-project/locations/global/publishers/google/models/gemini-nano-banana-2.1:generateContent",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move {
            for generation in 1..=2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let count = stream.read(&mut buf).await.unwrap();
                    assert!(count > 0, "connection closed before request was complete");
                    request.extend_from_slice(&buf[..count]);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .unwrap()
                            .parse()
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            assert!(headers.starts_with(
                                "post /v1/projects/test-project/locations/global/publishers/google/models/gemini-nano-banana-2.1:generatecontent "
                            ));
                            assert!(headers.contains(&format!(
                                "authorization: bearer short-lived-{generation}\r\n"
                            )));
                            assert!(headers.contains("x-goog-user-project: test-project"));
                            assert!(!headers.contains("x-goog-api-key"));
                            let body: serde_json::Value =
                                serde_json::from_slice(&request[end + 4..]).unwrap();
                            assert_eq!(body["contents"][0]["parts"][0]["text"], "test prompt");
                            assert_eq!(body["generationConfig"]["imageConfig"]["imageSize"], "1K");
                            break;
                        }
                    }
                }
                let response = if generation == 1 {
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"candidates\":[]}"
                };
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.unwrap();
            }
        });

        let response = tokio::time::timeout(
            Duration::from_secs(5),
            client.generate_content_at(Some(&url), "test prompt"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response, json!({"candidates": []}));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        server.await.unwrap();
    }

    #[test]
    fn test_extract_image_data_with_image() {
        let response = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Here is the image"},
                        {
                            "inlineData": {
                                "mimeType": "image/png",
                                "data": base64::engine::general_purpose::STANDARD.encode(b"fake-png-data")
                            }
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        });

        let data = extract_image_data(&response);
        assert!(data.is_some());
        assert_eq!(data.unwrap(), b"fake-png-data");
    }

    #[test]
    fn test_extract_image_data_text_only() {
        let response = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "I cannot generate that image"}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        });

        let data = extract_image_data(&response);
        assert!(data.is_none());
    }

    #[test]
    fn test_extract_image_data_empty_candidates() {
        let response = serde_json::json!({
            "candidates": []
        });

        let data = extract_image_data(&response);
        assert!(data.is_none());
    }
}
