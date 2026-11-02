//! Trait abstractions for the WeChat Official Account SDK.
//!
//! This module defines core traits that provide abstractions for:
//! - Token management with automatic refresh
//! - Content uploading with retry logic
//! - Image processing and management
//! - Content rendering and theming
//! - Caching strategies
//!
//! These traits enable better testability, modularity, and extensibility.

use crate::error::Result;
use crate::upload::{Article, DraftInfo};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Trait for managing WeChat access tokens with automatic refresh capabilities.
#[async_trait]
pub trait TokenProvider: Send + Sync {
    /// Gets a valid access token, refreshing if necessary.
    async fn get_token(&self) -> Result<String>;

    /// Forces a token refresh.
    async fn refresh_token(&self) -> Result<String>;

    /// Checks if the current token is expired.
    async fn is_token_expired(&self) -> bool;

    /// Gets token expiration time.
    async fn token_expires_at(&self) -> Option<DateTime<Utc>>;
}

/// Trait for uploading content to WeChat.
#[async_trait]
pub trait ContentUploader: Send + Sync {
    /// Uploads an image and returns its URL.
    async fn upload_image(&self, image_path: &str) -> Result<String>;

    /// Uploads an article and returns the draft ID.
    async fn upload_article(&self, article: &Article) -> Result<String>;

    /// Updates an existing draft.
    async fn update_draft(&self, draft_id: &str, article: &Article) -> Result<()>;

    /// Gets draft information by ID.
    async fn get_draft(&self, draft_id: &str) -> Result<DraftInfo>;

    /// Lists available drafts with pagination.
    async fn list_drafts(&self, offset: u32, count: u32) -> Result<Vec<DraftInfo>>;
}

/// Trait for image processing and management.
#[async_trait]
pub trait ImageProcessor: Send + Sync {
    /// Validates an image file.
    async fn validate_image(&self, path: &str) -> Result<()>;

    /// Gets image metadata (size, format, etc.).
    async fn get_image_info(&self, path: &str) -> Result<ImageInfo>;

    /// Resizes an image if needed.
    async fn resize_if_needed(
        &self,
        path: &str,
        max_width: u32,
        max_height: u32,
    ) -> Result<Vec<u8>>;

    /// Compresses an image to reduce file size.
    async fn compress_image(&self, data: &[u8], quality: u8) -> Result<Vec<u8>>;
}

/// Trait for rendering content with themes.
pub trait ContentRenderer: Send + Sync {
    /// Renders markdown content to HTML with the specified theme.
    fn render_content(
        &self,
        markdown: &str,
        theme: &str,
        code_theme: &str,
        metadata: &HashMap<String, String>,
    ) -> Result<String>;

    /// Gets available themes.
    fn available_themes(&self) -> Vec<String>;

    /// Checks if a theme exists.
    fn has_theme(&self, theme: &str) -> bool;

    /// Validates theme configuration.
    fn validate_theme(&self, theme: &str) -> Result<()>;
}

/// Trait for caching strategies.
#[async_trait]
pub trait Cache<K, V>: Send + Sync
where
    K: Send + Sync,
    V: Send + Sync + Clone,
{
    /// Gets a value from the cache.
    async fn get(&self, key: &K) -> Option<V>;

    /// Sets a value in the cache.
    async fn set(&self, key: K, value: V);

    /// Removes a value from the cache.
    async fn remove(&self, key: &K);

    /// Clears all cached values.
    async fn clear(&self);

    /// Gets cache statistics.
    async fn stats(&self) -> CacheStats;
}

/// Trait for HTTP client operations.
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Makes a GET request with token.
    async fn get_with_token(&self, endpoint: &str, token: &str) -> Result<reqwest::Response>;

    /// Makes a POST request with JSON body and token.
    async fn post_json_with_token<T: serde::Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        token: &str,
        body: &T,
    ) -> Result<reqwest::Response>;

    /// Uploads a file using multipart form data.
    async fn upload_file(
        &self,
        endpoint: &str,
        token: &str,
        field_name: &str,
        file_data: Vec<u8>,
        filename: &str,
    ) -> Result<reqwest::Response>;

    /// Downloads content from a URL with size limits.
    async fn download_with_limit(&self, url: &str, max_size: u64) -> Result<Vec<u8>>;
}

/// Trait for parsing and processing markdown content.
pub trait MarkdownProcessor: Send + Sync {
    /// Parses markdown content and extracts metadata.
    fn parse_content(&self, content: &str) -> Result<ParsedMarkdown>;

    /// Extracts image references from markdown.
    fn extract_images(&self, content: &str) -> Result<Vec<ImageReference>>;

    /// Replaces image URLs in markdown content.
    fn replace_image_urls(&self, content: &str, url_map: &HashMap<String, String>) -> String;

    /// Validates markdown structure.
    fn validate_markdown(&self, content: &str) -> Result<()>;
}

/// Image information structure.
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: u64,
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub hit_rate: f64,
}

/// Parsed markdown content with metadata.
#[derive(Debug, Clone)]
pub struct ParsedMarkdown {
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub cover: Option<String>,
    pub theme: Option<String>,
    pub code_theme: Option<String>,
}

/// Image reference in markdown.
#[derive(Debug, Clone)]
pub struct ImageReference {
    pub alt_text: String,
    pub url: String,
    pub title: Option<String>,
}

/// Default implementations and utilities
impl CacheStats {
    /// Creates new cache statistics.
    pub fn new(hits: u64, misses: u64, entries: usize) -> Self {
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        Self {
            hits,
            misses,
            entries,
            hit_rate,
        }
    }

    /// Updates hit count.
    pub fn record_hit(&mut self) {
        self.hits += 1;
        self.update_hit_rate();
    }

    /// Updates miss count.
    pub fn record_miss(&mut self) {
        self.misses += 1;
        self.update_hit_rate();
    }

    fn update_hit_rate(&mut self) {
        let total = self.hits + self.misses;
        self.hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
    }
}

impl Default for CacheStats {
    fn default() -> Self {
        Self::new(0, 0, 0)
    }
}

impl ParsedMarkdown {
    /// Creates a new ParsedMarkdown instance.
    pub fn new(content: String, metadata: HashMap<String, String>) -> Self {
        let title = metadata.get("title").cloned();
        let author = metadata.get("author").cloned();
        let cover = metadata.get("cover").cloned();
        let theme = metadata.get("theme").cloned();
        let code_theme = metadata.get("code").cloned();

        Self {
            content,
            metadata,
            title,
            author,
            cover,
            theme,
            code_theme,
        }
    }

    /// Gets a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Checks if required fields are present.
    pub fn has_required_fields(&self) -> bool {
        self.title.is_some() && self.cover.is_some()
    }
}

impl ImageReference {
    /// Creates a new image reference.
    pub fn new(alt_text: String, url: String, title: Option<String>) -> Self {
        Self {
            alt_text,
            url,
            title,
        }
    }

    /// Checks if this is a local image (not a URL).
    pub fn is_local(&self) -> bool {
        !self.url.starts_with("http://") && !self.url.starts_with("https://")
    }

    /// Gets the file extension of the image.
    pub fn extension(&self) -> Option<&str> {
        std::path::Path::new(&self.url)
            .extension()
            .and_then(|ext| ext.to_str())
    }
}

// Utility trait for types that can be validated
pub trait Validate {
    type Error;

    /// Validates the instance.
    fn validate(&self) -> std::result::Result<(), Self::Error>;
}

// Utility trait for types that can be configured
pub trait Configurable {
    type Config;

    /// Applies configuration to the instance.
    fn configure(&mut self, config: &Self::Config) -> Result<()>;

    /// Gets the current configuration.
    fn get_config(&self) -> &Self::Config;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats() {
        let mut stats = CacheStats::default();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate, 0.0);

        stats.record_hit();
        stats.record_hit();
        stats.record_miss();

        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 2.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn test_parsed_markdown() {
        let mut metadata = HashMap::new();
        metadata.insert("title".to_string(), "Test Title".to_string());
        metadata.insert("author".to_string(), "Test Author".to_string());
        metadata.insert("cover".to_string(), "cover.jpg".to_string());

        let parsed = ParsedMarkdown::new("# Content".to_string(), metadata);

        assert_eq!(parsed.title, Some("Test Title".to_string()));
        assert_eq!(parsed.author, Some("Test Author".to_string()));
        assert_eq!(parsed.cover, Some("cover.jpg".to_string()));
        assert!(parsed.has_required_fields());
        assert_eq!(
            parsed.get_metadata("title"),
            Some(&"Test Title".to_string())
        );
    }

    #[test]
    fn test_image_reference() {
        let img_ref = ImageReference::new(
            "Alt text".to_string(),
            "images/test.jpg".to_string(),
            Some("Title".to_string()),
        );

        assert!(img_ref.is_local());
        assert_eq!(img_ref.extension(), Some("jpg"));

        let url_ref = ImageReference::new(
            "Alt text".to_string(),
            "https://example.com/image.png".to_string(),
            None,
        );

        assert!(!url_ref.is_local());
        assert_eq!(url_ref.extension(), Some("png"));
    }
}
