//! Data models and configuration for the wx-uploader library
//!
//! This module contains core data structures used throughout the application,
//! including configuration, frontmatter parsing, and validation logic.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::env;

/// Configuration for the wx-uploader application
///
/// Contains all necessary API keys and settings for WeChat and OpenAI integration.
#[derive(Debug, Clone)]
pub struct Config {
    /// WeChat application ID
    pub wechat_app_id: String,
    /// WeChat application secret
    pub wechat_app_secret: String,
    /// Optional OpenAI API key for cover image generation
    pub openai_api_key: Option<String>,
    /// Optional Gemini API key for cover image generation
    pub gemini_api_key: Option<String>,
    /// Enable verbose logging
    pub verbose: bool,
}

impl Config {
    /// Creates a new configuration from environment variables
    ///
    /// # Required Environment Variables
    ///
    /// - `WECHAT_APP_ID`: WeChat application ID
    /// - `WECHAT_APP_SECRET`: WeChat application secret
    ///
    /// # Optional Environment Variables
    ///
    /// - `OPENAI_API_KEY`: OpenAI API key for cover image generation
    ///
    /// # Errors
    ///
    /// Returns an error if required environment variables are not set
    pub fn from_env() -> Result<Self> {
        let wechat_app_id =
            env::var("WECHAT_APP_ID").map_err(|_| Error::missing_env_var("WECHAT_APP_ID"))?;

        let wechat_app_secret = env::var("WECHAT_APP_SECRET")
            .map_err(|_| Error::missing_env_var("WECHAT_APP_SECRET"))?;

        let openai_api_key = env::var("OPENAI_API_KEY").ok();
        let gemini_api_key = env::var("GEMINI_API_KEY").ok();

        Ok(Self {
            wechat_app_id,
            wechat_app_secret,
            openai_api_key,
            gemini_api_key,
            verbose: false, // Default to false, can be overridden by CLI
        })
    }

    /// Creates a new configuration with explicit values
    pub fn new(
        wechat_app_id: String,
        wechat_app_secret: String,
        openai_api_key: Option<String>,
        gemini_api_key: Option<String>,
        verbose: bool,
    ) -> Self {
        Self {
            wechat_app_id,
            wechat_app_secret,
            openai_api_key,
            gemini_api_key,
            verbose,
        }
    }

    /// Sets the verbose flag
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Validates the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - WeChat app ID is empty
    /// - WeChat app secret is empty
    pub fn validate(&self) -> Result<()> {
        if self.wechat_app_id.trim().is_empty() {
            return Err(Error::config("WeChat app ID cannot be empty"));
        }

        if self.wechat_app_secret.trim().is_empty() {
            return Err(Error::config("WeChat app secret cannot be empty"));
        }

        Ok(())
    }
}

/// YAML frontmatter structure for markdown files.
///
/// This struct represents the frontmatter that can be present at the beginning
/// of markdown files. It supports common fields like `title` and `published`,
/// and uses `#[serde(flatten)]` to capture any additional fields.
///
/// # Examples
///
/// ```yaml
/// ---
/// title: "My Article"
/// published: "draft"
/// author: "John Doe"
/// tags: ["rust", "wechat"]
/// theme: "lapis"
/// code: "github"
/// cover: "cover.png"
/// ---
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct Frontmatter {
    /// The title of the article. Optional field that will be omitted from
    /// serialization if not present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Publication status of the article.
    ///
    /// Common values:
    /// - `None` or missing: not uploaded
    /// - `"draft"`: uploaded as draft to WeChat
    /// - `"true"`: published (will be skipped in directory mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,

    /// Cover image filename for the article.
    /// If missing, the system will attempt to generate one using AI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,

    /// Image generation model alias.
    ///
    /// Available models: nb2 (default, Gemini Flash), nb (Gemini Pro), gpt (OpenAI)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Theme for the WeChat article styling.
    ///
    /// Available themes: default, lapis, maize, orangeheart, phycat, pie, purple, rainbow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,

    /// Code highlighter for syntax highlighting.
    ///
    /// Available highlighters: github, github-dark, vscode, atom-one-light, atom-one-dark,
    /// solarized-light, solarized-dark, monokai, dracula, xcode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Description of the article.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,

    /// Captures any additional fields in the frontmatter that are not
    /// explicitly defined in this struct.
    #[serde(flatten)]
    pub other: serde_yaml::Value,
}

impl Frontmatter {
    /// Creates a new empty frontmatter
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a frontmatter with a title
    pub fn with_title(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            ..Default::default()
        }
    }

    /// Sets the title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    /// Sets the published status
    pub fn set_published(&mut self, status: impl Into<String>) {
        self.published = Some(status.into());
    }

    /// Sets the cover image
    pub fn set_cover(&mut self, cover: impl Into<String>) {
        self.cover = Some(cover.into());
    }

    /// Sets the theme
    pub fn set_theme(&mut self, theme: impl Into<String>) {
        self.theme = Some(theme.into());
    }

    /// Sets the code highlighter
    pub fn set_code_highlighter(&mut self, code: impl Into<String>) {
        self.code = Some(code.into());
    }

    /// Checks if the article is published
    pub fn is_published(&self) -> bool {
        // Check the published field first
        if matches!(self.published.as_deref(), Some("true") | Some("\"true\"")) {
            return true;
        }

        // Check if published is a boolean true in the other field
        if let serde_yaml::Value::Mapping(map) = &self.other
            && let Some(serde_yaml::Value::Bool(true)) =
                map.get(serde_yaml::Value::String("published".to_string()))
        {
            return true;
        }

        false
    }

    /// Checks if the article is a draft
    pub fn is_draft(&self) -> bool {
        matches!(self.published.as_deref(), Some("draft"))
    }

    /// Checks if the article is unpublished
    pub fn is_unpublished(&self) -> bool {
        self.published.is_none() || self.published.as_deref() == Some("")
    }

    /// Returns the effective model alias, defaulting to "nb2"
    pub fn effective_model(&self) -> &str {
        self.model.as_deref().unwrap_or("nb2")
    }

    /// Validates the frontmatter
    pub fn validate(&self) -> Result<()> {
        // Validate theme if present
        if let Some(theme) = &self.theme
            && !is_valid_theme(theme)
        {
            return Err(Error::config(format!(
                "Invalid theme '{}'. Available themes: {}",
                theme,
                VALID_THEMES.join(", ")
            )));
        }

        // Validate code highlighter if present
        if let Some(code) = &self.code
            && !is_valid_code_highlighter(code)
        {
            return Err(Error::config(format!(
                "Invalid code highlighter '{}'. Available highlighters: {}",
                code,
                VALID_CODE_HIGHLIGHTERS.join(", ")
            )));
        }

        // Validate model if present
        if let Some(model) = &self.model
            && !is_valid_model(model)
        {
            return Err(Error::config(format!(
                "Invalid model '{}'. Available models: {}",
                model,
                VALID_MODELS.join(", ")
            )));
        }

        Ok(())
    }
}

/// Valid themes for WeChat articles
pub const VALID_THEMES: &[&str] = &[
    "default",
    "lapis",
    "maize",
    "orangeheart",
    "phycat",
    "pie",
    "purple",
    "rainbow",
];

/// Valid image generation models
pub const VALID_MODELS: &[&str] = &["nb2", "nb", "gpt"];

/// Valid code highlighters for syntax highlighting
pub const VALID_CODE_HIGHLIGHTERS: &[&str] = &[
    "github",
    "github-dark",
    "vscode",
    "atom-one-light",
    "atom-one-dark",
    "solarized-light",
    "solarized-dark",
    "monokai",
    "dracula",
    "xcode",
];

/// Checks if a theme is valid
pub fn is_valid_theme(theme: &str) -> bool {
    VALID_THEMES.contains(&theme)
}

/// Checks if a code highlighter is valid
pub fn is_valid_code_highlighter(highlighter: &str) -> bool {
    VALID_CODE_HIGHLIGHTERS.contains(&highlighter)
}

/// Checks if a model alias is valid
pub fn is_valid_model(model: &str) -> bool {
    VALID_MODELS.contains(&model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_config_creation() {
        let config = Config::new(
            "test_app_id".to_string(),
            "test_secret".to_string(),
            Some("test_openai_key".to_string()),
            None,
            true,
        );

        assert_eq!(config.wechat_app_id, "test_app_id");
        assert_eq!(config.wechat_app_secret, "test_secret");
        assert_eq!(config.openai_api_key, Some("test_openai_key".to_string()));
        assert!(config.verbose);
    }

    #[test]
    fn test_config_with_verbose() {
        let config = Config::new(
            "test_app_id".to_string(),
            "test_secret".to_string(),
            None,
            None,
            false,
        )
        .with_verbose(true);

        assert!(config.verbose);
    }

    #[test]
    fn test_config_validation() {
        let valid_config = Config::new(
            "app_id".to_string(),
            "secret".to_string(),
            None,
            None,
            false,
        );
        assert!(valid_config.validate().is_ok());

        let empty_app_id = Config::new("".to_string(), "secret".to_string(), None, None, false);
        assert!(empty_app_id.validate().is_err());

        let empty_secret = Config::new("app_id".to_string(), "".to_string(), None, None, false);
        assert!(empty_secret.validate().is_err());
    }

    #[test]
    fn test_config_from_env() {
        // Set environment variables
        unsafe {
            env::set_var("WECHAT_APP_ID", "test_id");
            env::set_var("WECHAT_APP_SECRET", "test_secret");
            env::set_var("OPENAI_API_KEY", "test_openai");
        }

        let config = Config::from_env().unwrap();
        assert_eq!(config.wechat_app_id, "test_id");
        assert_eq!(config.wechat_app_secret, "test_secret");
        assert_eq!(config.openai_api_key, Some("test_openai".to_string()));

        // Clean up
        unsafe {
            env::remove_var("WECHAT_APP_ID");
            env::remove_var("WECHAT_APP_SECRET");
            env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    fn test_frontmatter_creation() {
        let frontmatter = Frontmatter::new();
        assert_eq!(frontmatter.title, None);
        assert_eq!(frontmatter.published, None);
        assert_eq!(frontmatter.cover, None);

        let frontmatter = Frontmatter::with_title("Test Article");
        assert_eq!(frontmatter.title, Some("Test Article".to_string()));
    }

    #[test]
    fn test_frontmatter_methods() {
        let mut frontmatter = Frontmatter::new();

        frontmatter.set_title("My Article");
        frontmatter.set_published("draft");
        frontmatter.set_cover("cover.png");
        frontmatter.set_theme("lapis");
        frontmatter.set_code_highlighter("github");

        assert_eq!(frontmatter.title, Some("My Article".to_string()));
        assert_eq!(frontmatter.published, Some("draft".to_string()));
        assert_eq!(frontmatter.cover, Some("cover.png".to_string()));
        assert_eq!(frontmatter.theme, Some("lapis".to_string()));
        assert_eq!(frontmatter.code, Some("github".to_string()));

        assert!(frontmatter.is_draft());
        assert!(!frontmatter.is_published());
        assert!(!frontmatter.is_unpublished());
    }

    #[test]
    fn test_frontmatter_status_checks() {
        let mut frontmatter = Frontmatter::new();

        // Test unpublished
        assert!(frontmatter.is_unpublished());
        assert!(!frontmatter.is_draft());
        assert!(!frontmatter.is_published());

        // Test draft
        frontmatter.set_published("draft");
        assert!(frontmatter.is_draft());
        assert!(!frontmatter.is_published());
        assert!(!frontmatter.is_unpublished());

        // Test published
        frontmatter.set_published("true");
        assert!(frontmatter.is_published());
        assert!(!frontmatter.is_draft());
        assert!(!frontmatter.is_unpublished());
    }

    #[test]
    fn test_frontmatter_validation() {
        let mut frontmatter = Frontmatter::new();

        // Valid frontmatter
        assert!(frontmatter.validate().is_ok());

        // Valid theme and code
        frontmatter.set_theme("lapis");
        frontmatter.set_code_highlighter("github");
        assert!(frontmatter.validate().is_ok());

        // Invalid theme
        frontmatter.set_theme("invalid_theme");
        assert!(frontmatter.validate().is_err());

        // Fix theme, invalid code
        frontmatter.set_theme("lapis");
        frontmatter.set_code_highlighter("invalid_highlighter");
        assert!(frontmatter.validate().is_err());
    }

    #[test]
    fn test_theme_validation() {
        assert!(is_valid_theme("lapis"));
        assert!(is_valid_theme("default"));
        assert!(!is_valid_theme("invalid"));
        assert!(!is_valid_theme(""));
    }

    #[test]
    fn test_code_highlighter_validation() {
        assert!(is_valid_code_highlighter("github"));
        assert!(is_valid_code_highlighter("monokai"));
        assert!(!is_valid_code_highlighter("invalid"));
        assert!(!is_valid_code_highlighter(""));
    }

    #[test]
    fn test_frontmatter_serialization() {
        let frontmatter = Frontmatter {
            title: Some("Test Article".to_string()),
            published: Some("draft".to_string()),
            description: "Test Article".to_string(),
            cover: Some("cover.png".to_string()),
            model: None,
            theme: Some("lapis".to_string()),
            code: Some("github".to_string()),
            other: serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        };

        let yaml = serde_yaml::to_string(&frontmatter).unwrap();
        assert!(yaml.contains("title: Test Article"));
        assert!(yaml.contains("published: draft"));
        assert!(yaml.contains("cover: cover.png"));
        assert!(yaml.contains("theme: lapis"));
        assert!(yaml.contains("code: github"));

        let deserialized: Frontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(frontmatter, deserialized);
    }
}
