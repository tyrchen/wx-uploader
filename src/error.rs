//! Error types for the wx-uploader library
//!
//! This module provides comprehensive error handling using `thiserror` for
//! library errors and integrates with `anyhow` for application-level error handling.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for the wx-uploader library
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the wx-uploader library
#[derive(Error, Debug)]
pub enum Error {
    /// I/O operation failed
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// WeChat API error
    #[error("WeChat API error: {message}")]
    WeChat { message: String },

    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// YAML parsing or serialization error
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON parsing or serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Regular expression error
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    /// File not found
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    /// Invalid file format
    #[error("Invalid file format for {path}: {reason}")]
    InvalidFormat { path: PathBuf, reason: String },

    /// Environment variable missing
    #[error("Missing required environment variable: {var}")]
    MissingEnvVar { var: String },

    /// OpenAI API error
    #[error("OpenAI API error: {message}")]
    OpenAI { message: String },

    /// Gemini API error
    #[error("Gemini API error: {message}")]
    Gemini { message: String },

    /// Cover image error
    #[error("Cover image error for {path}: {reason}")]
    CoverImage { path: PathBuf, reason: String },

    /// Markdown parsing error
    #[error("Markdown parsing error for {path}: {reason}")]
    MarkdownParse { path: PathBuf, reason: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// Generic error with context
    #[error("Operation failed: {message}")]
    Generic { message: String },
}

impl Error {
    /// Creates a new file not found error
    pub fn file_not_found(path: impl Into<PathBuf>) -> Self {
        Self::FileNotFound { path: path.into() }
    }

    /// Creates a new invalid format error
    pub fn invalid_format(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::InvalidFormat {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Creates a new missing environment variable error
    pub fn missing_env_var(var: impl Into<String>) -> Self {
        Self::MissingEnvVar { var: var.into() }
    }

    /// Creates a new OpenAI API error
    pub fn openai(message: impl Into<String>) -> Self {
        Self::OpenAI {
            message: message.into(),
        }
    }

    /// Creates a new Gemini API error
    pub fn gemini(message: impl Into<String>) -> Self {
        Self::Gemini {
            message: message.into(),
        }
    }

    /// Creates a new cover image error
    pub fn cover_image(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::CoverImage {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Creates a new markdown parsing error
    pub fn markdown_parse(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::MarkdownParse {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Creates a new configuration error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    /// Creates a generic error with context
    pub fn generic(message: impl Into<String>) -> Self {
        Self::Generic {
            message: message.into(),
        }
    }

    /// Creates a new WeChat API error
    pub fn wechat(message: impl Into<String>) -> Self {
        Self::WeChat {
            message: message.into(),
        }
    }
}

/// Conversion from anyhow::Error for compatibility
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Self::Generic {
            message: err.to_string(),
        }
    }
}

// Note: We don't implement From<wechat_pub_rs::Error> because the exact error type
// varies between versions. Instead, we handle WeChat errors manually in the wechat module.

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_error_creation() {
        let path = Path::new("test.md");

        let file_not_found = Error::file_not_found(path);
        assert!(matches!(file_not_found, Error::FileNotFound { .. }));
        assert!(file_not_found.to_string().contains("test.md"));

        let invalid_format = Error::invalid_format(path, "malformed YAML");
        assert!(matches!(invalid_format, Error::InvalidFormat { .. }));
        assert!(invalid_format.to_string().contains("malformed YAML"));

        let missing_env = Error::missing_env_var("WECHAT_APP_ID");
        assert!(matches!(missing_env, Error::MissingEnvVar { .. }));
        assert!(missing_env.to_string().contains("WECHAT_APP_ID"));

        let openai_error = Error::openai("API rate limit exceeded");
        assert!(matches!(openai_error, Error::OpenAI { .. }));
        assert!(openai_error.to_string().contains("rate limit"));

        let cover_error = Error::cover_image(path, "download failed");
        assert!(matches!(cover_error, Error::CoverImage { .. }));
        assert!(cover_error.to_string().contains("download failed"));

        let markdown_error = Error::markdown_parse(path, "invalid frontmatter");
        assert!(matches!(markdown_error, Error::MarkdownParse { .. }));
        assert!(markdown_error.to_string().contains("invalid frontmatter"));

        let config_error = Error::config("invalid configuration");
        assert!(matches!(config_error, Error::Config { .. }));
        assert!(config_error.to_string().contains("invalid configuration"));

        let generic_error = Error::generic("something went wrong");
        assert!(matches!(generic_error, Error::Generic { .. }));
        assert!(generic_error.to_string().contains("something went wrong"));
    }

    #[test]
    fn test_anyhow_conversion() {
        let anyhow_error = anyhow::anyhow!("test error message");
        let our_error: Error = anyhow_error.into();

        assert!(matches!(our_error, Error::Generic { .. }));
        assert!(our_error.to_string().contains("test error message"));
    }

    #[test]
    fn test_error_chain() {
        use std::io;

        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let our_error: Error = io_error.into();

        assert!(matches!(our_error, Error::Io(_)));
        assert!(our_error.to_string().contains("I/O error"));
    }
}
