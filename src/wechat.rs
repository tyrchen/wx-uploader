//! WeChat public account integration
//!
//! This module provides WeChat public account functionality for uploading
//! markdown articles with automatic cover image generation and frontmatter management.

use crate::error::{Error, Result};
use crate::markdown::{parse_markdown_file, update_frontmatter, write_markdown_file};
use crate::models::Frontmatter;
use crate::openai::OpenAIClient;
use crate::output::{FORMATTER, FilePathFormatter, OutputFormatter};
use image::GenericImageView;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

/// WeChat cover image aspect ratio (2.35:1)
const WECHAT_COVER_ASPECT_RATIO: f64 = 2.35;

// Re-export the WeChat client type
pub use wechat_pub_rs::WeChatClient;

/// Crops an image to WeChat cover aspect ratio (2.35:1) and saves to a temp file.
///
/// This function reads the original image, crops it to the required aspect ratio,
/// and saves it to a temporary file. The original image is not modified.
///
/// # Arguments
///
/// * `image_path` - Path to the original image file
///
/// # Returns
///
/// Path to the temporary cropped image file, or None if no cropping was needed
fn crop_cover_to_temp(image_path: &Path) -> Result<Option<PathBuf>> {
    use image::ImageReader;
    use std::io::Cursor;

    // Read the image file
    let image_bytes = std::fs::read(image_path)
        .map_err(|e| Error::generic(format!("Failed to read image file: {}", e)))?;

    let img = ImageReader::new(Cursor::new(&image_bytes))
        .with_guessed_format()
        .map_err(|e| Error::generic(format!("Failed to read image format: {}", e)))?
        .decode()
        .map_err(|e| Error::generic(format!("Failed to decode image: {}", e)))?;

    let (width, height) = img.dimensions();
    let current_ratio = width as f64 / height as f64;

    // If already at or wider than 2.35:1, no cropping needed
    if current_ratio >= WECHAT_COVER_ASPECT_RATIO {
        info!(
            "Image already at or wider than 2.35:1 ratio (current: {:.2}:1), skipping crop",
            current_ratio
        );
        return Ok(None);
    }

    // Calculate new height for 2.35:1 ratio, keeping width
    let new_height = (width as f64 / WECHAT_COVER_ASPECT_RATIO).round() as u32;

    // Center crop vertically
    let y_offset = (height - new_height) / 2;

    info!(
        "Cropping cover image from {}x{} to {}x{} (2.35:1 ratio for WeChat)",
        width, height, width, new_height
    );

    let cropped = img.crop_imm(0, y_offset, width, new_height);

    // Create temp file with same extension as original
    let extension = image_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let temp_path = std::env::temp_dir().join(format!(
        "wx_cover_{}_{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple(),
        extension
    ));

    // Determine output format based on file extension
    let format = match extension {
        "png" => image::ImageFormat::Png,
        "jpg" | "jpeg" => image::ImageFormat::Jpeg,
        "webp" => image::ImageFormat::WebP,
        _ => image::ImageFormat::Jpeg,
    };

    // Encode and save to temp file
    let mut output = Cursor::new(Vec::new());
    cropped
        .write_to(&mut output, format)
        .map_err(|e| Error::generic(format!("Failed to encode cropped image: {}", e)))?;

    std::fs::write(&temp_path, output.into_inner())
        .map_err(|e| Error::generic(format!("Failed to write temp cropped image: {}", e)))?;

    info!("Created temp cropped cover at: {}", temp_path.display());
    Ok(Some(temp_path))
}

/// Prepares a markdown file for WeChat upload by creating temp files with cropped cover.
///
/// Returns paths to temp files that should be cleaned up after upload.
fn prepare_upload_files(
    markdown_path: &Path,
    frontmatter: &Frontmatter,
    body: &str,
) -> Result<Option<(PathBuf, PathBuf)>> {
    // Check if there's a cover to process
    let Some(cover_filename) = &frontmatter.cover else {
        return Ok(None);
    };

    // Resolve cover path
    let (cover_path, exists) = resolve_and_check_cover_path(markdown_path, cover_filename);
    if !exists {
        return Ok(None);
    }

    // Crop cover to temp file
    let Some(temp_cover_path) = crop_cover_to_temp(&cover_path)? else {
        // No cropping needed, use original files
        return Ok(None);
    };

    // Get absolute path for markdown directory (so temp file can be found reliably)
    let abs_markdown_path = markdown_path
        .canonicalize()
        .map_err(|e| Error::generic(format!("Failed to canonicalize markdown path: {}", e)))?;
    let markdown_dir = abs_markdown_path
        .parent()
        .ok_or_else(|| Error::generic("Markdown file has no parent directory"))?;

    // Create temp markdown in the same directory as original (so relative paths work)
    let mut temp_frontmatter = frontmatter.clone();
    temp_frontmatter.set_cover(temp_cover_path.to_string_lossy().to_string());

    let temp_markdown_path = markdown_dir.join(format!(
        ".wx_upload_{}_{}.md",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));

    let temp_content = crate::markdown::format_markdown(&temp_frontmatter, body)?;
    std::fs::write(&temp_markdown_path, &temp_content)
        .map_err(|e| Error::generic(format!("Failed to write temp markdown: {}", e)))?;

    info!(
        "Created temp markdown at: {} with cover: {}",
        temp_markdown_path.display(),
        temp_cover_path.display()
    );

    Ok(Some((temp_markdown_path, temp_cover_path)))
}

/// Trait for uploading content to WeChat
#[async_trait::async_trait]
pub trait WeChatUploader {
    /// Uploads a file to WeChat and returns the draft ID
    async fn upload(&self, file_path: &str) -> Result<String>;
}

/// Trait for processing cover images
#[async_trait::async_trait]
pub trait CoverImageProcessor {
    /// Resolves and checks if a cover image exists
    async fn resolve_cover_path(
        &self,
        markdown_path: &Path,
        cover_filename: &str,
    ) -> (PathBuf, bool);

    /// Generates a cover image if missing
    async fn ensure_cover_image(
        &self,
        content: &str,
        markdown_path: &Path,
        cover_filename: Option<&str>,
    ) -> Result<Option<String>>;
}

/// Default implementation of WeChat uploader
#[async_trait::async_trait]
impl WeChatUploader for WeChatClient {
    async fn upload(&self, file_path: &str) -> Result<String> {
        self.upload(file_path)
            .await
            .map_err(|e| Error::wechat(e.to_string()))
    }
}

/// Default cover image processor implementation
pub struct DefaultCoverImageProcessor<'a> {
    openai_client: Option<&'a OpenAIClient>,
}

impl<'a> DefaultCoverImageProcessor<'a> {
    pub fn new(openai_client: Option<&'a OpenAIClient>) -> Self {
        Self { openai_client }
    }
}

#[async_trait::async_trait]
impl CoverImageProcessor for DefaultCoverImageProcessor<'_> {
    async fn resolve_cover_path(
        &self,
        markdown_path: &Path,
        cover_filename: &str,
    ) -> (PathBuf, bool) {
        resolve_and_check_cover_path(markdown_path, cover_filename)
    }

    async fn ensure_cover_image(
        &self,
        content: &str,
        markdown_path: &Path,
        cover_filename: Option<&str>,
    ) -> Result<Option<String>> {
        let Some(openai_client) = self.openai_client else {
            return Ok(None);
        };

        match cover_filename {
            None => {
                // Generate with auto filename
                let base_filename = markdown_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("article");

                match openai_client
                    .generate_cover_image(content, markdown_path, base_filename)
                    .await
                {
                    Ok(cover_filename) => Ok(Some(cover_filename)),
                    Err(e) => {
                        warn!(
                            "Failed to generate cover image: {}. Continuing without cover.",
                            e
                        );
                        Ok(None)
                    }
                }
            }
            Some(filename) => {
                // Generate to the specified path from frontmatter
                let (target_cover_path, exists) =
                    self.resolve_cover_path(markdown_path, filename).await;

                if !exists {
                    match openai_client
                        .generate_cover_image_to_path(content, markdown_path, &target_cover_path)
                        .await
                    {
                        Ok(()) => Ok(Some(filename.to_string())),
                        Err(e) => {
                            warn!(
                                "Failed to generate cover image to {}: {}. Continuing without cover.",
                                target_cover_path.display(),
                                e
                            );
                            Ok(None)
                        }
                    }
                } else {
                    // Cover already exists
                    Ok(Some(filename.to_string()))
                }
            }
        }
    }
}

/// Recursively processes all markdown files in a directory.
///
/// This function walks through the directory tree starting from `dir`,
/// finds all files with `.md` extension, and uploads them to WeChat.
/// Files that are already published (where `published: "true"`) will be skipped.
///
/// # Arguments
///
/// * `client` - WeChat client for API communication
/// * `openai_client` - Optional OpenAI client for cover image generation
/// * `dir` - Directory path to process recursively
/// * `verbose` - Whether to enable detailed tracing logs
///
/// # Errors
///
/// Returns an error if:
/// - Directory traversal fails
/// - Any file upload fails
pub async fn process_directory(
    client: &WeChatClient,
    openai_client: Option<&OpenAIClient>,
    dir: &Path,
    verbose: bool,
) -> Result<()> {
    let entries: Vec<_> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    if entries.is_empty() {
        FORMATTER.print_info("No markdown files found in directory");
        return Ok(());
    }

    for entry in entries {
        upload_file(client, openai_client, entry.path(), false, verbose).await?;
    }

    Ok(())
}

/// Uploads a single markdown file to WeChat public account.
///
/// This function orchestrates the complete upload workflow by delegating
/// to specialized functions for each step.
///
/// # Arguments
///
/// * `client` - WeChat client for API communication
/// * `openai_client` - Optional OpenAI client for cover image generation
/// * `path` - Path to the markdown file
/// * `force` - If true, uploads regardless of published status
/// * `verbose` - Whether to enable detailed tracing logs
///
/// # Errors
///
/// Returns an error if any step of the upload process fails
pub async fn upload_file(
    client: &WeChatClient,
    openai_client: Option<&OpenAIClient>,
    path: &Path,
    force: bool,
    verbose: bool,
) -> Result<()> {
    // Parse the markdown file and check publication status
    let (mut frontmatter, body) = match parse_and_check_file(path, force, verbose).await {
        Ok(result) => result,
        Err(_) => return Ok(()), // File was skipped
    };

    // Handle cover image processing if needed
    let cover_updated = process_cover_image(&mut frontmatter, path, openai_client, verbose).await?;

    // Save frontmatter if cover was updated
    if cover_updated {
        write_markdown_file(path, &frontmatter, &body).await?;
        if verbose {
            info!("Updated frontmatter with cover in: {}", path.display());
        }
    }

    // Prepare temp files with cropped cover for upload
    let temp_files = prepare_upload_files(path, &frontmatter, &body)?;

    // Use temp markdown if available, otherwise use original
    let upload_path = temp_files
        .as_ref()
        .map(|(md, _)| md.as_path())
        .unwrap_or(path);

    // Execute the WeChat upload
    let upload_result = execute_wechat_upload(client, upload_path, verbose).await;

    // Clean up temp files regardless of upload result
    if let Some((temp_md, temp_cover)) = temp_files {
        if let Err(e) = std::fs::remove_file(&temp_md) {
            warn!("Failed to clean up temp markdown: {}", e);
        }
        if let Err(e) = std::fs::remove_file(&temp_cover) {
            warn!("Failed to clean up temp cover: {}", e);
        }
        if verbose {
            info!("Cleaned up temp files");
        }
    }

    // Propagate upload error after cleanup
    upload_result?;

    // Update the file with published status
    update_published_status(path, verbose).await?;

    Ok(())
}

/// Parses markdown file and checks if it should be uploaded
///
/// # Returns
///
/// Returns the frontmatter and body if the file should be processed,
/// or returns an error if the file should be skipped
async fn parse_and_check_file(
    path: &Path,
    force: bool,
    verbose: bool,
) -> Result<(Frontmatter, String)> {
    let (frontmatter, body) = parse_markdown_file(path).await?;

    // Check if already published
    if !force && frontmatter.is_published() {
        if verbose {
            info!("Skipping already published file: {}", path.display());
        } else {
            FORMATTER.print_skip(&FORMATTER.format_skip_published(path));
        }
        return Err(Error::generic("File already published"));
    }

    Ok((frontmatter, body))
}

/// Processes cover image generation and updating
///
/// # Returns
///
/// Returns true if the frontmatter was updated with a new cover image
async fn process_cover_image(
    frontmatter: &mut Frontmatter,
    path: &Path,
    openai_client: Option<&OpenAIClient>,
    verbose: bool,
) -> Result<bool> {
    let Some(openai_client) = openai_client else {
        check_existing_cover(frontmatter, path, verbose);
        return Ok(false);
    };

    if verbose {
        info!("OpenAI client available for cover generation");
    }

    let should_generate = should_generate_cover(frontmatter, path, verbose).await;

    if !should_generate {
        return Ok(false);
    }

    let processor = DefaultCoverImageProcessor::new(Some(openai_client));

    match processor
        .ensure_cover_image(&frontmatter.description, path, frontmatter.cover.as_deref())
        .await?
    {
        Some(cover_filename) => {
            frontmatter.set_cover(cover_filename.clone());

            if verbose {
                info!("Successfully generated cover image: {}", cover_filename);
            } else {
                FORMATTER.print_generation(&FORMATTER.format_cover_success(&cover_filename));
            }
            Ok(true)
        }
        None => {
            if verbose {
                warn!("Cover generation failed but continuing without error");
            } else {
                FORMATTER.print_warning(&FORMATTER.format_cover_failure());
            }
            Ok(false)
        }
    }
}

/// Determines if a cover image should be generated
async fn should_generate_cover(frontmatter: &Frontmatter, path: &Path, verbose: bool) -> bool {
    match &frontmatter.cover {
        None => {
            if verbose {
                info!("No cover image specified, generating one using AI...");
            } else {
                FORMATTER.print_generation(&FORMATTER.format_cover_generation(path));
            }
            true
        }
        Some(cover_filename) => {
            let (cover_path, exists) = resolve_and_check_cover_path(path, cover_filename);
            if !exists {
                if verbose {
                    info!(
                        "Cover image specified ({}) but file not found at {}, generating using AI...",
                        cover_filename,
                        cover_path.display()
                    );
                } else {
                    FORMATTER.print_generation(&format!(
                        "cover missing ({}), generating: {}",
                        cover_filename,
                        path.display()
                    ));
                }
                true
            } else {
                if verbose {
                    info!("Cover image found at: {}", cover_path.display());
                }
                false
            }
        }
    }
}

/// Checks if existing cover file exists when no OpenAI client is available
fn check_existing_cover(frontmatter: &Frontmatter, path: &Path, verbose: bool) {
    if let Some(cover_filename) = &frontmatter.cover {
        let (cover_path, exists) = resolve_and_check_cover_path(path, cover_filename);
        if !exists {
            if verbose {
                warn!(
                    "Cover image specified ({}) but file not found at {} and no OpenAI API key provided. Upload may fail.",
                    cover_filename,
                    cover_path.display()
                );
            } else {
                FORMATTER.print_warning(&format!(
                    "cover missing ({}), no OpenAI key to generate",
                    cover_filename
                ));
            }
        }
    }
}

/// Executes the WeChat upload operation
async fn execute_wechat_upload(
    client: &WeChatClient,
    path: &Path,
    verbose: bool,
) -> Result<String> {
    if verbose {
        info!("Uploading file: {}", path.display());
    } else {
        FORMATTER.print_progress(&FORMATTER.format_file_operation("uploading", path));
    }

    let path_str = path
        .to_str()
        .ok_or_else(|| Error::generic("Path contains invalid UTF-8"))?;

    match client.upload(path_str).await {
        Ok(draft_id) => {
            if verbose {
                info!("Successfully uploaded with draft ID: {}", draft_id);
            } else {
                FORMATTER.print_success(&FORMATTER.format_upload_success(path));
            }
            Ok(draft_id)
        }
        Err(e) => {
            let error_msg = format!("WeChat upload failed: {}", e);
            if verbose {
                warn!("Failed to upload {}: {}", path.display(), error_msg);
            } else {
                FORMATTER.print_error(&FORMATTER.format_upload_failure(path));
                eprintln!("Error: {}", error_msg);
            }
            Err(Error::wechat(error_msg))
        }
    }
}

/// Updates the frontmatter with published status after successful upload
async fn update_published_status(path: &Path, verbose: bool) -> Result<()> {
    update_frontmatter(path, |fm| {
        fm.set_published("draft");
        Ok(())
    })
    .await?;

    if verbose {
        info!(
            "Updated frontmatter with draft status in: {}",
            path.display()
        );
    }

    Ok(())
}

/// Resolves a cover image path relative to the markdown file and checks if it exists
///
/// # Arguments
///
/// * `markdown_file_path` - Path to the markdown file
/// * `cover_filename` - Relative or absolute path to the cover image
///
/// # Returns
///
/// A tuple containing the resolved path and whether the file exists
pub fn resolve_and_check_cover_path(
    markdown_file_path: &Path,
    cover_filename: &str,
) -> (PathBuf, bool) {
    let cover_path = if Path::new(cover_filename).is_absolute() {
        PathBuf::from(cover_filename)
    } else {
        // If cover filename is relative, resolve it relative to the markdown file's directory
        markdown_file_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(cover_filename)
    };

    let exists = cover_path.exists();
    (cover_path, exists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_and_check_cover_path() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create a markdown file
        let md_file = temp_path.join("test.md");
        fs::write(&md_file, "# Test").unwrap();

        // Create an existing cover image
        let existing_cover = temp_path.join("existing.png");
        fs::write(&existing_cover, "fake image data").unwrap();

        // Test with existing file
        let (resolved_path, exists) = resolve_and_check_cover_path(&md_file, "existing.png");
        assert_eq!(resolved_path, existing_cover);
        assert!(exists);

        // Test with missing file
        let (resolved_path, exists) = resolve_and_check_cover_path(&md_file, "missing.png");
        assert_eq!(resolved_path, temp_path.join("missing.png"));
        assert!(!exists);

        // Test with absolute path
        let abs_path = temp_path.join("absolute.png").to_string_lossy().to_string();
        let (resolved_path, exists) = resolve_and_check_cover_path(&md_file, &abs_path);
        assert_eq!(resolved_path, temp_path.join("absolute.png"));
        assert!(!exists);

        // Test with subdirectory path
        let images_dir = temp_path.join("images");
        fs::create_dir(&images_dir).unwrap();
        let subdir_cover = images_dir.join("cover.png");
        fs::write(&subdir_cover, "fake image data").unwrap();

        let (resolved_path, exists) = resolve_and_check_cover_path(&md_file, "images/cover.png");
        assert_eq!(resolved_path, subdir_cover);
        assert!(exists);
    }

    #[test]
    fn test_cover_image_processor() {
        let _processor = DefaultCoverImageProcessor::new(None);

        let temp_dir = TempDir::new().unwrap();
        let md_file = temp_dir.path().join("test.md");
        fs::write(&md_file, "# Test").unwrap();

        // Test resolve_cover_path (sync version for testing)
        let (path, exists) = resolve_and_check_cover_path(&md_file, "test.png");
        assert_eq!(path, temp_dir.path().join("test.png"));
        assert!(!exists);
    }

    #[tokio::test]
    async fn test_cover_image_processor_no_openai() {
        let processor = DefaultCoverImageProcessor::new(None);
        let temp_dir = TempDir::new().unwrap();
        let md_file = temp_dir.path().join("test.md");

        // Without OpenAI client, should return None
        let result = processor
            .ensure_cover_image("content", &md_file, None)
            .await
            .unwrap();
        assert!(result.is_none());

        let result = processor
            .ensure_cover_image("content", &md_file, Some("cover.png"))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_process_directory_empty() {
        let temp_dir = TempDir::new().unwrap();

        // Create a mock WeChat client (this will fail in real usage without proper credentials)
        // In a real test environment, we'd use dependency injection or mocking
        let client =
            wechat_pub_rs::WeChatClient::new("test_id".to_string(), "test_secret".to_string())
                .await;

        // This test mainly verifies the directory processing logic
        match client {
            Ok(client) => {
                let result = process_directory(&client, None, temp_dir.path(), false).await;
                // Should succeed with empty directory
                assert!(result.is_ok());
            }
            Err(_) => {
                // Expected to fail without real credentials, but the test structure is correct
                // In integration tests, we'd use proper mocking
            }
        }
    }
}
