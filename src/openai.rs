//! OpenAI integration for cover image generation
//!
//! Single-stage flow: article content plus the shared style directives go
//! straight to gpt-image-2. No intermediate text model.

use crate::error::{Error, Result};
use crate::image_prompt::build_cover_prompt;
use crate::output::{ApiErrorFormatter, FORMATTER, FilePathFormatter, OutputFormatter};
use base64::Engine;
use reqwest::Client;
use serde_json::{Value, json};
use std::path::Path;
use tracing::info;

/// Trait for generating images from text descriptions
// async_trait adds #[must_use] to methods returning already-must-use boxed futures.
#[allow(clippy::double_must_use)]
#[async_trait::async_trait]
pub trait ImageGenerator {
    /// Generates an image from a text prompt and returns either a URL or
    /// a `base64:`-prefixed marker holding the raw image bytes.
    async fn generate_image(&self, prompt: &str) -> Result<String>;

    /// Downloads (or decodes) the image and saves it to the specified path
    async fn download_image(&self, url: &str, file_path: &Path) -> Result<()>;
}

/// OpenAI API client for generating cover images
#[derive(Clone, Debug)]
pub struct OpenAIClient {
    api_key: String,
    http_client: Client,
    base_url: String,
}

impl OpenAIClient {
    /// Creates a new OpenAI client with the provided API key
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http_client: Client::new(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Creates a new OpenAI client with a custom base URL (useful for testing)
    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        Self {
            api_key,
            http_client: Client::new(),
            base_url,
        }
    }

    /// Creates a new OpenAI client with a custom HTTP client
    pub fn with_client(api_key: String, http_client: Client) -> Self {
        Self {
            api_key,
            http_client,
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// Generates and saves a cover image with an auto-generated filename.
    ///
    /// The image is saved next to `file_path` and the resulting filename is
    /// returned so the caller can write it back into frontmatter.
    pub async fn generate_cover_image(
        &self,
        content: &str,
        file_path: &Path,
        base_filename: &str,
    ) -> Result<String> {
        let prompt = build_cover_prompt(content);
        info!("Generating cover image with OpenAI gpt-image-2");
        println!(
            "{}",
            FORMATTER.format_image_prompt("OpenAI (gpt-image-2): 微信封面生成")
        );

        let image_url = match self.generate_image(&prompt).await {
            Ok(url) => url,
            Err(e) => {
                FORMATTER.print_error(&FORMATTER.format_image_generation_failure(&e.to_string()));
                return Err(e);
            }
        };

        let cover_filename = format!(
            "{}_cover_{}.png",
            base_filename,
            uuid::Uuid::new_v4().simple()
        );
        let cover_path = file_path
            .parent()
            .ok_or_else(|| Error::generic("Failed to get parent directory"))?
            .join(&cover_filename);

        self.download_image(&image_url, &cover_path).await?;
        Ok(cover_filename)
    }

    /// Generates and saves a cover image to a specific path
    pub async fn generate_cover_image_to_path(
        &self,
        content: &str,
        _markdown_file_path: &Path,
        target_cover_path: &Path,
    ) -> Result<()> {
        println!("{}", FORMATTER.format_target_path(target_cover_path));

        let prompt = build_cover_prompt(content);
        info!("Generating cover image with OpenAI gpt-image-2");
        println!(
            "{}",
            FORMATTER.format_image_prompt("OpenAI (gpt-image-2): 微信封面生成")
        );

        let image_url = match self.generate_image(&prompt).await {
            Ok(url) => url,
            Err(e) => {
                FORMATTER.print_error(&FORMATTER.format_image_generation_failure(&e.to_string()));
                return Err(e);
            }
        };

        match self.download_image(&image_url, target_cover_path).await {
            Ok(()) => {
                println!("{}", FORMATTER.format_image_saved(target_cover_path));
                Ok(())
            }
            Err(e) => {
                FORMATTER.print_error(&FORMATTER.format_image_download_failure(&e.to_string()));
                Err(e)
            }
        }
    }

    /// Makes a POST request to the OpenAI API
    async fn post_request(&self, endpoint: &str, body: Value) -> Result<Value> {
        let url = format!("{}/{}", self.base_url, endpoint);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            eprintln!(
                "{}",
                FORMATTER.format_openai_error(
                    status.as_u16(),
                    &error_text,
                    &format!("{}/{}", self.base_url, endpoint)
                )
            );
            return Err(Error::openai(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        let response_json: Value = response.json().await?;
        Ok(response_json)
    }
}

#[async_trait::async_trait]
impl ImageGenerator for OpenAIClient {
    async fn generate_image(&self, prompt: &str) -> Result<String> {
        let request_body = json!({
            "model": "gpt-image-2",
            "prompt": prompt,
            // Near WeChat's 2.35:1 ratio, within GPT Image 2's 16-pixel / minimum-area limits.
            "size": "1280x544",
            "quality": "medium",
            "output_format": "png",
            "n": 1
        });

        let response_json = self
            .post_request("images/generations", request_body)
            .await?;

        let base64_data = if let Some(b64) = response_json["data"][0]["b64_json"].as_str() {
            b64.to_string()
        } else if let Some(url) = response_json["data"][0]["url"].as_str() {
            return Ok(url.to_string());
        } else {
            return Err(Error::openai(format!(
                "Failed to extract image data from response. Keys available: {:?}",
                response_json
                    .as_object()
                    .map(|o| o.keys().collect::<Vec<_>>())
            )));
        };

        Ok(format!("base64:{}", base64_data))
    }

    async fn download_image(&self, url: &str, file_path: &Path) -> Result<()> {
        let image_bytes = if let Some(base64_str) = url.strip_prefix("base64:") {
            base64::engine::general_purpose::STANDARD
                .decode(base64_str)
                .map_err(|e| Error::openai(format!("Failed to decode base64 image: {}", e)))?
        } else {
            let response = self.http_client.get(url).send().await?;

            if !response.status().is_success() {
                return Err(Error::openai(format!(
                    "Failed to download image: HTTP {}",
                    response.status()
                )));
            }

            let bytes = response.bytes().await?;
            bytes.to_vec()
        };

        // Generated covers are PNG; explicit .jpg/.webp cover paths must match their bytes.
        let image_bytes = match image::ImageFormat::from_path(file_path) {
            Ok(format) if image::guess_format(&image_bytes).ok() != Some(format) => {
                let image = image::load_from_memory(&image_bytes)
                    .map_err(|e| Error::openai(format!("Failed to decode generated image: {e}")))?;
                let mut output = std::io::Cursor::new(Vec::new());
                if format == image::ImageFormat::Jpeg {
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 95)
                        .encode_image(&image)
                        .map_err(|e| Error::openai(format!("Failed to encode cover image: {e}")))?;
                } else {
                    image
                        .write_to(&mut output, format)
                        .map_err(|e| Error::openai(format!("Failed to encode cover image: {e}")))?;
                }
                output.into_inner()
            }
            _ => image_bytes,
        };

        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(file_path, image_bytes).await?;
        Ok(())
    }
}

/// Builder for creating OpenAI clients with different configurations
pub struct OpenAIClientBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    http_client: Option<Client>,
}

impl OpenAIClientBuilder {
    pub fn new() -> Self {
        Self {
            api_key: None,
            base_url: None,
            http_client: None,
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = Some(base_url);
        self
    }

    pub fn with_http_client(mut self, client: Client) -> Self {
        self.http_client = Some(client);
        self
    }

    pub fn build(self) -> Result<OpenAIClient> {
        let api_key = self
            .api_key
            .ok_or_else(|| Error::config("OpenAI API key is required"))?;

        let client = match (self.base_url, self.http_client) {
            (Some(base_url), Some(http_client)) => OpenAIClient {
                api_key,
                http_client,
                base_url,
            },
            (Some(base_url), None) => OpenAIClient::with_base_url(api_key, base_url),
            (None, Some(http_client)) => OpenAIClient::with_client(api_key, http_client),
            (None, None) => OpenAIClient::new(api_key),
        };

        Ok(client)
    }
}

impl Default for OpenAIClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_openai_client_creation() {
        let client = OpenAIClient::new("test-key".to_string());
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_openai_client_with_base_url() {
        let client = OpenAIClient::with_base_url(
            "test-key".to_string(),
            "https://custom.api.com".to_string(),
        );
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_openai_client_builder() {
        let client = OpenAIClientBuilder::new()
            .with_api_key("test-key".to_string())
            .with_base_url("https://custom.api.com".to_string())
            .build()
            .unwrap();

        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_openai_client_builder_missing_api_key() {
        let result = OpenAIClientBuilder::new()
            .with_base_url("https://custom.api.com".to_string())
            .build();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("OpenAI API key is required")
        );
    }

    #[tokio::test]
    async fn test_generate_cover_sends_article_context_and_saves_png() {
        use crate::image_prompt::build_cover_context;
        use image::{DynamicImage, ImageFormat};
        use std::{io::Cursor, time::Duration};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut png = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(1280, 544)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let image_data = png.into_inner();
        let response = json!({"data": [{
            "b64_json": base64::engine::general_purpose::STANDARD.encode(&image_data)
        }]})
        .to_string();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let request_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            let (body_start, body_len) = loop {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0, "incomplete HTTP request");
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    assert!(headers.starts_with("POST /v1/images/generations "));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while request.len() < body_start + body_len {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0, "incomplete HTTP body");
                request.extend_from_slice(&buffer[..count]);
            }
            let payload: Value =
                serde_json::from_slice(&request[body_start..body_start + body_len]).unwrap();
            let http_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(http_response.as_bytes()).await.unwrap();
            payload
        });

        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir
            .path()
            .join("nested")
            .join("directory")
            .join("image.png");

        let client = OpenAIClient::with_base_url("test-key".into(), format!("http://{address}/v1"));
        let context = build_cover_context(
            Some("Rust"),
            "内存安全",
            "## 核心机制\n多个读者或一个写者。",
        );
        tokio::time::timeout(
            Duration::from_secs(5),
            client.generate_cover_image_to_path(&context, Path::new("article.md"), &nested_path),
        )
        .await
        .unwrap()
        .unwrap();
        let payload = request_task.await.unwrap();
        assert_eq!(payload["model"], "gpt-image-2");
        assert_eq!(payload["quality"], "medium");
        assert_eq!(payload["size"], "1280x544");
        assert_eq!(payload["output_format"], "png");
        assert!(payload.get("output_compression").is_none());
        assert!(
            payload["prompt"]
                .as_str()
                .unwrap()
                .contains("多个读者或一个写者")
        );
        let saved = tokio::fs::read(&nested_path).await.unwrap();
        assert_eq!(saved, image_data);
        assert_eq!(image::guess_format(&saved).unwrap(), ImageFormat::Png);
        assert_eq!(image::image_dimensions(&nested_path).unwrap(), (1280, 544));
    }

    #[test]
    fn test_error_messages_are_descriptive() {
        let error = Error::openai("Rate limit exceeded");
        assert!(error.to_string().contains("OpenAI API error"));
        assert!(error.to_string().contains("Rate limit exceeded"));

        let error = Error::config("Invalid configuration");
        assert!(error.to_string().contains("Configuration error"));
        assert!(error.to_string().contains("Invalid configuration"));
    }

    #[tokio::test]
    async fn test_png_response_is_encoded_for_explicit_jpeg_cover_path() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(32, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let source = format!(
            "base64:{}",
            base64::engine::general_purpose::STANDARD.encode(png.into_inner())
        );
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cover.jpg");
        OpenAIClient::new("test-key".into())
            .download_image(&source, &path)
            .await
            .unwrap();
        let bytes = tokio::fs::read(&path).await.unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert_eq!(image::image_dimensions(path).unwrap(), (32, 16));
    }
}
