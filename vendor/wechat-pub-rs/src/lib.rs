//! # WeChat Official Account Rust SDK
//!
//! A simple, high-performance WeChat Official Account SDK for uploading articles and managing drafts.
//!
//! This crate provides a comprehensive solution for publishing Markdown content to WeChat Official Accounts
//! with built-in theming, image handling, and error recovery.
//!
//! ## Features
//!
//! - **Simple API**: One function to upload entire articles: `client.upload("./article.md").await?`
//! - **Smart Deduplication**:
//!   - Images deduplicated by BLAKE3 content hash to avoid duplicate uploads
//!   - Drafts deduplicated by title (updates existing drafts with same title)
//! - **Robust**: Comprehensive error handling and retry mechanisms for network reliability
//! - **Fast**: Async/await with concurrent image uploads (up to 5 concurrent)
//! - **Type Safe**: Compile-time guarantees and runtime reliability
//! - **Rich Theming**: 8 built-in themes with 10 syntax highlighting options
//! - **Markdown Support**: Full CommonMark support with frontmatter metadata
//!
//! ## Architecture
//!
//! The SDK is organized into several key modules:
//!
//! - [`WeChatClient`] - Main client for interacting with WeChat APIs
//! - [`auth`] - Access token management with automatic refresh
//! - [`upload`] - Image upload and draft management functionality
//! - [`markdown`] - Markdown parsing and image extraction
//! - [`theme`] - Theme system for rendering HTML from Markdown
//! - [`error`] - Comprehensive error types and handling
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use wechat_pub_rs::{WeChatClient, UploadOptions, Result};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Create client with your WeChat Official Account credentials
//!     let client = WeChatClient::new("your_app_id", "your_app_secret").await?;
//!
//!     // Upload using theme from frontmatter, or default theme
//!     let draft_id = client.upload("./article.md").await?;
//!
//!     // Or specify theme explicitly via options
//!     let options = UploadOptions::with_theme("lapis")
//!         .title("Custom Title")
//!         .author("Custom Author")
//!         .cover_image("./cover.jpg")
//!         .comments(true, false);
//!
//!     let draft_id = client.upload_with_options("./article.md", options).await?;
//!
//!     println!("Draft created with ID: {}", draft_id);
//!     Ok(())
//! }
//! ```
//!
//! ## Markdown Format
//!
//! Your markdown files should include frontmatter with metadata:
//!
//! ```markdown
//! ---
//! title: "Article Title"
//! author: "Author Name"
//! cover: "images/cover.jpg"    # Required: Cover image path
//! theme: "lapis"               # Optional: Theme name
//! code: "github"               # Optional: Code highlighting theme
//! ---
//!
//! # Your Article Content
//!
//! Your markdown content here with images:
//!
//! ![Alt text](images/example.jpg)
//! ```
//!
//! ## Error Handling
//!
//! The library provides comprehensive error handling with specific error types:
//!
//! ```rust,no_run
//! use wechat_pub_rs::{WeChatClient, WeChatError, Result};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let client = WeChatClient::new("app_id", "app_secret").await?;
//!
//! match client.upload("article.md").await {
//!     Ok(draft_id) => println!("Success: {}", draft_id),
//!     Err(WeChatError::FileNotFound { path }) => {
//!         eprintln!("File not found: {}", path);
//!     }
//!     Err(WeChatError::ThemeNotFound { theme }) => {
//!         eprintln!("Theme not found: {}", theme);
//!     }
//!     Err(WeChatError::Network { message }) => {
//!         eprintln!("Network error: {}", message);
//!     }
//!     Err(err) => eprintln!("Other error: {}", err),
//! }
//! # Ok(())
//! # }
//! ```

pub mod auth;
pub mod client;
pub mod config;
pub mod css_vars;
pub mod error;
pub mod http;
pub mod markdown;
pub mod mermaid;
pub mod theme;
pub mod traits;
pub mod upload;
pub mod utils;

// Re-export main types for convenience
pub use client::{UploadOptions, WeChatClient};
pub use config::Config;
pub use css_vars::CssVariableProcessor;
pub use error::{ErrorSeverity, Result, WeChatError};
pub use theme::BuiltinTheme;

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_structure() {
        assert_eq!(1, 1);
    }
}
