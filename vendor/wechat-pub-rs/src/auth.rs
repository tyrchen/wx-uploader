//! Authentication module for managing WeChat access tokens.
//!
//! This module handles the complex process of WeChat access token management,
//! including automatic refresh, caching, and thread-safe access.
//!
//! ## Features
//!
//! - **Automatic Token Refresh**: Tokens are refreshed before expiration
//! - **Thread-Safe Caching**: Multiple threads can safely access tokens
//! - **Expiration Handling**: Built-in buffer time to prevent edge cases
//! - **Concurrent Protection**: Prevents multiple simultaneous refresh requests
//! - **Error Recovery**: Comprehensive error handling for auth failures
//!
//! ## Token Lifecycle
//!
//! 1. **Initial Request**: Token requested on first API call
//! 2. **Caching**: Token cached with expiration time
//! 3. **Validation**: Each use checks if token is still valid
//! 4. **Refresh**: Automatic refresh before expiration (300s buffer)
//! 5. **Cleanup**: Expired tokens are discarded
//!
//! ## Usage
//!
//! ```rust
//! use wechat_pub_rs::auth::TokenManager;
//! use wechat_pub_rs::http::WeChatHttpClient;
//! use std::sync::Arc;
//!
//! # async fn example() -> wechat_pub_rs::Result<()> {
//! let http_client = Arc::new(WeChatHttpClient::new()?);
//! let token_manager = TokenManager::new(
//!     "your_app_id".to_string(),
//!     "your_app_secret".to_string(),
//!     http_client
//! );
//!
//! // Get a valid access token (handles caching and refresh automatically)
//! let token = token_manager.get_access_token().await?;
//! println!("Access token: {}", token);
//!
//! // Force refresh if needed
//! let new_token = token_manager.force_refresh().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Thread Safety
//!
//! The token manager is designed to be shared across multiple threads safely:
//!
//! ```rust
//! use std::sync::Arc;
//! # use wechat_pub_rs::auth::TokenManager;
//! # use wechat_pub_rs::http::WeChatHttpClient;
//!
//! # async fn example() -> wechat_pub_rs::Result<()> {
//! # let http_client = Arc::new(WeChatHttpClient::new()?);
//! let token_manager = Arc::new(TokenManager::new(
//!     "app_id".to_string(),
//!     "app_secret".to_string(),
//!     http_client
//! ));
//!
//! // Share across threads
//! let manager_clone = Arc::clone(&token_manager);
//! tokio::spawn(async move {
//!     let token = manager_clone.get_access_token().await.unwrap();
//!     // Use token...
//! });
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use crate::http::{AccessTokenResponse, WeChatHttpClient, WeChatResponse};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Access token with expiration information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
    /// The access token string
    pub token: String,
    /// When the token expires
    pub expires_at: DateTime<Utc>,
}

impl AccessToken {
    /// Creates a new access token with expiration time.
    pub fn new(token: String, expires_in_seconds: u64) -> Self {
        let expires_at = Utc::now() + Duration::seconds(expires_in_seconds as i64);
        Self { token, expires_at }
    }

    /// Checks if the token is expired or will expire within the buffer time.
    pub fn is_expired(&self, buffer_seconds: i64) -> bool {
        let buffer_time = Duration::seconds(buffer_seconds);
        Utc::now() + buffer_time >= self.expires_at
    }

    /// Gets the remaining time until expiration.
    pub fn time_until_expiry(&self) -> Duration {
        self.expires_at - Utc::now()
    }
}

/// Token manager responsible for obtaining and caching access tokens.
#[derive(Debug)]
pub struct TokenManager {
    app_id: String,
    app_secret: String,
    http_client: Arc<WeChatHttpClient>,
    token_cache: Arc<RwLock<Option<AccessToken>>>,
    refresh_lock: Arc<tokio::sync::Mutex<()>>,
}

impl TokenManager {
    /// Creates a new token manager.
    pub fn new(
        app_id: impl Into<String>,
        app_secret: impl Into<String>,
        http_client: Arc<WeChatHttpClient>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_secret: app_secret.into(),
            http_client,
            token_cache: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Gets a valid access token, refreshing if necessary.
    ///
    /// This method is thread-safe and will prevent concurrent token refreshes.
    pub async fn get_access_token(&self) -> Result<String> {
        // Check cache first (fast path)
        if let Some(token) = self.get_cached_token().await {
            return Ok(token);
        }

        // Slow path: need to refresh token
        self.refresh_token().await
    }

    /// Gets a cached token if it's still valid.
    async fn get_cached_token(&self) -> Option<String> {
        let cache = self.token_cache.read().await;
        if let Some(ref token) = *cache {
            // Use 60-second buffer to avoid edge cases
            if !token.is_expired(60) {
                return Some(token.token.clone());
            }
        }
        None
    }

    /// Refreshes the access token from WeChat API.
    async fn refresh_token(&self) -> Result<String> {
        // Prevent concurrent refreshes
        let _guard = self.refresh_lock.lock().await;

        // Double-check after acquiring lock
        if let Some(token) = self.get_cached_token().await {
            return Ok(token);
        }

        info!("Refreshing WeChat access token");

        // Make API call to get new token
        let url = format!(
            "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid={}&secret={}",
            self.app_id, self.app_secret
        );

        let response_bytes = self.http_client.download(&url).await?;

        let api_response: WeChatResponse<AccessTokenResponse> =
            serde_json::from_slice(&response_bytes)?;

        let token_response = api_response.into_result()?;

        // Create and cache the new token
        let new_token = AccessToken::new(token_response.access_token, token_response.expires_in);
        let token_string = new_token.token.clone();

        // Update cache
        {
            let mut cache = self.token_cache.write().await;
            *cache = Some(new_token);
        }

        info!("Successfully refreshed WeChat access token");
        Ok(token_string)
    }

    /// Forces a token refresh (useful for testing or when token is known to be invalid).
    pub async fn force_refresh(&self) -> Result<String> {
        // Clear cache first
        {
            let mut cache = self.token_cache.write().await;
            *cache = None;
        }

        self.refresh_token().await
    }

    /// Gets token information for debugging purposes.
    pub async fn get_token_info(&self) -> Option<TokenInfo> {
        let cache = self.token_cache.read().await;
        cache.as_ref().map(|token| TokenInfo {
            is_expired: token.is_expired(0),
            expires_at: token.expires_at,
            time_until_expiry: token.time_until_expiry(),
        })
    }

    /// Clears the token cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.token_cache.write().await;
        *cache = None;
    }
}

/// Token information for debugging and monitoring.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub is_expired: bool,
    pub expires_at: DateTime<Utc>,
    pub time_until_expiry: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_token_expiry() {
        // Create a token that expires in 1 hour
        let token = AccessToken::new("test_token".to_string(), 3600);

        // Should not be expired without buffer
        assert!(!token.is_expired(0));

        // Should not be expired with 30-minute buffer
        assert!(!token.is_expired(1800));

        // Should be considered expired with 2-hour buffer
        assert!(token.is_expired(7200));
    }

    #[test]
    fn test_access_token_time_until_expiry() {
        let token = AccessToken::new("test_token".to_string(), 3600);
        let time_until_expiry = token.time_until_expiry();

        // Should be approximately 1 hour (allowing for test execution time)
        assert!(time_until_expiry.num_seconds() > 3590);
        assert!(time_until_expiry.num_seconds() <= 3600);
    }

    #[tokio::test]
    async fn test_token_manager_creation() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let manager = TokenManager::new("test_app_id", "test_app_secret", http_client);

        assert_eq!(manager.app_id, "test_app_id");
        assert_eq!(manager.app_secret, "test_app_secret");

        // Cache should be empty initially
        let cache = manager.token_cache.read().await;
        assert!(cache.is_none());
    }

    #[tokio::test]
    async fn test_cached_token_retrieval() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let manager = TokenManager::new("test_app_id", "test_app_secret", http_client);

        // No cached token initially
        assert!(manager.get_cached_token().await.is_none());

        // Add a valid token to cache
        {
            let mut cache = manager.token_cache.write().await;
            *cache = Some(AccessToken::new("cached_token".to_string(), 3600));
        }

        // Should return cached token
        let cached = manager.get_cached_token().await;
        assert_eq!(cached, Some("cached_token".to_string()));

        // Clear cache
        manager.clear_cache().await;
        assert!(manager.get_cached_token().await.is_none());
    }

    #[tokio::test]
    async fn test_token_info() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let manager = TokenManager::new("test_app_id", "test_app_secret", http_client);

        // No token info initially
        assert!(manager.get_token_info().await.is_none());

        // Add a token
        {
            let mut cache = manager.token_cache.write().await;
            *cache = Some(AccessToken::new("test_token".to_string(), 3600));
        }

        // Should have token info
        let info = manager.get_token_info().await;
        assert!(info.is_some());

        let info = info.unwrap();
        assert!(!info.is_expired);
        assert!(info.time_until_expiry.num_seconds() > 3590);
    }
}
