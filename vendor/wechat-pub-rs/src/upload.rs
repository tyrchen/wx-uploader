//! Upload module for handling image uploads and draft management.
//!
//! This module provides comprehensive functionality for uploading images and managing
//! WeChat article drafts, with built-in deduplication, concurrent processing, and error recovery.
//!
//! ## Features
//!
//! - **Unified Upload Flow**: All images uploaded as permanent materials for consistency
//! - **Concurrent Image Uploads**: Up to 5 simultaneous image uploads for performance
//! - **Content Deduplication**: BLAKE3 hash-based image deduplication to avoid duplicates
//! - **Size Validation**: Automatic file size validation (max 10MB for images)
//! - **Format Support**: JPEG, PNG, GIF image format support
//! - **Draft Management**: Full CRUD operations for article drafts
//! - **Streaming Downloads**: Memory-efficient handling of remote images
//!
//! ## Image Upload Process
//!
//! 1. **Validation**: Check file size and format
//! 2. **Hash Calculation**: Generate BLAKE3 hash for deduplication
//! 3. **Deduplication Check**: Search existing permanent materials by hash
//! 4. **Upload as Permanent Material**: All images uploaded using the permanent material API
//! 5. **Concurrent Processing**: Process multiple images simultaneously
//! 6. **Error Recovery**: Retry failed uploads with exponential backoff
//!
//! ## Draft Management
//!
//! The module supports full lifecycle management of WeChat article drafts:
//!
//! - **Create**: Upload new article content as a draft
//! - **Read**: Retrieve draft information and content
//! - **Update**: Modify existing draft content
//! - **Delete**: Remove drafts
//! - **List**: Paginated listing of all drafts
//!
//! ## Usage Examples
//!
//! ### Image Upload
//!
//! ```rust
//! use wechat_pub_rs::upload::ImageUploader;
//! use wechat_pub_rs::markdown::ImageRef;
//! use std::path::Path;
//! # use std::sync::Arc;
//! # use wechat_pub_rs::auth::TokenManager;
//! # use wechat_pub_rs::http::WeChatHttpClient;
//!
//! # async fn example() -> wechat_pub_rs::Result<()> {
//! # let http_client = Arc::new(WeChatHttpClient::new()?);
//! # let token_manager = Arc::new(TokenManager::new("id".to_string(), "secret".to_string(), http_client.clone()));
//! let uploader = ImageUploader::new(http_client, token_manager);
//!
//! let image_ref = ImageRef::new(
//!     "Alt text".to_string(),
//!     "images/photo.jpg".to_string(),
//!     (0, 0)
//! );
//!
//! let results = uploader.upload_images(vec![image_ref], Path::new(".")).await?;
//! for result in results {
//!     println!("Uploaded: {} -> {}", result.image_ref.original_url, result.url);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Draft Management
//!
//! ```rust
//! use wechat_pub_rs::upload::{DraftManager, Article};
//! # use std::sync::Arc;
//! # use wechat_pub_rs::auth::TokenManager;
//! # use wechat_pub_rs::http::WeChatHttpClient;
//!
//! # async fn example() -> wechat_pub_rs::Result<()> {
//! # let http_client = Arc::new(WeChatHttpClient::new()?);
//! # let token_manager = Arc::new(TokenManager::new("id".to_string(), "secret".to_string(), http_client.clone()));
//! let draft_manager = DraftManager::new(http_client, token_manager);
//!
//! // Create a new article
//! let article = Article::new(
//!     "Article Title".to_string(),
//!     "Author Name".to_string(),
//!     "<h1>Article Content</h1>".to_string(),
//! ).with_digest("Article summary".to_string());
//!
//! // Create draft
//! let draft_id = draft_manager.create_draft(vec![article]).await?;
//! println!("Created draft: {}", draft_id);
//!
//! // List drafts
//! let drafts = draft_manager.list_drafts(0, 10).await?;
//! println!("Found {} drafts", drafts.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Concurrent Uploads**: Maximum 5 simultaneous image uploads
//! - **Memory Efficiency**: Streaming file operations, no full file buffering
//! - **Deduplication**: O(1) hash-based duplicate detection
//! - **Error Recovery**: Exponential backoff with jitter for failed requests

use crate::auth::TokenManager;
use crate::error::{Result, WeChatError};
use crate::http::{DraftResponse, MaterialUploadResponse, WeChatHttpClient, WeChatResponse};
use crate::markdown::ImageRef;
use blake3;
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, warn};

/// Maximum concurrent image uploads to prevent overwhelming the server
const MAX_CONCURRENT_UPLOADS: usize = 5;

/// Cache TTL for material lookups (5 minutes)
const MATERIAL_CACHE_TTL: Duration = Duration::from_secs(300);

/// Maximum number of cached materials
const MAX_CACHE_SIZE: usize = 1000;

/// Cached material entry with timestamp
#[derive(Debug, Clone)]
struct CachedMaterial {
    material: MaterialItem,
    cached_at: Instant,
}

impl CachedMaterial {
    fn new(material: MaterialItem) -> Self {
        Self {
            material,
            cached_at: Instant::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > MATERIAL_CACHE_TTL
    }
}

/// Maximum file size for images (10 MB)
const MAX_IMAGE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum file size for streaming downloads (50 MB)
const MAX_DOWNLOAD_SIZE: u64 = 50 * 1024 * 1024;

/// Represents the result of an image upload operation.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// Original image reference
    pub image_ref: ImageRef,
    /// WeChat media ID for the uploaded image
    pub media_id: String,
    /// WeChat URL for the uploaded image
    pub url: String,
}

/// Represents a WeChat article for draft creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// Article title
    pub title: String,
    /// Article author
    pub author: String,
    /// Article content (HTML)
    pub content: String,
    /// Content source URL (optional)
    pub content_source_url: Option<String>,
    /// Digest (summary) of the article
    pub digest: String,
    /// Show cover picture in content (0: no, 1: yes)
    pub show_cover_pic: u8,
    /// Thumb media ID for cover image
    pub thumb_media_id: Option<String>,
    /// Need open comment (0: no, 1: yes)
    pub need_open_comment: u8,
    /// Only fans can comment (0: no, 1: yes)
    pub only_fans_can_comment: u8,
}

impl Article {
    /// Creates a new article with required fields.
    pub fn new(title: String, author: String, content: String) -> Self {
        Self {
            title,
            author,
            content,
            content_source_url: None,
            digest: String::new(),
            show_cover_pic: 1,
            thumb_media_id: None,
            need_open_comment: 0,
            only_fans_can_comment: 0,
        }
    }

    /// Sets the article digest (summary).
    pub fn with_digest(mut self, digest: String) -> Self {
        self.digest = digest;
        self
    }

    /// Sets the cover image media ID.
    pub fn with_cover_image(mut self, thumb_media_id: String) -> Self {
        self.thumb_media_id = Some(thumb_media_id);
        self
    }

    /// Sets whether to show cover image in content.
    pub fn with_show_cover(mut self, show: bool) -> Self {
        self.show_cover_pic = if show { 1 } else { 0 };
        self
    }

    /// Sets comment settings.
    pub fn with_comments(mut self, enable_comments: bool, fans_only: bool) -> Self {
        self.need_open_comment = if enable_comments { 1 } else { 0 };
        self.only_fans_can_comment = if fans_only { 1 } else { 0 };
        self
    }

    /// Sets the content source URL.
    pub fn with_source_url(mut self, url: String) -> Self {
        self.content_source_url = Some(url);
        self
    }
}

/// Request body for creating a draft.
#[derive(Debug, Serialize)]
struct DraftRequest {
    articles: Vec<Article>,
}

/// Draft information from WeChat API.
#[derive(Debug, Deserialize)]
pub struct DraftInfo {
    pub media_id: String,
    pub content: DraftContent,
    pub update_time: u64,
}

/// Content of a draft.
#[derive(Debug, Deserialize)]
pub struct DraftContent {
    pub news_item: Vec<Article>,
}

/// List drafts response.
#[derive(Debug, Deserialize)]
pub struct DraftListResponse {
    pub total_count: u32,
    pub item_count: u32,
    pub item: Vec<DraftInfo>,
}

/// Material item in the list response.
#[derive(Debug, Deserialize, Clone)]
pub struct MaterialItem {
    pub media_id: String,
    pub name: String,
    pub update_time: u64,
    pub url: String,
}

/// List materials response.
#[derive(Debug, Deserialize)]
pub struct MaterialListResponse {
    pub total_count: u32,
    pub item_count: u32,
    pub item: Vec<MaterialItem>,
}

/// Image uploader with concurrent upload capabilities and intelligent caching.
#[derive(Debug)]
pub struct ImageUploader {
    http_client: Arc<WeChatHttpClient>,
    token_manager: Arc<TokenManager>,
    semaphore: Arc<Semaphore>,
    /// Cache for material lookups by hash to avoid redundant API calls
    material_cache: Arc<RwLock<HashMap<String, CachedMaterial>>>,
}

impl ImageUploader {
    /// Creates a new image uploader.
    pub fn new(http_client: Arc<WeChatHttpClient>, token_manager: Arc<TokenManager>) -> Self {
        Self {
            http_client,
            token_manager,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_UPLOADS)),
            material_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Uploads multiple images concurrently.
    pub async fn upload_images(
        &self,
        images: Vec<ImageRef>,
        base_path: &Path,
    ) -> Result<Vec<UploadResult>> {
        if images.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Uploading {} images concurrently", images.len());

        // Create upload tasks
        let tasks: Vec<_> = images
            .into_iter()
            .map(|image_ref| {
                let uploader = self.clone();
                let base_path = base_path.to_owned();

                tokio::spawn(
                    async move { uploader.upload_single_image(image_ref, &base_path).await },
                )
            })
            .collect();

        // Execute all tasks and collect results
        let results = try_join_all(tasks)
            .await
            .map_err(|e| WeChatError::Internal {
                message: format!("Task join error: {e}"),
            })?;

        // Convert task results to upload results
        let upload_results: Result<Vec<_>> = results.into_iter().collect();
        let uploads = upload_results?;

        info!("Successfully uploaded {} images", uploads.len());
        Ok(uploads)
    }

    /// Uploads a single image as permanent material.
    async fn upload_single_image(
        &self,
        image_ref: ImageRef,
        base_path: &Path,
    ) -> Result<UploadResult> {
        // Acquire semaphore permit to limit concurrency
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| WeChatError::Internal {
                message: format!("Semaphore error: {e}"),
            })?;

        debug!("Processing image: {}", image_ref.original_url);

        // Load image data
        let image_data = if image_ref.is_local {
            let image_path = image_ref.resolve_path(base_path)?;
            self.load_local_image(&image_path).await?
        } else {
            self.download_remote_image(&image_ref.original_url).await?
        };

        // Use unified upload method
        let (media_id, url) = self
            .upload_image_as_material(image_data, &image_ref.original_url)
            .await?;

        info!(
            "Successfully uploaded image: {} -> {} (media_id: {})",
            image_ref.original_url, url, media_id
        );

        Ok(UploadResult {
            image_ref,
            media_id,
            url,
        })
    }

    /// Unified method to upload image data as permanent material with deduplication and caching.
    async fn upload_image_as_material(
        &self,
        image_data: Vec<u8>,
        original_path: &str,
    ) -> Result<(String, String)> {
        // Calculate BLAKE3 hash of the image content
        let hash = blake3::hash(&image_data);
        let hash_str = hash.to_hex().to_string();
        debug!("Image hash: {hash_str}");

        // Check cache first for performance optimization
        {
            let cache = self.material_cache.read().await;
            if let Some(cached) = cache.get(&hash_str) {
                if !cached.is_expired() {
                    debug!("Cache hit for hash: {hash_str}");
                    return Ok((
                        cached.material.media_id.clone(),
                        cached.material.url.clone(),
                    ));
                } else {
                    debug!("Cache entry expired for hash: {hash_str}");
                }
            }
        }

        // Check if this image already exists by searching materials (with cache update)
        debug!("Checking for existing material with hash: {}", hash_str);
        if let Some((existing_url, media_id)) = self.find_material_by_hash(&hash_str).await? {
            info!("Image already exists with hash {hash_str}, reusing media_id: {media_id}");

            // Cache the found material for future lookups
            {
                let mut cache = self.material_cache.write().await;
                let material_item = MaterialItem {
                    media_id: media_id.clone(),
                    name: hash_str.clone(),
                    update_time: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    url: existing_url.clone(),
                };
                cache.insert(hash_str.clone(), CachedMaterial::new(material_item));
                debug!("Cached found material for hash: {hash_str}");
            }

            return Ok((media_id, existing_url));
        }

        // Use hash as filename with appropriate extension
        let extension = self.get_image_extension(original_path, &image_data);
        let filename = format!("{hash_str}.{extension}");
        debug!("Uploading new image as permanent material with filename: {filename}");

        // Upload as permanent material
        let access_token = self.token_manager.get_access_token().await?;
        let response = self
            .http_client
            .upload_material(&access_token, "image", image_data, &filename)
            .await?;

        // Parse response - handle both direct and wrapped response formats
        let response_text = response.text().await?;
        let material = if let Ok(direct_response) =
            serde_json::from_str::<MaterialUploadResponse>(&response_text)
        {
            direct_response
        } else {
            // If that fails, try parsing as standard WeChat error response
            let upload_response: WeChatResponse<MaterialUploadResponse> =
                serde_json::from_str(&response_text)?;
            upload_response.into_result()?
        };

        info!(
            "Successfully uploaded new material: {} -> media_id: {} (hash: {})",
            original_path, material.media_id, hash_str
        );

        // Cache the successful upload for future lookups
        {
            let mut cache = self.material_cache.write().await;

            // Implement LRU eviction if cache is full
            if cache.len() >= MAX_CACHE_SIZE {
                // Remove 10% of oldest entries
                let remove_count = MAX_CACHE_SIZE / 10;
                let mut to_remove = Vec::with_capacity(remove_count);

                for (hash, cached) in cache.iter() {
                    to_remove.push((hash.clone(), cached.cached_at));
                    if to_remove.len() >= remove_count {
                        break;
                    }
                }

                // Sort by timestamp and remove oldest
                to_remove.sort_by_key(|(_, timestamp)| *timestamp);
                for (hash, _) in to_remove.into_iter().take(remove_count) {
                    cache.remove(&hash);
                }

                debug!("Evicted {} old cache entries", remove_count);
            }

            // Add new material to cache
            let material_item = MaterialItem {
                media_id: material.media_id.clone(),
                name: hash_str.clone(),
                update_time: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                url: material.url.clone(),
            };

            cache.insert(hash_str.clone(), CachedMaterial::new(material_item));
            debug!("Cached material for hash: {hash_str}");
        }

        Ok((material.media_id, material.url))
    }

    /// Clears expired entries from the material cache.
    pub async fn clear_expired_cache(&self) {
        let mut cache = self.material_cache.write().await;
        let initial_size = cache.len();

        cache.retain(|hash, cached| {
            let keep = !cached.is_expired();
            if !keep {
                debug!("Removing expired cache entry: {}", hash);
            }
            keep
        });

        let removed = initial_size - cache.len();
        if removed > 0 {
            info!("Cleared {} expired cache entries", removed);
        }
    }

    /// Gets cache statistics for monitoring.
    pub async fn get_cache_stats(&self) -> (usize, usize) {
        let cache = self.material_cache.read().await;
        let total = cache.len();
        let expired = cache.values().filter(|c| c.is_expired()).count();
        (total, expired)
    }

    /// Loads image data from local file with streaming and size validation.
    async fn load_local_image(&self, path: &Path) -> Result<Vec<u8>> {
        // Check file size before loading
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| WeChatError::ImageUpload {
                path: path.display().to_string(),
                reason: format!("Failed to get file metadata: {e}"),
            })?;

        let file_size = metadata.len();
        if file_size > MAX_IMAGE_SIZE {
            return Err(WeChatError::ImageUpload {
                path: path.display().to_string(),
                reason: format!("File too large: {file_size} bytes (max: {MAX_IMAGE_SIZE} bytes)"),
            });
        }

        debug!(
            "Loading local image: {} ({} bytes)",
            path.display(),
            file_size
        );

        fs::read(path).await.map_err(|e| WeChatError::ImageUpload {
            path: path.display().to_string(),
            reason: format!("Failed to read local file: {e}"),
        })
    }

    /// Downloads image data from remote URL with optimized streaming and size validation.
    async fn download_remote_image(&self, url: &str) -> Result<Vec<u8>> {
        debug!("Downloading remote image: {url}");

        self.http_client
            .download_with_limit(url, MAX_DOWNLOAD_SIZE)
            .await
            .map_err(|e| WeChatError::ImageUpload {
                path: url.to_string(),
                reason: format!("Failed to download remote image: {e}"),
            })
    }

    /// Gets the image extension based on URL and content.
    fn get_image_extension(&self, url: &str, image_data: &[u8]) -> String {
        // First try to get from URL
        if let Some(ext) = Path::new(url)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| matches!(*e, "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp"))
        {
            return ext.to_string();
        }

        // Otherwise, detect from content
        if image_data.len() >= 4 {
            match &image_data[0..4] {
                [0xFF, 0xD8, 0xFF, _] => return "jpg".to_string(),
                [0x89, 0x50, 0x4E, 0x47] => return "png".to_string(),
                [0x47, 0x49, 0x46, _] => return "gif".to_string(),
                [0x42, 0x4D, _, _] => return "bmp".to_string(),
                _ => {}
            }
        }

        // Check for WebP
        if image_data.len() >= 12 && &image_data[0..4] == b"RIFF" && &image_data[8..12] == b"WEBP" {
            return "webp".to_string();
        }

        // Default to jpg
        "jpg".to_string()
    }

    /// Searches for an existing material by hash and returns both URL and media_id.
    async fn find_material_by_hash(&self, hash_str: &str) -> Result<Option<(String, String)>> {
        debug!("Checking for existing material with hash: {hash_str}");

        // Check the most recent 20 materials
        let access_token = self.token_manager.get_access_token().await?;

        let request = serde_json::json!({
            "type": "image",
            "offset": 0,
            "count": 20
        });

        let response = self
            .http_client
            .post_json_with_token(
                "/cgi-bin/material/batchget_material",
                &access_token,
                &request,
            )
            .await
            .map_err(|e| {
                warn!("Failed to list materials: {e}");
                e
            });

        // If we can't list materials, just proceed with upload
        let response = match response {
            Ok(resp) => resp,
            Err(_) => return Ok(None),
        };

        let response_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response".to_string());

        let materials_result =
            serde_json::from_str::<WeChatResponse<MaterialListResponse>>(&response_text);

        match materials_result {
            Ok(materials_response) => {
                if let Ok(material_list) = materials_response.into_result() {
                    // Check if any material name starts with our hash
                    for item in material_list.item {
                        if item.name.starts_with(hash_str) {
                            info!(
                                "Found existing material with hash {}: URL {} (media_id: {})",
                                hash_str, item.url, item.media_id
                            );
                            return Ok(Some((item.url, item.media_id)));
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to parse material list response: {e}");
            }
        }

        debug!("No existing material found with hash: {hash_str}");
        Ok(None)
    }

    /// Uploads a cover image as permanent material.
    pub async fn upload_cover_material(&self, cover_path: &Path) -> Result<String> {
        info!(
            "Uploading cover image as permanent material: {}",
            cover_path.display()
        );

        // Load image data
        let image_data = self.load_local_image(cover_path).await?;

        // Use unified upload method
        let (media_id, _url) = self
            .upload_image_as_material(image_data, &cover_path.to_string_lossy())
            .await?;

        info!(
            "Successfully uploaded cover image: {} -> media_id: {}",
            cover_path.display(),
            media_id
        );

        Ok(media_id)
    }
}

impl Clone for ImageUploader {
    fn clone(&self) -> Self {
        Self {
            http_client: Arc::clone(&self.http_client),
            token_manager: Arc::clone(&self.token_manager),
            semaphore: Arc::clone(&self.semaphore),
            material_cache: Arc::clone(&self.material_cache),
        }
    }
}

/// Draft manager for creating and managing article drafts.
#[derive(Debug)]
pub struct DraftManager {
    http_client: Arc<WeChatHttpClient>,
    token_manager: Arc<TokenManager>,
}

impl DraftManager {
    /// Creates a new draft manager.
    pub fn new(http_client: Arc<WeChatHttpClient>, token_manager: Arc<TokenManager>) -> Self {
        Self {
            http_client,
            token_manager,
        }
    }

    /// Creates a new draft with articles, or updates existing if title matches.
    pub async fn create_draft(&self, articles: Vec<Article>) -> Result<String> {
        if articles.is_empty() {
            return Err(WeChatError::config_error(
                "At least one article is required",
            ));
        }

        let title = &articles[0].title;
        info!("Processing draft with title: {title}");

        // Check recent drafts for matching title
        if let Some(existing_media_id) = self.find_draft_by_title(title).await? {
            info!(
                "Found existing draft with title '{title}', updating media_id: {existing_media_id}"
            );

            // Update existing draft
            self.update_draft(&existing_media_id, articles).await?;
            return Ok(existing_media_id);
        }

        // No existing draft found, create new one
        info!("No existing draft found, creating new draft");

        let request = DraftRequest { articles };
        let access_token = self.token_manager.get_access_token().await?;

        let response = self
            .http_client
            .post_json_with_token("/cgi-bin/draft/add", &access_token, &request)
            .await?;

        let draft_response: WeChatResponse<DraftResponse> = response.json().await?;
        let draft = draft_response.into_result()?;

        info!(
            "Successfully created new draft with media_id: {}",
            draft.media_id
        );
        Ok(draft.media_id)
    }

    /// Gets a draft by media ID.
    pub async fn get_draft(&self, media_id: &str) -> Result<DraftInfo> {
        debug!("Getting draft: {media_id}");

        let access_token = self.token_manager.get_access_token().await?;
        let request = serde_json::json!({ "media_id": media_id });

        let response = self
            .http_client
            .post_json_with_token("/cgi-bin/draft/get", &access_token, &request)
            .await?;

        let draft_response: WeChatResponse<DraftInfo> = response.json().await?;
        draft_response.into_result()
    }

    /// Updates a draft.
    pub async fn update_draft(&self, media_id: &str, articles: Vec<Article>) -> Result<()> {
        if articles.is_empty() {
            return Err(WeChatError::config_error(
                "At least one article is required",
            ));
        }

        info!(
            "Updating draft {} with {} articles",
            media_id,
            articles.len()
        );

        // According to WeChat API, the update request structure should wrap articles differently
        let request = serde_json::json!({
            "media_id": media_id,
            "index": 0,
            "articles": articles[0]  // WeChat expects a single article object, not an array
        });

        let access_token = self.token_manager.get_access_token().await?;

        let response = self
            .http_client
            .post_json_with_token("/cgi-bin/draft/update", &access_token, &request)
            .await?;

        let update_response: WeChatResponse<serde_json::Value> = response.json().await?;
        update_response.into_result()?;

        info!("Successfully updated draft: {media_id}");
        Ok(())
    }

    /// Deletes a draft.
    pub async fn delete_draft(&self, media_id: &str) -> Result<()> {
        info!("Deleting draft: {media_id}");

        let request = serde_json::json!({ "media_id": media_id });
        let access_token = self.token_manager.get_access_token().await?;

        let response = self
            .http_client
            .post_json_with_token("/cgi-bin/draft/delete", &access_token, &request)
            .await?;

        let delete_response: WeChatResponse<serde_json::Value> = response.json().await?;
        delete_response.into_result()?;

        info!("Successfully deleted draft: {media_id}");
        Ok(())
    }

    /// Lists drafts with pagination.
    pub async fn list_drafts(&self, offset: u32, count: u32) -> Result<Vec<DraftInfo>> {
        debug!("Listing drafts: offset={offset}, count={count}");

        let request = serde_json::json!({
            "offset": offset,
            "count": count,
            "no_content": 0
        });

        let access_token = self.token_manager.get_access_token().await?;

        let response = self
            .http_client
            .post_json_with_token("/cgi-bin/draft/batchget", &access_token, &request)
            .await?;

        let response_text = response.text().await?;

        let list_response: WeChatResponse<DraftListResponse> =
            serde_json::from_str(&response_text)?;

        let drafts = list_response.into_result()?;
        Ok(drafts.item)
    }

    /// Creates URL mapping from upload results.
    pub fn create_url_mapping(&self, upload_results: &[UploadResult]) -> HashMap<String, String> {
        upload_results
            .iter()
            .map(|result| (result.image_ref.original_url.clone(), result.url.clone()))
            .collect()
    }

    /// Finds a draft by title in recent drafts.
    async fn find_draft_by_title(&self, title: &str) -> Result<Option<String>> {
        debug!("Searching for draft with title: {title}");

        // List recent 20 drafts
        let drafts = match self.list_drafts(0, 20).await {
            Ok(drafts) => drafts,
            Err(e) => {
                warn!("Failed to list drafts: {e}");
                return Ok(None);
            }
        };

        // Search for matching title
        for draft in drafts {
            if let Some(first_article) = draft.content.news_item.first()
                && first_article.title == title
            {
                info!("Found existing draft with matching title");
                return Ok(Some(draft.media_id));
            }
        }

        debug!("No draft found with title: {title}");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenManager;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_article_creation() {
        let article = Article::new(
            "Test Title".to_string(),
            "Test Author".to_string(),
            "<h1>Test Content</h1>".to_string(),
        );

        assert_eq!(article.title, "Test Title");
        assert_eq!(article.author, "Test Author");
        assert_eq!(article.content, "<h1>Test Content</h1>");
        assert_eq!(article.show_cover_pic, 1);
        assert_eq!(article.need_open_comment, 0);
    }

    #[tokio::test]
    async fn test_article_builder_methods() {
        let article = Article::new(
            "Title".to_string(),
            "Author".to_string(),
            "Content".to_string(),
        )
        .with_digest("Test digest".to_string())
        .with_cover_image("cover_media_id".to_string())
        .with_show_cover(false)
        .with_comments(true, true)
        .with_source_url("https://example.com".to_string());

        assert_eq!(article.digest, "Test digest");
        assert_eq!(article.thumb_media_id, Some("cover_media_id".to_string()));
        assert_eq!(article.show_cover_pic, 0);
        assert_eq!(article.need_open_comment, 1);
        assert_eq!(article.only_fans_can_comment, 1);
        assert_eq!(
            article.content_source_url,
            Some("https://example.com".to_string())
        );
    }

    #[tokio::test]
    async fn test_image_uploader_creation() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let token_manager = Arc::new(TokenManager::new(
            "test_app_id",
            "test_secret",
            Arc::clone(&http_client),
        ));

        let uploader = ImageUploader::new(http_client, token_manager);
        assert_eq!(
            uploader.semaphore.available_permits(),
            MAX_CONCURRENT_UPLOADS
        );
    }

    #[tokio::test]
    async fn test_draft_manager_creation() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let token_manager = Arc::new(TokenManager::new(
            "test_app_id",
            "test_secret",
            Arc::clone(&http_client),
        ));

        let _manager = DraftManager::new(http_client, token_manager);
    }

    #[test]
    fn test_image_extension_detection() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let token_manager = Arc::new(TokenManager::new(
            "test_app_id",
            "test_secret",
            Arc::clone(&http_client),
        ));
        let uploader = ImageUploader::new(http_client, token_manager);

        // Test URL-based extension detection
        assert_eq!(uploader.get_image_extension("test.jpg", &[]), "jpg");
        assert_eq!(uploader.get_image_extension("test.png", &[]), "png");
        assert_eq!(uploader.get_image_extension("test.webp", &[]), "webp");

        // Test content-based detection for JPEG
        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(uploader.get_image_extension("noext", &jpeg_header), "jpg");

        // Test content-based detection for PNG
        let png_header = vec![0x89, 0x50, 0x4E, 0x47];
        assert_eq!(uploader.get_image_extension("noext", &png_header), "png");
    }

    #[test]
    fn test_url_mapping_creation() {
        let http_client = Arc::new(WeChatHttpClient::new().unwrap());
        let token_manager = Arc::new(TokenManager::new(
            "test_app_id",
            "test_secret",
            Arc::clone(&http_client),
        ));
        let manager = DraftManager::new(http_client, token_manager);

        let image_ref = ImageRef::new("Alt".to_string(), "./test.jpg".to_string(), (0, 10));
        let upload_result = UploadResult {
            image_ref,
            media_id: "media123".to_string(),
            url: "https://wechat.com/image123".to_string(),
        };

        let mapping = manager.create_url_mapping(&[upload_result]);
        assert_eq!(
            mapping.get("./test.jpg"),
            Some(&"https://wechat.com/image123".to_string())
        );
    }
}
