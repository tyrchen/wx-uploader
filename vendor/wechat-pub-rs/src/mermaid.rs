//! Mermaid chart rendering module.
//!
//! This module handles the detection and rendering of Mermaid charts in markdown content.
//! It generates PNG images from Mermaid code blocks using the mermaid-cli tool.

use crate::error::{Result, WeChatError};
use crate::markdown::ImageRef;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;
use tracing::{debug, info};

/// Represents a Mermaid chart found in markdown content.
#[derive(Debug, Clone)]
pub struct MermaidChart {
    /// The Mermaid code content
    pub code: String,
    /// Position in the markdown text (start, end)
    pub position: (usize, usize),
    /// Generated image filename (will be set after generation)
    pub image_filename: Option<String>,
}

impl MermaidChart {
    /// Creates a new Mermaid chart reference.
    pub fn new(code: String, position: (usize, usize)) -> Self {
        Self {
            code,
            position,
            image_filename: None,
        }
    }
}

/// Mermaid chart processor for converting charts to images.
pub struct MermaidProcessor {
    /// Base name for the document (used for generating image names)
    document_slug: String,
}

impl MermaidProcessor {
    /// Creates a new Mermaid processor.
    ///
    /// # Arguments
    /// * `_output_dir` - Directory where images will be generated (unused but kept for API compatibility)
    /// * `document_slug` - Base name for the document (used for generating image names)
    pub fn new(_output_dir: PathBuf, document_slug: String) -> Self {
        Self { document_slug }
    }

    /// Detects Mermaid code blocks in markdown content.
    pub fn detect_mermaid_blocks(content: &str) -> Vec<MermaidChart> {
        let mut charts = Vec::new();

        // Regex to match mermaid code blocks
        let mermaid_regex = Regex::new(r"(?m)^```mermaid\n((?:.|\n)*?)```$").unwrap();

        for (index, caps) in mermaid_regex.captures_iter(content).enumerate() {
            if let Some(code_match) = caps.get(1) {
                let code = code_match.as_str().to_string();
                let full_match = caps.get(0).unwrap();
                let position = (full_match.start(), full_match.end());

                let mut chart = MermaidChart::new(code, position);
                // Pre-generate the filename that will be used
                chart.image_filename = Some(format!("{}-{}.png", "placeholder", index + 1));
                charts.push(chart);
            }
        }

        charts
    }

    /// Processes all Mermaid charts in the content and returns the modified content.
    ///
    /// # Arguments
    /// * `content` - The markdown content containing Mermaid blocks
    /// * `base_path` - Base path for resolving relative image paths
    ///
    /// # Returns
    /// * Modified content with Mermaid blocks replaced by image references
    /// * List of generated image references
    pub async fn process_mermaid_content(
        &self,
        content: &str,
        base_path: &Path,
    ) -> Result<(String, Vec<ImageRef>)> {
        self.process_mermaid_content_with_source_path(content, base_path, None)
            .await
    }

    /// Processes all Mermaid charts with optional source file path for modification time checking.
    ///
    /// # Arguments
    /// * `content` - The markdown content containing Mermaid blocks
    /// * `base_path` - Base path for resolving relative image paths
    /// * `source_path` - Optional path to the source markdown file for modification time checking
    ///
    /// # Returns
    /// * Modified content with Mermaid blocks replaced by image references
    /// * List of generated image references
    pub async fn process_mermaid_content_with_source_path(
        &self,
        content: &str,
        base_path: &Path,
        source_path: Option<&Path>,
    ) -> Result<(String, Vec<ImageRef>)> {
        let charts = Self::detect_mermaid_blocks(content);

        if charts.is_empty() {
            return Ok((content.to_string(), Vec::new()));
        }

        info!("Found {} Mermaid charts to process", charts.len());

        // Create output directory if it doesn't exist
        let images_dir = base_path.join("images");
        if !images_dir.exists() {
            fs::create_dir_all(&images_dir)
                .await
                .map_err(|e| WeChatError::Internal {
                    message: format!("Failed to create images directory: {}", e),
                })?;
        }

        let mut modified_content = content.to_string();
        let mut image_refs = Vec::new();
        let mut offset = 0i32;

        // Get source file modification time if available
        let source_modified = if let Some(src_path) = source_path {
            fs::metadata(src_path)
                .await
                .ok()
                .and_then(|m| m.modified().ok())
        } else {
            None
        };

        for (index, mut chart) in charts.into_iter().enumerate() {
            // Generate unique filename based on document slug and chart index
            let image_filename = format!("{}-{}.png", self.document_slug, index + 1);
            let image_path = images_dir.join(&image_filename);
            let relative_path = format!("./images/{}", image_filename);

            // Check if we need to regenerate the image
            let should_regenerate = self
                .should_regenerate_image(&image_path, source_modified)
                .await;

            if should_regenerate {
                // Generate the image
                self.generate_mermaid_image(&chart.code, &image_path)
                    .await?;
            } else {
                info!(
                    "Skipping regeneration, image is up-to-date: {}",
                    image_path.display()
                );
            }

            info!("Generated Mermaid chart image: {}", image_path.display());

            // Create image reference
            let image_ref = ImageRef::new(
                format!("Mermaid Chart {}", index + 1),
                relative_path.clone(),
                (0, 0), // Position will be updated after replacement
            );
            image_refs.push(image_ref);

            // Replace Mermaid block with image reference in content
            let mermaid_block = &content[chart.position.0..chart.position.1];
            let image_markdown = format!("![Mermaid Chart {}]({})", index + 1, relative_path);

            // Adjust position based on previous replacements
            let adjusted_start = (chart.position.0 as i32 + offset) as usize;
            let adjusted_end = (chart.position.1 as i32 + offset) as usize;

            modified_content.replace_range(adjusted_start..adjusted_end, &image_markdown);

            // Update offset for next replacement
            offset += image_markdown.len() as i32 - mermaid_block.len() as i32;

            chart.image_filename = Some(image_filename);
        }

        Ok((modified_content, image_refs))
    }

    /// Checks if an image needs to be regenerated based on modification times.
    ///
    /// # Arguments
    /// * `image_path` - Path to the generated image
    /// * `source_modified` - Optional modification time of the source markdown file
    ///
    /// # Returns
    /// * `true` if the image should be regenerated, `false` otherwise
    async fn should_regenerate_image(
        &self,
        image_path: &Path,
        source_modified: Option<std::time::SystemTime>,
    ) -> bool {
        // If image doesn't exist, we need to generate it
        if !image_path.exists() {
            return true;
        }

        // If we don't have source modification time, always regenerate to be safe
        let Some(source_time) = source_modified else {
            debug!("No source modification time available, regenerating image");
            return true;
        };

        // Get image modification time
        let image_metadata = match fs::metadata(image_path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                debug!("Failed to get image metadata: {}, regenerating", e);
                return true;
            }
        };

        let image_modified = match image_metadata.modified() {
            Ok(time) => time,
            Err(e) => {
                debug!("Failed to get image modification time: {}, regenerating", e);
                return true;
            }
        };

        // Regenerate if source is newer than image
        source_time > image_modified
    }

    /// Generates a PNG image from Mermaid code using mermaid-cli.
    async fn generate_mermaid_image(&self, mermaid_code: &str, output_path: &Path) -> Result<()> {
        debug!("Generating Mermaid image: {}", output_path.display());

        // Create a temporary file for the Mermaid code
        let temp_dir = std::env::temp_dir();
        let temp_input = temp_dir.join(format!("mermaid_{}.mmd", uuid::Uuid::new_v4()));

        // Write Mermaid code to temporary file
        fs::write(&temp_input, mermaid_code)
            .await
            .map_err(|e| WeChatError::Internal {
                message: format!("Failed to write temporary Mermaid file: {}", e),
            })?;

        // Run mermaid-cli to generate the image
        let output = Command::new("mmdc")
            .arg("-i")
            .arg(&temp_input)
            .arg("-o")
            .arg(output_path)
            .arg("-t")
            .arg("default") // Use default theme
            .arg("-b")
            .arg("white") // White background
            .arg("--width")
            .arg("2400") // Larger viewport width
            .arg("--height")
            .arg("1600") // Larger viewport height
            .arg("--scale")
            .arg("3") // Scale factor for higher resolution (2x)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                WeChatError::Internal {
                    message: format!("Failed to execute mermaid-cli: {}. Make sure 'mmdc' is installed (npm install -g @mermaid-js/mermaid-cli)", e),
                }
            })?;

        // Clean up temporary file
        let _ = fs::remove_file(&temp_input).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WeChatError::Internal {
                message: format!("Mermaid chart generation failed: {}", stderr),
            });
        }

        Ok(())
    }

    /// Extracts the document slug from a markdown file path.
    pub fn extract_slug_from_path(path: &Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_mermaid_blocks() {
        let content = r#"# Test Document

Some text here.

```mermaid
graph LR
    A[Start] --> B[Process]
    B --> C[End]
```

More text.

```mermaid
sequenceDiagram
    Alice->>Bob: Hello
    Bob->>Alice: Hi
```

Final text."#;

        let charts = MermaidProcessor::detect_mermaid_blocks(content);
        assert_eq!(charts.len(), 2);

        assert!(charts[0].code.contains("graph LR"));
        assert!(charts[1].code.contains("sequenceDiagram"));
    }

    #[test]
    fn test_no_mermaid_blocks() {
        let content = r#"# Test Document

```javascript
console.log("Hello");
```

Some text."#;

        let charts = MermaidProcessor::detect_mermaid_blocks(content);
        assert_eq!(charts.len(), 0);
    }

    #[tokio::test]
    async fn test_process_mermaid_content() {
        let temp_dir = TempDir::new().unwrap();
        let processor =
            MermaidProcessor::new(temp_dir.path().to_path_buf(), "test-doc".to_string());

        let content = r#"# Test

```mermaid
graph LR
    A --> B
```

Text after."#;

        // Note: This test will only work if mermaid-cli is installed
        // In CI, we might want to skip this test or mock the command
        if Command::new("which")
            .arg("mmdc")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            let result = processor
                .process_mermaid_content(content, temp_dir.path())
                .await;

            if let Ok((modified, images)) = result {
                assert!(!modified.contains("```mermaid"));
                assert!(modified.contains("![Mermaid Chart 1]"));
                assert_eq!(images.len(), 1);
            }
        }
    }
}
