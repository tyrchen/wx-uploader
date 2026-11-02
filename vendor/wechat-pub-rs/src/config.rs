//! Configuration management for the WeChat Official Account SDK.
//!
//! This module provides a centralized configuration system that enables:
//! - Type-safe configuration management
//! - Environment variable integration
//! - YAML configuration file support
//! - Builder pattern for easy setup
//! - Configuration validation
//!
//! ## Usage
//!
//! ```rust
//! use wechat_pub_rs::config::{Config, SecurityConfig, PerformanceConfig, HttpConfig};
//! use wechat_pub_rs::Result;
//!
//! fn example() -> Result<()> {
//!     // Create default configuration
//!     let config = Config::default();
//!
//!     // Build custom configuration
//!     let config = Config::builder()
//!         .security(SecurityConfig::builder()
//!             .max_upload_size(20 * 1024 * 1024) // 20MB
//!             .build())
//!         .http(HttpConfig::builder()
//!             .request_timeout_secs(60)
//!             .build())
//!         .performance(PerformanceConfig::builder()
//!             .max_concurrent_uploads(10)
//!             .cache_ttl_minutes(30)
//!             .build())
//!         .build();
//!
//!     // Load from environment variables
//!     let config = Config::from_env()?;
//!     Ok(())
//! }
//! ```

use crate::error::{Result, WeChatError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main configuration structure for the WeChat SDK.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Security-related configuration
    pub security: SecurityConfig,
    /// Performance-related configuration
    pub performance: PerformanceConfig,
    /// HTTP client configuration
    pub http: HttpConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Retry configuration
    pub retry: RetryConfig,
}

/// Security configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Maximum allowed file size for uploads in bytes (default: 10MB)
    pub max_upload_size: u64,
    /// Maximum allowed file size for downloads in bytes (default: 20MB)
    pub max_download_size: u64,
    /// Whether to validate file paths for security (default: true)
    pub validate_file_paths: bool,
    /// Whether to sanitize filenames (default: true)
    pub sanitize_filenames: bool,
    /// List of blocked file extensions for security
    pub blocked_extensions: Vec<String>,
}

/// Performance configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum number of concurrent uploads (default: 5)
    pub max_concurrent_uploads: usize,
    /// Cache TTL in minutes (default: 15)
    pub cache_ttl_minutes: u64,
    /// Maximum cache size in entries (default: 1000)
    pub max_cache_entries: usize,
    /// Whether to enable parallel processing (default: true)
    pub enable_parallel_processing: bool,
}

/// HTTP client configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Request timeout in seconds (default: 30)
    pub request_timeout_secs: u64,
    /// Connection timeout in seconds (default: 10)
    pub connect_timeout_secs: u64,
    /// Base URL for WeChat API (default: "https://api.weixin.qq.com")
    pub base_url: String,
    /// User agent string for requests
    pub user_agent: String,
}

/// Cache configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Whether to enable material lookup caching (default: true)
    pub enable_material_cache: bool,
    /// Whether to enable token caching (default: true)
    pub enable_token_cache: bool,
    /// Cache cleanup interval in minutes (default: 60)
    pub cleanup_interval_minutes: u64,
}

/// Retry configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    pub max_attempts: u32,
    /// Base delay between retries in milliseconds (default: 500)
    pub base_delay_ms: u64,
    /// Maximum delay between retries in seconds (default: 30)
    pub max_delay_secs: u64,
    /// Exponential backoff factor (default: 2.0)
    pub backoff_factor: f64,
    /// Whether to add jitter to retry delays (default: true)
    pub enable_jitter: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_upload_size: 10 * 1024 * 1024,   // 10MB
            max_download_size: 20 * 1024 * 1024, // 20MB
            validate_file_paths: true,
            sanitize_filenames: true,
            blocked_extensions: vec![
                "exe".to_string(),
                "bat".to_string(),
                "cmd".to_string(),
                "com".to_string(),
                "pif".to_string(),
                "scr".to_string(),
                "vbs".to_string(),
                "js".to_string(),
                "jar".to_string(),
                "sh".to_string(),
                "php".to_string(),
                "asp".to_string(),
                "aspx".to_string(),
                "jsp".to_string(),
            ],
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uploads: 5,
            cache_ttl_minutes: 15,
            max_cache_entries: 1000,
            enable_parallel_processing: true,
        }
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: 30,
            connect_timeout_secs: 10,
            base_url: "https://api.weixin.qq.com".to_string(),
            user_agent: format!("wechat-pub-rs/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enable_material_cache: true,
            enable_token_cache: true,
            cleanup_interval_minutes: 60,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            max_delay_secs: 30,
            backoff_factor: 2.0,
            enable_jitter: true,
        }
    }
}

impl Config {
    /// Creates a new configuration builder.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// Loads configuration from environment variables.
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        // Security settings
        if let Ok(val) = std::env::var("WECHAT_MAX_UPLOAD_SIZE") {
            config.security.max_upload_size = val
                .parse()
                .map_err(|_| WeChatError::config_error("Invalid WECHAT_MAX_UPLOAD_SIZE value"))?;
        }

        if let Ok(val) = std::env::var("WECHAT_MAX_DOWNLOAD_SIZE") {
            config.security.max_download_size = val
                .parse()
                .map_err(|_| WeChatError::config_error("Invalid WECHAT_MAX_DOWNLOAD_SIZE value"))?;
        }

        // Performance settings
        if let Ok(val) = std::env::var("WECHAT_MAX_CONCURRENT_UPLOADS") {
            config.performance.max_concurrent_uploads = val.parse().map_err(|_| {
                WeChatError::config_error("Invalid WECHAT_MAX_CONCURRENT_UPLOADS value")
            })?;
        }

        if let Ok(val) = std::env::var("WECHAT_CACHE_TTL_MINUTES") {
            config.performance.cache_ttl_minutes = val
                .parse()
                .map_err(|_| WeChatError::config_error("Invalid WECHAT_CACHE_TTL_MINUTES value"))?;
        }

        // HTTP settings
        if let Ok(val) = std::env::var("WECHAT_REQUEST_TIMEOUT") {
            config.http.request_timeout_secs = val
                .parse()
                .map_err(|_| WeChatError::config_error("Invalid WECHAT_REQUEST_TIMEOUT value"))?;
        }

        if let Ok(val) = std::env::var("WECHAT_BASE_URL") {
            config.http.base_url = val;
        }

        // Retry settings
        if let Ok(val) = std::env::var("WECHAT_MAX_RETRIES") {
            config.retry.max_attempts = val
                .parse()
                .map_err(|_| WeChatError::config_error("Invalid WECHAT_MAX_RETRIES value"))?;
        }

        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration for consistency and constraints.
    pub fn validate(&self) -> Result<()> {
        // Validate security settings
        if self.security.max_upload_size == 0 {
            return Err(WeChatError::config_error(
                "max_upload_size must be greater than 0",
            ));
        }

        if self.security.max_download_size == 0 {
            return Err(WeChatError::config_error(
                "max_download_size must be greater than 0",
            ));
        }

        // Validate performance settings
        if self.performance.max_concurrent_uploads == 0 {
            return Err(WeChatError::config_error(
                "max_concurrent_uploads must be greater than 0",
            ));
        }

        if self.performance.max_concurrent_uploads > 20 {
            return Err(WeChatError::config_error(
                "max_concurrent_uploads should not exceed 20",
            ));
        }

        // Validate HTTP settings
        if self.http.request_timeout_secs == 0 {
            return Err(WeChatError::config_error(
                "request_timeout_secs must be greater than 0",
            ));
        }

        if self.http.connect_timeout_secs == 0 {
            return Err(WeChatError::config_error(
                "connect_timeout_secs must be greater than 0",
            ));
        }

        if self.http.base_url.is_empty() {
            return Err(WeChatError::config_error("base_url cannot be empty"));
        }

        // Validate retry settings
        if self.retry.max_attempts == 0 {
            return Err(WeChatError::config_error(
                "max_attempts must be greater than 0",
            ));
        }

        if self.retry.backoff_factor < 1.0 {
            return Err(WeChatError::config_error("backoff_factor must be >= 1.0"));
        }

        Ok(())
    }

    /// Converts retry config to Duration types for easier use.
    pub fn retry_base_delay(&self) -> Duration {
        Duration::from_millis(self.retry.base_delay_ms)
    }

    /// Converts retry config to Duration types for easier use.
    pub fn retry_max_delay(&self) -> Duration {
        Duration::from_secs(self.retry.max_delay_secs)
    }

    /// Converts HTTP timeout to Duration types for easier use.
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.http.request_timeout_secs)
    }

    /// Converts HTTP timeout to Duration types for easier use.
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.http.connect_timeout_secs)
    }

    /// Converts cache TTL to Duration types for easier use.
    pub fn cache_ttl(&self) -> Duration {
        Duration::from_secs(self.performance.cache_ttl_minutes * 60)
    }
}

/// Builder for creating Config instances.
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    security: Option<SecurityConfig>,
    performance: Option<PerformanceConfig>,
    http: Option<HttpConfig>,
    cache: Option<CacheConfig>,
    retry: Option<RetryConfig>,
}

impl ConfigBuilder {
    /// Sets the security configuration.
    pub fn security(mut self, security: SecurityConfig) -> Self {
        self.security = Some(security);
        self
    }

    /// Sets the performance configuration.
    pub fn performance(mut self, performance: PerformanceConfig) -> Self {
        self.performance = Some(performance);
        self
    }

    /// Sets the HTTP configuration.
    pub fn http(mut self, http: HttpConfig) -> Self {
        self.http = Some(http);
        self
    }

    /// Sets the cache configuration.
    pub fn cache(mut self, cache: CacheConfig) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Sets the retry configuration.
    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Builds the configuration.
    pub fn build(self) -> Config {
        Config {
            security: self.security.unwrap_or_default(),
            performance: self.performance.unwrap_or_default(),
            http: self.http.unwrap_or_default(),
            cache: self.cache.unwrap_or_default(),
            retry: self.retry.unwrap_or_default(),
        }
    }
}

// Builder implementations for individual config sections

impl SecurityConfig {
    /// Creates a new security config builder.
    pub fn builder() -> SecurityConfigBuilder {
        SecurityConfigBuilder::default()
    }
}

impl PerformanceConfig {
    /// Creates a new performance config builder.
    pub fn builder() -> PerformanceConfigBuilder {
        PerformanceConfigBuilder::default()
    }
}

impl HttpConfig {
    /// Creates a new HTTP config builder.
    pub fn builder() -> HttpConfigBuilder {
        HttpConfigBuilder::default()
    }
}

impl CacheConfig {
    /// Creates a new cache config builder.
    pub fn builder() -> CacheConfigBuilder {
        CacheConfigBuilder::default()
    }
}

impl RetryConfig {
    /// Creates a new retry config builder.
    pub fn builder() -> RetryConfigBuilder {
        RetryConfigBuilder::default()
    }
}

/// Builder for SecurityConfig.
#[derive(Debug, Default)]
pub struct SecurityConfigBuilder {
    max_upload_size: Option<u64>,
    max_download_size: Option<u64>,
    validate_file_paths: Option<bool>,
    sanitize_filenames: Option<bool>,
    blocked_extensions: Option<Vec<String>>,
}

impl SecurityConfigBuilder {
    pub fn max_upload_size(mut self, size: u64) -> Self {
        self.max_upload_size = Some(size);
        self
    }

    pub fn max_download_size(mut self, size: u64) -> Self {
        self.max_download_size = Some(size);
        self
    }

    pub fn validate_file_paths(mut self, validate: bool) -> Self {
        self.validate_file_paths = Some(validate);
        self
    }

    pub fn sanitize_filenames(mut self, sanitize: bool) -> Self {
        self.sanitize_filenames = Some(sanitize);
        self
    }

    pub fn blocked_extensions(mut self, extensions: Vec<String>) -> Self {
        self.blocked_extensions = Some(extensions);
        self
    }

    pub fn build(self) -> SecurityConfig {
        let default = SecurityConfig::default();
        SecurityConfig {
            max_upload_size: self.max_upload_size.unwrap_or(default.max_upload_size),
            max_download_size: self.max_download_size.unwrap_or(default.max_download_size),
            validate_file_paths: self
                .validate_file_paths
                .unwrap_or(default.validate_file_paths),
            sanitize_filenames: self
                .sanitize_filenames
                .unwrap_or(default.sanitize_filenames),
            blocked_extensions: self
                .blocked_extensions
                .unwrap_or(default.blocked_extensions),
        }
    }
}

/// Builder for PerformanceConfig.
#[derive(Debug, Default)]
pub struct PerformanceConfigBuilder {
    max_concurrent_uploads: Option<usize>,
    cache_ttl_minutes: Option<u64>,
    max_cache_entries: Option<usize>,
    enable_parallel_processing: Option<bool>,
}

impl PerformanceConfigBuilder {
    pub fn max_concurrent_uploads(mut self, count: usize) -> Self {
        self.max_concurrent_uploads = Some(count);
        self
    }

    pub fn cache_ttl_minutes(mut self, minutes: u64) -> Self {
        self.cache_ttl_minutes = Some(minutes);
        self
    }

    pub fn max_cache_entries(mut self, entries: usize) -> Self {
        self.max_cache_entries = Some(entries);
        self
    }

    pub fn enable_parallel_processing(mut self, enable: bool) -> Self {
        self.enable_parallel_processing = Some(enable);
        self
    }

    pub fn build(self) -> PerformanceConfig {
        let default = PerformanceConfig::default();
        PerformanceConfig {
            max_concurrent_uploads: self
                .max_concurrent_uploads
                .unwrap_or(default.max_concurrent_uploads),
            cache_ttl_minutes: self.cache_ttl_minutes.unwrap_or(default.cache_ttl_minutes),
            max_cache_entries: self.max_cache_entries.unwrap_or(default.max_cache_entries),
            enable_parallel_processing: self
                .enable_parallel_processing
                .unwrap_or(default.enable_parallel_processing),
        }
    }
}

/// Builder for HttpConfig.
#[derive(Debug, Default)]
pub struct HttpConfigBuilder {
    request_timeout_secs: Option<u64>,
    connect_timeout_secs: Option<u64>,
    base_url: Option<String>,
    user_agent: Option<String>,
}

impl HttpConfigBuilder {
    pub fn request_timeout_secs(mut self, timeout: u64) -> Self {
        self.request_timeout_secs = Some(timeout);
        self
    }

    pub fn connect_timeout_secs(mut self, timeout: u64) -> Self {
        self.connect_timeout_secs = Some(timeout);
        self
    }

    pub fn base_url(mut self, url: String) -> Self {
        self.base_url = Some(url);
        self
    }

    pub fn user_agent(mut self, agent: String) -> Self {
        self.user_agent = Some(agent);
        self
    }

    pub fn build(self) -> HttpConfig {
        let default = HttpConfig::default();
        HttpConfig {
            request_timeout_secs: self
                .request_timeout_secs
                .unwrap_or(default.request_timeout_secs),
            connect_timeout_secs: self
                .connect_timeout_secs
                .unwrap_or(default.connect_timeout_secs),
            base_url: self.base_url.unwrap_or(default.base_url),
            user_agent: self.user_agent.unwrap_or(default.user_agent),
        }
    }
}

/// Builder for CacheConfig.
#[derive(Debug, Default)]
pub struct CacheConfigBuilder {
    enable_material_cache: Option<bool>,
    enable_token_cache: Option<bool>,
    cleanup_interval_minutes: Option<u64>,
}

impl CacheConfigBuilder {
    pub fn enable_material_cache(mut self, enable: bool) -> Self {
        self.enable_material_cache = Some(enable);
        self
    }

    pub fn enable_token_cache(mut self, enable: bool) -> Self {
        self.enable_token_cache = Some(enable);
        self
    }

    pub fn cleanup_interval_minutes(mut self, minutes: u64) -> Self {
        self.cleanup_interval_minutes = Some(minutes);
        self
    }

    pub fn build(self) -> CacheConfig {
        let default = CacheConfig::default();
        CacheConfig {
            enable_material_cache: self
                .enable_material_cache
                .unwrap_or(default.enable_material_cache),
            enable_token_cache: self
                .enable_token_cache
                .unwrap_or(default.enable_token_cache),
            cleanup_interval_minutes: self
                .cleanup_interval_minutes
                .unwrap_or(default.cleanup_interval_minutes),
        }
    }
}

/// Builder for RetryConfig.
#[derive(Debug, Default)]
pub struct RetryConfigBuilder {
    max_attempts: Option<u32>,
    base_delay_ms: Option<u64>,
    max_delay_secs: Option<u64>,
    backoff_factor: Option<f64>,
    enable_jitter: Option<bool>,
}

impl RetryConfigBuilder {
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = Some(attempts);
        self
    }

    pub fn base_delay_ms(mut self, delay: u64) -> Self {
        self.base_delay_ms = Some(delay);
        self
    }

    pub fn max_delay_secs(mut self, delay: u64) -> Self {
        self.max_delay_secs = Some(delay);
        self
    }

    pub fn backoff_factor(mut self, factor: f64) -> Self {
        self.backoff_factor = Some(factor);
        self
    }

    pub fn enable_jitter(mut self, enable: bool) -> Self {
        self.enable_jitter = Some(enable);
        self
    }

    pub fn build(self) -> RetryConfig {
        let default = RetryConfig::default();
        RetryConfig {
            max_attempts: self.max_attempts.unwrap_or(default.max_attempts),
            base_delay_ms: self.base_delay_ms.unwrap_or(default.base_delay_ms),
            max_delay_secs: self.max_delay_secs.unwrap_or(default.max_delay_secs),
            backoff_factor: self.backoff_factor.unwrap_or(default.backoff_factor),
            enable_jitter: self.enable_jitter.unwrap_or(default.enable_jitter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());

        // Test default values
        assert_eq!(config.security.max_upload_size, 10 * 1024 * 1024);
        assert_eq!(config.performance.max_concurrent_uploads, 5);
        assert_eq!(config.http.request_timeout_secs, 30);
        assert_eq!(config.retry.max_attempts, 3);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::builder()
            .security(
                SecurityConfig::builder()
                    .max_upload_size(5 * 1024 * 1024)
                    .validate_file_paths(false)
                    .build(),
            )
            .performance(
                PerformanceConfig::builder()
                    .max_concurrent_uploads(10)
                    .cache_ttl_minutes(30)
                    .build(),
            )
            .build();

        assert_eq!(config.security.max_upload_size, 5 * 1024 * 1024);
        assert!(!config.security.validate_file_paths);
        assert_eq!(config.performance.max_concurrent_uploads, 10);
        assert_eq!(config.performance.cache_ttl_minutes, 30);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        config.security.max_upload_size = 0;
        assert!(config.validate().is_err());

        let mut config = Config::default();
        config.performance.max_concurrent_uploads = 0;
        assert!(config.validate().is_err());

        let mut config = Config::default();
        config.performance.max_concurrent_uploads = 25;
        assert!(config.validate().is_err());

        let mut config = Config::default();
        config.retry.backoff_factor = 0.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_duration_conversions() {
        let config = Config::default();

        assert_eq!(config.retry_base_delay(), Duration::from_millis(500));
        assert_eq!(config.retry_max_delay(), Duration::from_secs(30));
        assert_eq!(config.request_timeout(), Duration::from_secs(30));
        assert_eq!(config.connect_timeout(), Duration::from_secs(10));
        assert_eq!(config.cache_ttl(), Duration::from_secs(15 * 60));
    }

    #[test]
    fn test_environment_loading() {
        // Set some environment variables
        unsafe {
            std::env::set_var("WECHAT_MAX_UPLOAD_SIZE", "5242880"); // 5MB
            std::env::set_var("WECHAT_MAX_CONCURRENT_UPLOADS", "10");
            std::env::set_var("WECHAT_REQUEST_TIMEOUT", "60");
        }

        let config = Config::from_env().unwrap();

        assert_eq!(config.security.max_upload_size, 5242880);
        assert_eq!(config.performance.max_concurrent_uploads, 10);
        assert_eq!(config.http.request_timeout_secs, 60);

        // Clean up
        unsafe {
            std::env::remove_var("WECHAT_MAX_UPLOAD_SIZE");
            std::env::remove_var("WECHAT_MAX_CONCURRENT_UPLOADS");
            std::env::remove_var("WECHAT_REQUEST_TIMEOUT");
        }
    }

    #[test]
    fn test_invalid_environment_values() {
        unsafe {
            std::env::set_var("WECHAT_MAX_UPLOAD_SIZE", "invalid");
        }
        assert!(Config::from_env().is_err());
        unsafe {
            std::env::remove_var("WECHAT_MAX_UPLOAD_SIZE");
        }
    }
}
