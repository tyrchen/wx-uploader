//! Main WeChat client implementation.

use tracing::{debug, info};

use crate::auth::TokenManager;
use crate::error::{Result, WeChatError};
use crate::http::WeChatHttpClient;
use crate::markdown::{MarkdownContent, MarkdownParser};
use crate::mermaid::MermaidProcessor;
use crate::theme::ThemeManager;
use crate::upload::{Article, DraftInfo, DraftManager, ImageUploader};
use crate::utils;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Upload options for customizing the upload behavior.
#[derive(Debug, Clone)]
pub struct UploadOptions {
    /// Theme name to use for rendering
    pub theme: String,
    /// Custom title (overrides extracted title)
    pub title: Option<String>,
    /// Custom author (overrides extracted author)
    pub author: Option<String>,
    /// Path to cover image file
    pub cover_image: Option<String>,
    /// Whether to show cover image in content
    pub show_cover: bool,
    /// Whether to enable comments
    pub enable_comments: bool,
    /// Whether only fans can comment
    pub fans_only_comments: bool,
    /// Source URL for the article
    pub source_url: Option<String>,
}

impl Default for UploadOptions {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            title: None,
            author: None,
            cover_image: None,
            show_cover: true,
            enable_comments: false,
            fans_only_comments: false,
            source_url: None,
        }
    }
}

impl UploadOptions {
    /// Creates upload options with a specific theme.
    pub fn with_theme(theme: impl Into<String>) -> Self {
        Self {
            theme: theme.into(),
            ..Default::default()
        }
    }

    /// Sets the title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the author.
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Sets the cover image path.
    pub fn cover_image(mut self, path: impl Into<String>) -> Self {
        self.cover_image = Some(path.into());
        self
    }

    /// Sets whether to show the cover image in content.
    pub fn show_cover(mut self, show: bool) -> Self {
        self.show_cover = show;
        self
    }

    /// Sets comment options.
    pub fn comments(mut self, enable: bool, fans_only: bool) -> Self {
        self.enable_comments = enable;
        self.fans_only_comments = fans_only;
        self
    }

    /// Sets the source URL.
    pub fn source_url(mut self, url: impl Into<String>) -> Self {
        self.source_url = Some(url.into());
        self
    }
}

/// Main WeChat Official Account client.
#[derive(Debug)]
pub struct WeChatClient {
    http_client: Arc<WeChatHttpClient>,
    token_manager: Arc<TokenManager>,
    image_uploader: ImageUploader,
    draft_manager: DraftManager,
    markdown_parser: MarkdownParser,
    theme_manager: ThemeManager,
}

impl WeChatClient {
    /// Creates a new WeChat client with app credentials.
    pub async fn new(app_id: impl Into<String>, app_secret: impl Into<String>) -> Result<Self> {
        let app_id = app_id.into();
        let app_secret = app_secret.into();

        // Validate credentials format
        utils::validate_app_credentials(&app_id, &app_secret).map_err(WeChatError::config_error)?;

        // Create HTTP client
        let http_client = Arc::new(WeChatHttpClient::new()?);

        // Create token manager
        let token_manager = Arc::new(TokenManager::new(
            app_id,
            app_secret,
            Arc::clone(&http_client),
        ));

        // Create service components
        let image_uploader =
            ImageUploader::new(Arc::clone(&http_client), Arc::clone(&token_manager));

        let draft_manager = DraftManager::new(Arc::clone(&http_client), Arc::clone(&token_manager));

        let markdown_parser = MarkdownParser::new();
        let theme_manager = ThemeManager::new();

        Ok(Self {
            http_client,
            token_manager,
            image_uploader,
            draft_manager,
            markdown_parser,
            theme_manager,
        })
    }

    /// Uploads a markdown file as a WeChat draft article.
    ///
    /// This is the main convenience method that handles the entire workflow:
    /// 1. Parse markdown file
    /// 2. Extract and upload images
    /// 3. Replace image URLs in content
    /// 4. Render content with theme (from frontmatter, options, or default)
    /// 5. Create draft article
    ///
    /// # Arguments
    /// * `markdown_path` - Path to the markdown file
    ///
    /// # Returns
    /// Returns the media ID of the created draft
    pub async fn upload(&self, markdown_path: &str) -> Result<String> {
        let options = UploadOptions::default();
        self.upload_with_options(markdown_path, options).await
    }

    /// Uploads a markdown file with custom options.
    ///
    /// # Arguments
    /// * `markdown_path` - Path to the markdown file
    /// * `options` - Upload options for customization
    ///
    /// # Returns
    /// Returns the media ID of the created draft
    pub async fn upload_with_options(
        &self,
        markdown_path: &str,
        options: UploadOptions,
    ) -> Result<String> {
        let markdown_path = Path::new(markdown_path);

        // Validate input
        self.validate_upload_input(markdown_path, &options).await?;

        info!("Starting upload process for: {}", markdown_path.display());

        // Step 1: Parse markdown content
        let mut content = self.parse_markdown_file(markdown_path).await?;
        debug!("Found {} images in content", content.images.len());

        // Step 1.5: Process Mermaid charts
        let base_dir = utils::get_base_directory(markdown_path).unwrap_or_else(|| Path::new("."));
        let document_slug = MermaidProcessor::extract_slug_from_path(markdown_path);
        let mermaid_processor = MermaidProcessor::new(base_dir.to_path_buf(), document_slug);

        let (modified_content, mermaid_images) = mermaid_processor
            .process_mermaid_content_with_source_path(
                &content.content,
                base_dir,
                Some(markdown_path),
            )
            .await?;

        // Update content with Mermaid-processed version
        content.content = modified_content;

        // Add Mermaid-generated images to the image list
        content.images.extend(mermaid_images.clone());

        debug!(
            "Total images to upload (including Mermaid): {}",
            content.images.len()
        );

        // Step 2: Upload images concurrently
        let upload_results = self
            .image_uploader
            .upload_images(content.images.clone(), base_dir)
            .await?;
        info!("Completed uploading {} images", upload_results.len());

        // Step 3: Replace image URLs in content
        let url_mapping = self.draft_manager.create_url_mapping(&upload_results);
        content.replace_image_urls(&url_mapping)?;

        // Step 4: Upload cover image (from options or frontmatter)
        let cover_path = options
            .cover_image
            .as_ref()
            .or(content.cover.as_ref())
            .expect("Cover image should be available from validation");

        info!("Starting to upload cover image: {}", cover_path);
        let cover_media_id = Some(self.upload_cover_image(cover_path, base_dir).await?);
        info!("Completed uploading cover image");

        // Step 5: Render content with theme (from frontmatter, options, or default)
        let theme = content
            .theme
            .as_ref()
            .or(Some(&options.theme))
            .map(|t| t.as_str())
            .unwrap_or("default");

        // Validate theme exists
        if !self.theme_manager.has_theme(theme) {
            return Err(WeChatError::ThemeNotFound {
                theme: theme.to_string(),
            });
        }

        let html_content = self.render_content(&content, theme, &options)?;

        // Step 6: Create article and draft
        let article = self.create_article(&content, &options, html_content, cover_media_id);
        let draft_id = self.draft_manager.create_draft(vec![article]).await?;

        info!("Successfully created draft with ID: {draft_id}");
        Ok(draft_id)
    }

    /// Gets a draft by media ID.
    pub async fn get_draft(&self, media_id: &str) -> Result<DraftInfo> {
        self.draft_manager.get_draft(media_id).await
    }

    /// Updates an existing draft with new content.
    pub async fn update_draft(&self, media_id: &str, markdown_path: &str) -> Result<()> {
        let options = UploadOptions::default();
        self.update_draft_with_options(media_id, markdown_path, options)
            .await
    }

    /// Updates an existing draft with custom options.
    pub async fn update_draft_with_options(
        &self,
        media_id: &str,
        markdown_path: &str,
        options: UploadOptions,
    ) -> Result<()> {
        let markdown_path = Path::new(markdown_path);
        self.validate_upload_input(markdown_path, &options).await?;

        info!(
            "Updating draft {} with: {}",
            media_id,
            markdown_path.display()
        );

        // Parse and process content (same as upload)
        let mut content = self.parse_markdown_file(markdown_path).await?;
        let base_dir = utils::get_base_directory(markdown_path).unwrap_or_else(|| Path::new("."));

        // Process Mermaid charts
        let document_slug = MermaidProcessor::extract_slug_from_path(markdown_path);
        let mermaid_processor = MermaidProcessor::new(base_dir.to_path_buf(), document_slug);

        let (modified_content, mermaid_images) = mermaid_processor
            .process_mermaid_content_with_source_path(
                &content.content,
                base_dir,
                Some(markdown_path),
            )
            .await?;

        // Update content with Mermaid-processed version
        content.content = modified_content;

        // Add Mermaid-generated images to the image list
        content.images.extend(mermaid_images);

        let upload_results = self
            .image_uploader
            .upload_images(content.images.clone(), base_dir)
            .await?;

        let url_mapping = self.draft_manager.create_url_mapping(&upload_results);
        content.replace_image_urls(&url_mapping)?;

        let cover_path = options
            .cover_image
            .as_ref()
            .or(content.cover.as_ref())
            .expect("Cover image should be available from validation");

        let cover_media_id = Some(self.upload_cover_image(cover_path, base_dir).await?);

        let theme = content
            .theme
            .as_ref()
            .or(Some(&options.theme))
            .map(|t| t.as_str())
            .unwrap_or("default");

        // Validate theme exists
        if !self.theme_manager.has_theme(theme) {
            return Err(WeChatError::ThemeNotFound {
                theme: theme.to_string(),
            });
        }

        let html_content = self.render_content(&content, theme, &options)?;
        let article = self.create_article(&content, &options, html_content, cover_media_id);

        self.draft_manager
            .update_draft(media_id, vec![article])
            .await?;

        info!("Successfully updated draft: {media_id}");
        Ok(())
    }

    /// Deletes a draft by media ID.
    pub async fn delete_draft(&self, media_id: &str) -> Result<()> {
        self.draft_manager.delete_draft(media_id).await
    }

    /// Lists drafts with pagination.
    pub async fn list_drafts(&self, offset: u32, count: u32) -> Result<Vec<DraftInfo>> {
        self.draft_manager.list_drafts(offset, count).await
    }

    /// Uploads a single image file and returns the WeChat URL.
    pub async fn upload_image(&self, image_path: &str) -> Result<String> {
        let image_path = Path::new(image_path);

        if !utils::file_exists(image_path).await {
            return Err(WeChatError::FileNotFound {
                path: image_path.display().to_string(),
            });
        }

        if !utils::is_image_file(image_path) {
            return Err(WeChatError::config_error(
                "File is not a supported image format",
            ));
        }

        // Create a dummy image reference for uploading
        let image_ref = crate::markdown::ImageRef::new(
            "Uploaded image".to_string(),
            image_path.display().to_string(),
            (0, 0),
        );

        let base_dir = utils::get_base_directory(image_path).unwrap_or_else(|| Path::new("."));

        let results = self
            .image_uploader
            .upload_images(vec![image_ref], base_dir)
            .await?;

        Ok(results.into_iter().next().unwrap().url)
    }

    /// Creates a draft with custom articles.
    pub async fn create_draft(&self, articles: Vec<Article>) -> Result<String> {
        self.draft_manager.create_draft(articles).await
    }

    /// Gets the list of available themes.
    pub fn available_themes(&self) -> Vec<&String> {
        self.theme_manager.available_themes()
    }

    /// Checks if a theme exists.
    pub fn has_theme(&self, theme: &str) -> bool {
        self.theme_manager.has_theme(theme)
    }

    /// Gets access token information for debugging.
    pub async fn get_token_info(&self) -> Option<crate::auth::TokenInfo> {
        self.token_manager.get_token_info().await
    }

    /// Forces a token refresh.
    pub async fn refresh_token(&self) -> Result<String> {
        self.token_manager.force_refresh().await
    }

    /// Gets the underlying HTTP client for advanced usage.
    pub fn http_client(&self) -> &WeChatHttpClient {
        &self.http_client
    }

    // Private helper methods

    async fn validate_upload_input(
        &self,
        markdown_path: &Path,
        options: &UploadOptions,
    ) -> Result<()> {
        // Check if markdown file exists
        if !utils::file_exists(markdown_path).await {
            return Err(WeChatError::FileNotFound {
                path: markdown_path.display().to_string(),
            });
        }

        // Check if it's a markdown file
        if !utils::is_markdown_file(markdown_path) {
            return Err(WeChatError::config_error(
                "File is not a markdown file (.md or .markdown)",
            ));
        }

        // Theme validation will happen later when we determine the actual theme to use

        // Parse markdown to check for frontmatter cover
        let content = self.parse_markdown_file(markdown_path).await?;

        // Check that cover image is provided either via options or frontmatter
        let has_cover_option = options.cover_image.is_some();
        let has_cover_frontmatter = content.cover.is_some();

        if !has_cover_option && !has_cover_frontmatter {
            return Err(WeChatError::config_error(
                "Cover image is required. Please provide via --cover-image option or 'cover:' in frontmatter",
            ));
        }

        // Validate cover image from options if specified
        if let Some(cover_path) = &options.cover_image {
            let base_dir =
                utils::get_base_directory(markdown_path).unwrap_or_else(|| Path::new("."));

            let resolved_cover_path = if Path::new(cover_path).is_absolute() {
                PathBuf::from(cover_path)
            } else {
                base_dir.join(cover_path)
            };

            if !utils::file_exists(&resolved_cover_path).await {
                return Err(WeChatError::FileNotFound {
                    path: resolved_cover_path.display().to_string(),
                });
            }

            if !utils::is_image_file(&resolved_cover_path) {
                return Err(WeChatError::config_error(
                    "Cover file is not a supported image format",
                ));
            }
        }

        // Validate cover image from frontmatter if specified
        if let Some(cover_path) = &content.cover {
            let base_dir =
                utils::get_base_directory(markdown_path).unwrap_or_else(|| Path::new("."));

            let resolved_cover_path = if Path::new(cover_path).is_absolute() {
                PathBuf::from(cover_path)
            } else {
                base_dir.join(cover_path)
            };

            if !utils::file_exists(&resolved_cover_path).await {
                return Err(WeChatError::FileNotFound {
                    path: resolved_cover_path.display().to_string(),
                });
            }

            if !utils::is_image_file(&resolved_cover_path) {
                return Err(WeChatError::config_error(
                    "Cover file specified in frontmatter is not a supported image format",
                ));
            }
        }

        Ok(())
    }

    async fn parse_markdown_file(&self, path: &Path) -> Result<MarkdownContent> {
        self.markdown_parser.parse_file(path).await
    }

    async fn upload_cover_image(&self, cover_path: &str, base_dir: &Path) -> Result<String> {
        let cover_path = if Path::new(cover_path).is_absolute() {
            PathBuf::from(cover_path)
        } else {
            base_dir.join(cover_path)
        };

        // Upload cover image as permanent material
        self.image_uploader.upload_cover_material(&cover_path).await
    }

    fn render_content(
        &self,
        content: &MarkdownContent,
        theme: &str,
        options: &UploadOptions,
    ) -> Result<String> {
        let mut metadata = content.metadata.clone();

        // Use frontmatter values as defaults, override with options if provided
        if let Some(title) = content.title.as_ref() {
            metadata.insert("title".to_string(), title.clone());
        }
        if let Some(author) = content.author.as_ref() {
            metadata.insert("author".to_string(), author.clone());
        }

        // Override with options if provided
        if let Some(title) = &options.title {
            metadata.insert("title".to_string(), title.clone());
        }
        if let Some(author) = &options.author {
            metadata.insert("author".to_string(), author.clone());
        }

        self.theme_manager.render(
            &content.content,
            theme,
            content.code.as_deref().unwrap_or("vscode"),
            &metadata,
        )
    }

    fn create_article(
        &self,
        content: &MarkdownContent,
        options: &UploadOptions,
        html_content: String,
        cover_media_id: Option<String>,
    ) -> Article {
        // Determine title and author
        let title = options
            .title
            .clone()
            .or_else(|| content.title.clone())
            .unwrap_or_else(|| "Untitled".to_string());

        let author = options
            .author
            .clone()
            .or_else(|| content.author.clone())
            .unwrap_or_else(|| "Anonymous".to_string());

        // Use description from frontmatter if available, otherwise generate summary
        let digest = content
            .description
            .clone()
            .unwrap_or_else(|| content.get_summary(120));

        // Create article
        let mut article = Article::new(title, author, html_content)
            .with_digest(digest)
            .with_show_cover(options.show_cover)
            .with_comments(options.enable_comments, options.fans_only_comments);

        if let Some(media_id) = cover_media_id {
            article = article.with_cover_image(media_id);
        }

        if let Some(source_url) = &options.source_url {
            article = article.with_source_url(source_url.clone());
        }

        article
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upload_options_builder() {
        let options = UploadOptions::with_theme("github")
            .title("Test Title")
            .author("Test Author")
            .cover_image("cover.jpg")
            .show_cover(false)
            .comments(true, true)
            .source_url("https://example.com");

        assert_eq!(options.theme, "github");
        assert_eq!(options.title, Some("Test Title".to_string()));
        assert_eq!(options.author, Some("Test Author".to_string()));
        assert_eq!(options.cover_image, Some("cover.jpg".to_string()));
        assert!(!options.show_cover);
        assert!(options.enable_comments);
        assert!(options.fans_only_comments);
        assert_eq!(options.source_url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_upload_options_default() {
        let options = UploadOptions::default();

        assert_eq!(options.theme, "default");
        assert_eq!(options.title, None);
        assert_eq!(options.author, None);
        assert_eq!(options.cover_image, None);
        assert!(options.show_cover);
        assert!(!options.enable_comments);
        assert!(!options.fans_only_comments);
        assert_eq!(options.source_url, None);
    }

    #[tokio::test]
    async fn test_client_creation_with_invalid_credentials() {
        let result = WeChatClient::new("invalid", "12345678901234567890123456789012").await;
        assert!(result.is_err());

        let result = WeChatClient::new("wx1234567890123456", "short").await;
        assert!(result.is_err());

        let result = WeChatClient::new("", "").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_creation_with_valid_credentials() {
        let result =
            WeChatClient::new("wx1234567890123456", "12345678901234567890123456789012").await;
        assert!(result.is_ok());

        let client = result.unwrap();
        assert!(client.available_themes().len() >= 4);
        assert!(client.has_theme("default"));
        assert!(client.has_theme("lapis"));
        assert!(client.has_theme("maize"));
        assert!(client.has_theme("orangeheart"));
    }

    #[tokio::test]
    async fn test_cover_requirement_validation() {
        use tempfile::Builder;

        let client = WeChatClient::new("wx1234567890123456", "12345678901234567890123456789012")
            .await
            .unwrap();

        // Test 1: Markdown without cover in frontmatter or options should fail
        let temp_file = Builder::new().suffix(".md").tempfile().unwrap();
        let markdown_without_cover = r#"---
title: Test Article
author: Test Author
---

# Content
Some article content here.
"#;
        tokio::fs::write(temp_file.path(), markdown_without_cover)
            .await
            .unwrap();

        let options = UploadOptions::with_theme("default");
        let result = client
            .validate_upload_input(temp_file.path(), &options)
            .await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Cover image is required"));

        // Test 2: Markdown with cover in frontmatter should work (if file exists)
        let temp_file2 = Builder::new().suffix(".md").tempfile().unwrap();
        let markdown_with_cover = r#"---
title: Test Article
author: Test Author
cover: ../fixtures/images/02-cover.png
---

# Content
Some article content here.
"#;
        tokio::fs::write(temp_file2.path(), markdown_with_cover)
            .await
            .unwrap();

        let options2 = UploadOptions::with_theme("default");
        let result2 = client
            .validate_upload_input(temp_file2.path(), &options2)
            .await;
        // This will fail because the cover file doesn't exist, but it should fail with file not found, not cover required
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("02-cover.png"));

        // Test 3: Options with cover should work (if file exists)
        let temp_file3 = Builder::new().suffix(".md").tempfile().unwrap();
        let markdown_no_frontmatter_cover = r#"---
title: Test Article
author: Test Author
---

# Content
Some article content here.
"#;
        tokio::fs::write(temp_file3.path(), markdown_no_frontmatter_cover)
            .await
            .unwrap();

        let options3 =
            UploadOptions::with_theme("default").cover_image("../fixtures/images/02-cover.png");
        let result3 = client
            .validate_upload_input(temp_file3.path(), &options3)
            .await;
        // This will fail because the cover file doesn't exist, but should not be the "cover required" error
        assert!(result3.is_err());
        assert!(result3.unwrap_err().to_string().contains("02-cover.png"));
    }

    #[tokio::test]
    async fn test_fixture_file_parsing() {
        let client = WeChatClient::new("wx1234567890123456", "12345678901234567890123456789012")
            .await
            .unwrap();

        // Parse the fixture file to verify it has the expected frontmatter
        let content = client
            .parse_markdown_file(std::path::Path::new("fixtures/example.md"))
            .await
            .unwrap();

        assert_eq!(content.author, Some("陈小天".to_string()));
        assert_eq!(
            content.description,
            Some("为了这壶醋，我包了这顿饺子（写了几千行 Rust，做了个工具）".to_string())
        );
        assert_eq!(content.cover, Some("images/02-cover.png".to_string()));

        // Verify that validation works with the fixture (should pass because cover exists in frontmatter and file exists)
        let options = UploadOptions::with_theme("default");
        let result = client
            .validate_upload_input(std::path::Path::new("fixtures/example.md"), &options)
            .await;
        assert!(
            result.is_ok(),
            "Validation should pass for fixture file with cover in frontmatter"
        );
    }
}
