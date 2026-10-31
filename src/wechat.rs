//! WeChat public account integration
//!
//! This module provides WeChat public account functionality for uploading
//! markdown articles with automatic cover image generation and frontmatter management.

use crate::error::{Error, Result};
use crate::gemini::GeminiClient;
use crate::image_prompt::build_cover_prompt;
use crate::markdown::{parse_markdown_file, update_frontmatter, write_markdown_file};
use crate::models::{Config, Frontmatter};
use crate::openai::OpenAIClient;
use crate::output::{FORMATTER, FilePathFormatter, OutputFormatter};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

// Re-export the WeChat client type
pub use wechat_pub_rs::WeChatClient;

pub use crate::upload_images::TempUploadFiles;
use crate::upload_images::prepare_upload_files;

/// Trait for uploading content to WeChat
// async_trait adds #[must_use] to methods returning already-must-use boxed futures.
#[allow(clippy::double_must_use)]
#[async_trait::async_trait]
pub trait WeChatUploader {
    /// Uploads a file to WeChat and returns the draft ID
    async fn upload(&self, file_path: &str) -> Result<String>;
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

/// Image generation backend, resolved per-file from frontmatter model field
#[derive(Debug)]
enum ImageBackend {
    Gemini(GeminiClient),
    OpenAI(OpenAIClient),
}

impl ImageBackend {
    /// Generate a cover image with auto-generated filename
    async fn generate_cover_image(
        &self,
        content: &str,
        file_path: &Path,
        base_filename: &str,
    ) -> Result<String> {
        match self {
            Self::Gemini(client) => {
                client
                    .generate_cover_image(content, file_path, base_filename)
                    .await
            }
            Self::OpenAI(client) => {
                client
                    .generate_cover_image(content, file_path, base_filename)
                    .await
            }
        }
    }

    /// Generate a cover image to a specific path
    async fn generate_cover_image_to_path(
        &self,
        content: &str,
        markdown_file_path: &Path,
        target_cover_path: &Path,
    ) -> Result<()> {
        match self {
            Self::Gemini(client) => {
                client
                    .generate_cover_image_to_path(content, markdown_file_path, target_cover_path)
                    .await
            }
            Self::OpenAI(client) => {
                client
                    .generate_cover_image_to_path(content, markdown_file_path, target_cover_path)
                    .await
            }
        }
    }
}

/// Resolve the image generation backend from frontmatter model and config
fn resolve_backend(frontmatter: &Frontmatter, config: &Config) -> Result<ImageBackend> {
    let model = frontmatter.effective_model();

    match model {
        "nb2" | "nb" => {
            let model_id: &'static str = match model {
                "nb" => "gemini-3-pro-image-preview",
                _ => "gemini-nano-banana-2.1",
            };
            let client = match config
                .gemini_api_key
                .as_ref()
                .filter(|key| !key.trim().is_empty())
            {
                Some(api_key) => GeminiClient::new(api_key.clone(), model_id),
                None => GeminiClient::from_adc(model_id),
            };
            Ok(ImageBackend::Gemini(client))
        }
        "gpt" => {
            let Some(api_key) = config.openai_api_key.as_ref() else {
                return Err(Error::config(
                    "OPENAI_API_KEY required for model 'gpt'. Set OPENAI_API_KEY or use model: nb2 in frontmatter.",
                ));
            };
            Ok(ImageBackend::OpenAI(OpenAIClient::new(api_key.clone())))
        }
        _ => {
            // Should not happen if frontmatter validation is correct
            Err(Error::config(format!("Unknown model '{}'", model)))
        }
    }
}

/// Recursively processes all markdown files in a directory.
pub async fn process_directory(
    client: &WeChatClient,
    config: &Config,
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
        upload_file(client, config, entry.path(), false, verbose).await?;
    }

    Ok(())
}

/// Uploads a single markdown file to WeChat public account.
pub async fn upload_file(
    client: &WeChatClient,
    config: &Config,
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
    let cover_updated = process_cover_image(&mut frontmatter, path, config, verbose).await?;

    // Save frontmatter if cover was updated
    if cover_updated {
        write_markdown_file(path, &frontmatter, &body).await?;
        if verbose {
            info!("Updated frontmatter with cover in: {}", path.display());
        }
    }

    // Keep the private upload directory alive for the entire request.
    let temp_files = prepare_upload_files(path, &frontmatter, &body).await?;

    // Execute the WeChat upload (display original path, upload from temp)
    let upload_result =
        execute_wechat_upload(client, temp_files.markdown.path(), path, verbose).await;

    // Temp files are automatically cleaned up when temp_files is dropped
    drop(temp_files);

    // Propagate upload error after cleanup
    upload_result?;

    // Update the file with published status
    update_published_status(path, verbose).await?;

    Ok(())
}

/// Parses markdown file and checks if it should be uploaded
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
async fn process_cover_image(
    frontmatter: &mut Frontmatter,
    path: &Path,
    config: &Config,
    verbose: bool,
) -> Result<bool> {
    // Check if we need to generate at all
    if !should_generate_cover(frontmatter, path, verbose).await {
        return Ok(false);
    }

    // Resolve the backend based on frontmatter model
    let backend = match resolve_backend(frontmatter, config) {
        Ok(backend) => backend,
        Err(e) => {
            // Invalid backend configuration — warn and continue without cover
            FORMATTER.print_warning(&e.to_string());
            return Ok(false);
        }
    };

    let model_name = frontmatter.effective_model();
    if verbose {
        info!("Using model '{}' for cover generation", model_name);
    }

    let image_prompt = match build_cover_prompt(frontmatter) {
        Ok(prompt) => prompt,
        Err(error) => {
            FORMATTER.print_warning(&error.to_string());
            return Ok(false);
        }
    };

    // Generate the cover image
    match &frontmatter.cover {
        None => {
            // Generate with auto filename
            let base_filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("article");

            match backend
                .generate_cover_image(&image_prompt, path, base_filename)
                .await
            {
                Ok(cover_filename) => {
                    frontmatter.set_cover(cover_filename.clone());
                    if verbose {
                        info!("Successfully generated cover image: {}", cover_filename);
                    } else {
                        FORMATTER
                            .print_generation(&FORMATTER.format_cover_success(&cover_filename));
                    }
                    Ok(true)
                }
                Err(e) => {
                    warn!(
                        "Failed to generate cover image: {}. Continuing without cover.",
                        e
                    );
                    if !verbose {
                        FORMATTER.print_warning(&format!(
                            "{} {}",
                            FORMATTER.format_cover_failure(),
                            e
                        ));
                    }
                    Ok(false)
                }
            }
        }
        Some(cover_filename) => {
            let (target_cover_path, exists) = resolve_and_check_cover_path(path, cover_filename);

            if exists {
                return Ok(false);
            }

            match backend
                .generate_cover_image_to_path(&image_prompt, path, &target_cover_path)
                .await
            {
                Ok(()) => {
                    if verbose {
                        info!("Successfully generated cover image: {}", cover_filename);
                    } else {
                        FORMATTER.print_generation(&FORMATTER.format_cover_success(cover_filename));
                    }
                    Ok(true)
                }
                Err(e) => {
                    warn!(
                        "Failed to generate cover image to {}: {}. Continuing without cover.",
                        target_cover_path.display(),
                        e
                    );
                    if !verbose {
                        FORMATTER.print_warning(&format!(
                            "{} {}",
                            FORMATTER.format_cover_failure(),
                            e
                        ));
                    }
                    Ok(false)
                }
            }
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

/// Executes the WeChat upload operation
///
/// `upload_path` is the actual file to upload (may be a temp file).
/// `display_path` is the original file path shown to the user.
async fn execute_wechat_upload(
    client: &WeChatClient,
    upload_path: &Path,
    display_path: &Path,
    verbose: bool,
) -> Result<String> {
    if verbose {
        info!("Uploading file: {}", display_path.display());
    } else {
        FORMATTER.print_progress(&FORMATTER.format_file_operation("uploading", display_path));
    }

    let path_str = upload_path
        .to_str()
        .ok_or_else(|| Error::generic("Path contains invalid UTF-8"))?;

    match client.upload(path_str).await {
        Ok(draft_id) => {
            if verbose {
                info!("Successfully uploaded with draft ID: {}", draft_id);
            } else {
                FORMATTER.print_success(&FORMATTER.format_upload_success(display_path));
            }
            Ok(draft_id)
        }
        Err(e) => {
            let error_msg = format!("WeChat upload failed: {}", e);
            if verbose {
                warn!("Failed to upload {}: {}", display_path.display(), error_msg);
            } else {
                FORMATTER.print_error(&FORMATTER.format_upload_failure(display_path));
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
    fn test_resolve_backend_gemini_nb2() {
        let frontmatter = Frontmatter::new(); // defaults to nb2
        let config = Config::new(
            "app".into(),
            "secret".into(),
            None,
            Some("gemini-key".into()),
            false,
        );

        let backend = resolve_backend(&frontmatter, &config);
        assert!(backend.is_ok());
    }

    #[test]
    fn test_resolve_backend_gemini_uses_adc_without_key() {
        let frontmatter = Frontmatter::new(); // defaults to nb2
        let config = Config::new("app".into(), "secret".into(), None, None, false);

        let result = resolve_backend(&frontmatter, &config);
        assert!(matches!(result.unwrap(), ImageBackend::Gemini(_)));
    }

    #[test]
    fn test_resolve_backend_gpt() {
        let mut frontmatter = Frontmatter::new();
        frontmatter.model = Some("gpt".into());
        let config = Config::new(
            "app".into(),
            "secret".into(),
            Some("openai-key".into()),
            None,
            false,
        );

        let backend = resolve_backend(&frontmatter, &config);
        assert!(backend.is_ok());
    }

    #[test]
    fn test_resolve_backend_gpt_missing_key() {
        let mut frontmatter = Frontmatter::new();
        frontmatter.model = Some("gpt".into());
        let config = Config::new("app".into(), "secret".into(), None, None, false);

        let result = resolve_backend(&frontmatter, &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("OPENAI_API_KEY"));
    }

    #[tokio::test]
    async fn test_process_directory_empty() {
        let temp_dir = TempDir::new().unwrap();

        let client =
            wechat_pub_rs::WeChatClient::new("test_id".to_string(), "test_secret".to_string())
                .await;

        match client {
            Ok(client) => {
                let config = Config::new("test_id".into(), "test_secret".into(), None, None, false);
                let result = process_directory(&client, &config, temp_dir.path(), false).await;
                assert!(result.is_ok());
            }
            Err(_) => {
                // Expected to fail without real credentials
            }
        }
    }
}
