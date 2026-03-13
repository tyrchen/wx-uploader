//! Gemini API integration for cover image generation
//!
//! This module provides Google Gemini API integration for generating article cover
//! images using Gemini's native image generation capabilities.

use crate::error::{Error, Result};
use crate::output::{FORMATTER, FilePathFormatter};
use base64::Engine;
use reqwest::Client;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn};

const API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const MAX_RETRIES: u32 = 5;
const INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Gemini API client for generating cover images
#[derive(Clone, Debug)]
pub struct GeminiClient {
    api_key: String,
    http_client: Client,
    model_id: &'static str,
}

impl GeminiClient {
    /// Creates a new Gemini client with the provided API key and model ID
    pub fn new(api_key: String, model_id: &'static str) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("failed to create HTTP client");

        Self {
            api_key,
            http_client,
            model_id,
        }
    }

    /// Generates a cover image from article content and saves it to the target path
    async fn generate_cover(&self, content: &str, target_path: &Path) -> Result<()> {
        let truncated = if content.len() > 2000 {
            // Find a safe char boundary for truncation
            let mut end = 2000;
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            &content[..end]
        } else {
            content
        };

        let prompt = format!(
            "为微信公众号文章生成一张宽幅封面图（16:9比例）。\n\n\
             请仔细阅读以下文章信息，然后创作一幅能直观传达文章核心思想的视觉隐喻图像。\
             读者仅凭图片就应该能感受到文章的主题。\n\n\
             {}\n\n\
             重要要求：\n\
             - 图像必须体现上述文章的具体概念，不要生成通用的科技素材图\n\
             - 寻找一个有创意的视觉隐喻或象征场景来表达文章独特的主题\n\
             - 画面要有故事感和情感张力，让人产生好奇和共鸣\n\n\
             风格要求：高端数字艺术，优雅精致。\
             色调以电光蓝、暖琥珀、深紫、亮青色为主，深色背景。\
             运用体积光、发光粒子、电影级景深、充足的留白。\
             作为缩略图必须醒目：高对比度、主体突出、禁止出现任何文字或水印。",
            truncated
        );

        info!(
            "Generating cover image with Gemini model: {}",
            self.model_id
        );
        println!(
            "{}",
            FORMATTER.format_image_prompt(&format!("Gemini ({}): 微信封面生成", self.model_id))
        );

        let image_data = self.generate_image(&prompt).await?;

        // Ensure parent directory exists
        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(target_path, &image_data).await?;
        println!("{}", FORMATTER.format_image_saved(target_path));

        Ok(())
    }

    /// Generates a cover image and saves with auto-generated filename
    pub async fn generate_cover_image(
        &self,
        content: &str,
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

        self.generate_cover(content, &cover_path).await?;

        Ok(cover_filename)
    }

    /// Generates a cover image to a specific path
    pub async fn generate_cover_image_to_path(
        &self,
        content: &str,
        _markdown_file_path: &Path,
        target_cover_path: &Path,
    ) -> Result<()> {
        println!("{}", FORMATTER.format_target_path(target_cover_path));
        self.generate_cover(content, target_cover_path).await
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
        let url = format!("{}/{}:generateContent", API_BASE_URL, self.model_id);

        let request_body = json!({
            "contents": [{
                "parts": [{"text": prompt}],
                "role": "user"
            }],
            "generationConfig": {
                "responseModalities": ["TEXT", "IMAGE"],
                "imageConfig": {
                    "aspectRatio": "16:9",
                    "imageSize": "2K"
                }
            }
        });

        let mut backoff = INITIAL_BACKOFF;

        for attempt in 1..=MAX_RETRIES {
            let resp = self
                .http_client
                .post(&url)
                .header("x-goog-api-key", &self.api_key)
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

    #[test]
    fn test_gemini_client_creation() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-3.1-flash-image-preview");
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model_id, "gemini-3.1-flash-image-preview");
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
