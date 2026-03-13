//! WeChat Public Account Markdown Uploader Library
//!
//! A library for uploading markdown files to WeChat public accounts with automatic
//! cover image generation and frontmatter management.
//!
//! ## Features
//!
//! - Upload individual markdown files or process directories recursively
//! - Parse and manage YAML frontmatter to track publication status
//! - Automatically generate cover images using Gemini (default) or OpenAI
//! - Skip already published files in directory processing mode
//! - Support for custom themes and code highlighters via frontmatter
//!
//! ## Usage
//!
//! ```rust,no_run
//! use wx_uploader::{WxUploader, Config, Result};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = Config::from_env()?;
//!     let uploader = WxUploader::new(config).await?;
//!
//!     // Upload a single file
//!     uploader.upload_file("article.md", true).await?;
//!
//!     // Process a directory
//!     uploader.process_directory("./articles").await?;
//!
//!     Ok(())
//! }
//! ```

pub mod cli;
pub mod error;
pub mod gemini;
pub mod markdown;
pub mod models;
pub mod openai;
pub mod output;
pub mod wechat;

pub use error::{Error, Result};
pub use models::{Config, Frontmatter};
// Core uploader functionality is implemented directly in this module

use std::path::Path;

/// Core uploader functionality combining WeChat and image generation clients
pub struct WxUploader {
    wechat_client: wechat::WeChatClient,
    config: Config,
}

impl WxUploader {
    /// Creates a new uploader instance with the provided configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration containing API keys and settings
    ///
    /// # Errors
    ///
    /// Returns an error if WeChat client initialization fails
    pub async fn new(config: Config) -> Result<Self> {
        let wechat_client = wechat::WeChatClient::new(
            config.wechat_app_id.clone(),
            config.wechat_app_secret.clone(),
        )
        .await
        .map_err(|e| Error::wechat(e.to_string()))?;

        Ok(Self {
            wechat_client,
            config,
        })
    }

    /// Forces a refresh of the WeChat access token
    ///
    /// This clears the in-memory token cache and fetches a new token from the WeChat API.
    ///
    /// # Errors
    ///
    /// Returns an error if the token refresh fails
    pub async fn refresh_token(&self) -> Result<String> {
        self.wechat_client
            .refresh_token()
            .await
            .map_err(|e| Error::wechat(e.to_string()))
    }

    /// Uploads a single markdown file to WeChat
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the markdown file
    /// * `force` - If true, uploads regardless of published status
    ///
    /// # Errors
    ///
    /// Returns an error if the upload process fails
    pub async fn upload_file<P: AsRef<Path>>(&self, path: P, force: bool) -> Result<()> {
        wechat::upload_file(
            &self.wechat_client,
            &self.config,
            path.as_ref(),
            force,
            self.config.verbose,
        )
        .await
    }

    /// Processes all markdown files in a directory recursively
    ///
    /// Files marked as published will be skipped unless forced.
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory path to process
    ///
    /// # Errors
    ///
    /// Returns an error if directory processing fails
    pub async fn process_directory<P: AsRef<Path>>(&self, dir: P) -> Result<()> {
        wechat::process_directory(
            &self.wechat_client,
            &self.config,
            dir.as_ref(),
            self.config.verbose,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_uploader_creation_without_keys() {
        let config = Config {
            wechat_app_id: "test_app_id".to_string(),
            wechat_app_secret: "test_secret".to_string(),
            openai_api_key: None,
            gemini_api_key: None,
            verbose: false,
        };

        // This would fail in real scenario without valid WeChat credentials,
        // but tests the structure
        let result = WxUploader::new(config).await;
        // We expect this to fail with network/auth error, not a compilation error
        assert!(result.is_err());
    }

    #[test]
    fn test_config_from_env_missing_required() {
        // Clear environment variables
        unsafe {
            std::env::remove_var("WECHAT_APP_ID");
            std::env::remove_var("WECHAT_APP_SECRET");
        }

        let result = Config::from_env();
        assert!(result.is_err());
    }
}
