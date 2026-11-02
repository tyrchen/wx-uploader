//! Error types and handling for the WeChat Official Account SDK.
//!
//! This module provides comprehensive error handling with specific error types for different
//! failure scenarios. Errors are designed to be actionable and include retry logic.
//!
//! ## Error Categories
//!
//! - **Network Errors**: Connection issues, timeouts (retryable)
//! - **Authentication Errors**: Invalid tokens, credentials (retryable once)
//! - **File System Errors**: Missing files, read failures (not retryable)
//! - **Parsing Errors**: Markdown, JSON parsing failures (not retryable)
//! - **WeChat API Errors**: Server responses with error codes (situational)
//! - **Configuration Errors**: Invalid settings (not retryable)
//!
//! ## Usage
//!
//! ```rust
//! use wechat_pub_rs::{WeChatError, Result};
//! use wechat_pub_rs::error::ErrorSeverity;
//! use tracing::{error, warn};
//!
//! fn handle_error(error: WeChatError) {
//!     match error.severity() {
//!         ErrorSeverity::Warning => {
//!             warn!("Recoverable error: {}", error);
//!             if error.is_retryable() {
//!                 // Implement retry logic
//!             }
//!         }
//!         ErrorSeverity::Error => {
//!             error!("Error occurred: {}", error);
//!         }
//!         ErrorSeverity::Critical => {
//!             error!("Critical error: {}", error);
//!             // May require immediate attention
//!         }
//!     }
//! }
//! ```

use std::fmt;

/// Result type alias for WeChat SDK operations.
pub type Result<T> = std::result::Result<T, WeChatError>;

/// Comprehensive error type for WeChat SDK operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum WeChatError {
    /// Network-related errors (retryable)
    #[error("Network request failed: {message}")]
    Network { message: String },

    /// Request timeout (retryable)
    #[error("Request timeout")]
    Timeout,

    /// Authentication errors (may be retryable once)
    #[error("Invalid access token")]
    InvalidToken,

    #[error("Invalid application credentials")]
    InvalidCredentials,

    /// File system errors (not retryable)
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Failed to read file: {path}, reason: {reason}")]
    FileRead { path: String, reason: String },

    /// Markdown processing errors (not retryable)
    #[error("Markdown parsing failed: {reason}")]
    MarkdownParse { reason: String },

    /// Image processing errors (may be retryable)
    #[error("Image upload failed: {path}, reason: {reason}")]
    ImageUpload { path: String, reason: String },

    /// Theme system errors (not retryable)
    #[error("Theme not found: {theme}")]
    ThemeNotFound { theme: String },

    #[error("Theme rendering failed: {theme}, reason: {reason}")]
    ThemeRender { theme: String, reason: String },

    /// WeChat API errors (retryability depends on error code)
    #[error("WeChat API error [{code}]: {message}")]
    WeChatApi { code: i32, message: String },

    /// Configuration errors (not retryable)
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// JSON serialization/deserialization errors
    #[error("JSON processing failed: {message}")]
    Json { message: String },

    /// I/O errors
    #[error("I/O error: {message}")]
    Io { message: String },

    /// Generic errors for wrapping other error types
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl WeChatError {
    /// Determines if an error is retryable.
    ///
    /// Network errors, timeouts, and certain WeChat API errors are retryable.
    /// Authentication errors are retryable once (token might be expired).
    /// File system, parsing, and configuration errors are not retryable.
    pub fn is_retryable(&self) -> bool {
        match self {
            // Network and timeout errors are always retryable
            WeChatError::Network { .. } | WeChatError::Timeout => true,

            // Authentication errors are retryable once
            WeChatError::InvalidToken => true,

            // Some image upload errors might be retryable (network issues)
            WeChatError::ImageUpload { .. } => true,

            // WeChat API errors - check specific error codes
            WeChatError::WeChatApi { code, .. } => match code {
                // Access token related errors (retryable)
                40001 | 40014 | 42001 | 42007 => true,
                // Rate limiting (retryable with delay)
                45009 | 45011 => true,
                // Server errors (retryable)
                -1 | 50001 | 50002 => true,
                // All other API errors are not retryable
                _ => false,
            },

            // All other errors are not retryable
            _ => false,
        }
    }

    /// Gets the severity level of the error for logging purposes.
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            WeChatError::Network { .. }
            | WeChatError::Timeout
            | WeChatError::ImageUpload { .. } => ErrorSeverity::Warning,

            WeChatError::InvalidToken | WeChatError::InvalidCredentials => ErrorSeverity::Error,

            WeChatError::FileNotFound { .. }
            | WeChatError::FileRead { .. }
            | WeChatError::MarkdownParse { .. }
            | WeChatError::ThemeNotFound { .. }
            | WeChatError::Config { .. } => ErrorSeverity::Error,

            WeChatError::WeChatApi { code, .. } => match code {
                // Critical API errors
                40013 | 48001 => ErrorSeverity::Critical,
                // Regular API errors
                _ => ErrorSeverity::Error,
            },

            WeChatError::ThemeRender { .. }
            | WeChatError::Json { .. }
            | WeChatError::Io { .. }
            | WeChatError::Internal { .. } => ErrorSeverity::Error,
        }
    }

    /// Creates a WeChat API error from response data.
    pub fn from_api_response(code: i32, message: impl Into<String>) -> Self {
        WeChatError::WeChatApi {
            code,
            message: message.into(),
        }
    }

    /// Creates a file-related error.
    pub fn file_error(path: impl Into<String>, reason: impl Into<String>) -> Self {
        WeChatError::FileRead {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Creates a configuration error.
    pub fn config_error(message: impl Into<String>) -> Self {
        WeChatError::Config {
            message: message.into(),
        }
    }

    /// Gets the recommended retry delay for this error type.
    pub fn retry_delay(&self) -> std::time::Duration {
        use std::time::Duration;

        match self {
            // Network errors - exponential backoff starting from 1s
            WeChatError::Network { .. } | WeChatError::Timeout => Duration::from_secs(1),

            // Authentication errors - immediate retry
            WeChatError::InvalidToken => Duration::from_millis(100),

            // Image upload errors - moderate delay
            WeChatError::ImageUpload { .. } => Duration::from_millis(500),

            // WeChat API errors - depends on error code
            WeChatError::WeChatApi { code, .. } => match code {
                // Rate limiting - longer delay
                45009 | 45011 => Duration::from_secs(10),
                // Server errors - moderate delay
                -1 | 50001 | 50002 => Duration::from_secs(2),
                // Token errors - quick retry
                40001 | 40014 | 42001 | 42007 => Duration::from_millis(200),
                // Default delay
                _ => Duration::from_secs(1),
            },

            // Non-retryable errors - no delay (won't be used)
            _ => Duration::ZERO,
        }
    }

    /// Gets the maximum number of retry attempts for this error type.
    pub fn max_retries(&self) -> u32 {
        match self {
            // Network errors - many retries
            WeChatError::Network { .. } | WeChatError::Timeout => 5,

            // Authentication errors - few retries (token refresh should fix it)
            WeChatError::InvalidToken => 2,

            // Image upload errors - moderate retries
            WeChatError::ImageUpload { .. } => 3,

            // WeChat API errors - depends on error code
            WeChatError::WeChatApi { code, .. } => match code {
                // Rate limiting - more retries with longer delays
                45009 | 45011 => 10,
                // Server errors - moderate retries
                -1 | 50001 | 50002 => 3,
                // Token errors - few retries
                40001 | 40014 | 42001 | 42007 => 2,
                // Other API errors - no retries
                _ => 0,
            },

            // Non-retryable errors
            _ => 0,
        }
    }

    /// Determines if this error indicates a temporary service issue.
    pub fn is_temporary(&self) -> bool {
        match self {
            WeChatError::Network { .. } | WeChatError::Timeout => true,
            WeChatError::WeChatApi { code, .. } => match code {
                // Server errors are typically temporary
                -1 | 50001 | 50002 => true,
                // Rate limiting is temporary
                45009 | 45011 => true,
                // Other errors are typically permanent
                _ => false,
            },
            _ => false,
        }
    }

    /// Gets recovery suggestions for this error.
    pub fn recovery_suggestion(&self) -> Option<&'static str> {
        match self {
            WeChatError::InvalidToken => Some("Try refreshing the access token"),
            WeChatError::InvalidCredentials => Some("Check your app_id and app_secret"),
            WeChatError::FileNotFound { .. } => Some("Check if the file path is correct"),
            WeChatError::ImageUpload { .. } => Some("Check file size and format"),
            WeChatError::ThemeNotFound { .. } => Some("Use a valid theme name or 'default'"),
            WeChatError::WeChatApi { code, .. } => match code {
                40001 => Some("Access token expired, refresh and retry"),
                40003 => Some("Check your openid parameter"),
                45009 => Some("Rate limit exceeded, wait and retry"),
                48001 => Some("API unauthorized, check permissions"),
                _ => Some("Check WeChat API documentation for error code"),
            },
            _ => None,
        }
    }
}

impl From<reqwest::Error> for WeChatError {
    fn from(error: reqwest::Error) -> Self {
        WeChatError::Network {
            message: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for WeChatError {
    fn from(error: serde_json::Error) -> Self {
        WeChatError::Json {
            message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for WeChatError {
    fn from(error: std::io::Error) -> Self {
        WeChatError::Io {
            message: error.to_string(),
        }
    }
}

impl From<anyhow::Error> for WeChatError {
    fn from(error: anyhow::Error) -> Self {
        WeChatError::Internal {
            message: error.to_string(),
        }
    }
}

/// Error severity levels for logging and monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Low impact errors that don't affect core functionality
    Warning,
    /// Standard errors that affect specific operations
    Error,
    /// High impact errors that affect core functionality
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Warning => write!(f, "WARNING"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryability() {
        // Timeout errors should be retryable
        let timeout_err = WeChatError::Timeout;
        assert!(timeout_err.is_retryable());

        // File not found should not be retryable
        let file_err = WeChatError::FileNotFound {
            path: "test.md".to_string(),
        };
        assert!(!file_err.is_retryable());

        // Token errors should be retryable
        let token_err = WeChatError::from_api_response(40001, "invalid credential");
        assert!(token_err.is_retryable());

        // Invalid parameter errors should not be retryable
        let param_err = WeChatError::from_api_response(40003, "invalid openid");
        assert!(!param_err.is_retryable());
    }

    #[test]
    fn test_error_severity() {
        let network_err = WeChatError::Timeout;
        assert_eq!(network_err.severity(), ErrorSeverity::Warning);

        let config_err = WeChatError::config_error("missing app_id");
        assert_eq!(config_err.severity(), ErrorSeverity::Error);

        let critical_api_err = WeChatError::from_api_response(40013, "invalid appid");
        assert_eq!(critical_api_err.severity(), ErrorSeverity::Critical);
    }

    #[test]
    fn test_error_creation_helpers() {
        let file_err = WeChatError::file_error("/path/to/file.md", "permission denied");
        match file_err {
            WeChatError::FileRead { path, reason } => {
                assert_eq!(path, "/path/to/file.md");
                assert_eq!(reason, "permission denied");
            }
            _ => panic!("Expected FileRead error"),
        }

        let config_err = WeChatError::config_error("invalid configuration");
        match config_err {
            WeChatError::Config { message } => {
                assert_eq!(message, "invalid configuration");
            }
            _ => panic!("Expected Config error"),
        }
    }

    #[test]
    fn test_retry_delay() {
        // Network errors should have base delay
        let network_err = WeChatError::Network {
            message: "connection failed".to_string(),
        };
        assert_eq!(network_err.retry_delay(), std::time::Duration::from_secs(1));

        // Rate limiting should have longer delay
        let rate_limit_err = WeChatError::from_api_response(45009, "rate limit exceeded");
        assert_eq!(
            rate_limit_err.retry_delay(),
            std::time::Duration::from_secs(10)
        );

        // Token errors should have quick retry
        let token_err = WeChatError::from_api_response(40001, "invalid credential");
        assert_eq!(
            token_err.retry_delay(),
            std::time::Duration::from_millis(200)
        );
    }

    #[test]
    fn test_max_retries() {
        // Network errors should have many retries
        let network_err = WeChatError::Network {
            message: "connection failed".to_string(),
        };
        assert_eq!(network_err.max_retries(), 5);

        // Rate limiting should have more retries
        let rate_limit_err = WeChatError::from_api_response(45009, "rate limit exceeded");
        assert_eq!(rate_limit_err.max_retries(), 10);

        // Non-retryable errors should have no retries
        let config_err = WeChatError::config_error("invalid config");
        assert_eq!(config_err.max_retries(), 0);
    }

    #[test]
    fn test_is_temporary() {
        // Network errors are temporary
        let network_err = WeChatError::Network {
            message: "connection failed".to_string(),
        };
        assert!(network_err.is_temporary());

        // Server errors are temporary
        let server_err = WeChatError::from_api_response(50001, "server error");
        assert!(server_err.is_temporary());

        // Configuration errors are not temporary
        let config_err = WeChatError::config_error("invalid config");
        assert!(!config_err.is_temporary());
    }

    #[test]
    fn test_recovery_suggestion() {
        // Token errors should suggest refresh
        let token_err = WeChatError::InvalidToken;
        assert_eq!(
            token_err.recovery_suggestion(),
            Some("Try refreshing the access token")
        );

        // File not found should suggest checking path
        let file_err = WeChatError::FileNotFound {
            path: "test.md".to_string(),
        };
        assert_eq!(
            file_err.recovery_suggestion(),
            Some("Check if the file path is correct")
        );

        // API errors should provide specific suggestions
        let api_err = WeChatError::from_api_response(40001, "invalid credential");
        assert_eq!(
            api_err.recovery_suggestion(),
            Some("Access token expired, refresh and retry")
        );
    }
}
