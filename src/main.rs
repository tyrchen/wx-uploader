//! WeChat Public Account Markdown Uploader
//!
//! A command-line tool for uploading markdown files to WeChat public accounts.
//! This tool provides functionality to:
//!
//! - Upload individual markdown files to WeChat public accounts
//! - Recursively scan directories for markdown files and upload them
//! - Parse YAML frontmatter to track publication status
//! - Automatically update frontmatter after successful uploads
//! - Skip already published files when processing directories
//!
//! ## Usage
//!
//! Set the required environment variables:
//! ```bash
//! export WECHAT_APP_ID="your_app_id"
//! export WECHAT_APP_SECRET="your_app_secret"
//! ```
//!
//! Upload a single file (forces upload regardless of published status):
//! ```bash
//! wx-uploader article.md
//! ```
//!
//! Process all markdown files in a directory:
//! ```bash
//! wx-uploader ./articles/
//! ```
//!
//! ## Frontmatter Format
//!
//! The tool recognizes YAML frontmatter in the following format:
//! ```markdown
//! ---
//! title: "Article Title"
//! published: "draft"
//! ---
//! # Article Content
//! ```
//!
//! The `published` field is used to track upload status:
//! - `null` or missing: not uploaded
//! - `"draft"`: uploaded as draft
//! - `"true"`: published (will be skipped in directory mode)

use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use wechat_pub_rs::WeChatClient;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "A tool to upload articles to WeChat Official Account",
    after_help = "REQUIREMENTS: Set WECHAT_APP_ID and WECHAT_APP_SECRET environment variables\n\nTHEMING: Supports 8 themes and 10 code highlighters via frontmatter:\n  ---\n  title: \"My Article\"\n  theme: \"lapis\"        # Themes: default, lapis, maize, orangeheart, phycat, pie, purple, rainbow\n  code: \"github\"        # Highlighters: github, github-dark, vscode, atom-one-light, atom-one-dark, solarized-light, solarized-dark, monokai, dracula, xcode\n  published: \"draft\"\n  ---\n\nEXAMPLES:\n  wx-uploader article.md      # Upload single file (force)\n  wx-uploader ./articles/     # Process directory (skip published)\n  wx-uploader -v ./blog/      # Verbose logging"
)]
struct Args {
    #[arg(
        help = "Path to markdown file or directory to upload. Files uploaded regardless of status. Directories skip published files. Set theme and code highlighter in frontmatter - see help for complete lists."
    )]
    path: PathBuf,

    #[arg(
        short,
        long,
        help = "Enable verbose logging with detailed tracing information"
    )]
    verbose: bool,
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
/// ---
/// ```
#[derive(Debug, Deserialize, Serialize, Default)]
struct Frontmatter {
    /// The title of the article. Optional field that will be omitted from
    /// serialization if not present.
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,

    /// Publication status of the article.
    ///
    /// Common values:
    /// - `None` or missing: not uploaded
    /// - `"draft"`: uploaded as draft to WeChat
    /// - `"true"`: published (will be skipped in directory mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    published: Option<String>,

    /// Captures any additional fields in the frontmatter that are not
    /// explicitly defined in this struct.
    #[serde(flatten)]
    other: serde_yaml::Value,
}

/// Main entry point for the WeChat uploader application.
///
/// This function:
/// 1. Parses command line arguments
/// 2. Initializes logging conditionally based on verbose flag
/// 3. Reads WeChat API credentials from environment variables
/// 4. Creates a WeChat client
/// 5. Processes the input path (file or directory)
///
/// # Environment Variables
///
/// - `WECHAT_APP_ID`: WeChat application ID
/// - `WECHAT_APP_SECRET`: WeChat application secret
///
/// # Command Line Options
///
/// - `-v, --verbose`: Enable detailed tracing logs
///
/// # Errors
///
/// Returns an error if:
/// - Required environment variables are not set
/// - WeChat client initialization fails
/// - File/directory processing fails
/// - The provided path is neither a file nor a directory
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging based on verbose flag
    if args.verbose {
        tracing_subscriber::fmt::init();
    }

    // WeChatClient needs app_id and app_secret from environment variables
    let app_id =
        std::env::var("WECHAT_APP_ID").context("WECHAT_APP_ID environment variable not set")?;
    let app_secret = std::env::var("WECHAT_APP_SECRET")
        .context("WECHAT_APP_SECRET environment variable not set")?;

    let client = WeChatClient::new(app_id, app_secret).await?;

    if args.path.is_file() {
        // Force upload single file
        upload_file(&client, &args.path, true, args.verbose).await?;
    } else if args.path.is_dir() {
        // Process directory
        process_directory(&client, &args.path, args.verbose).await?;
    } else {
        anyhow::bail!("Path must be a file or directory");
    }

    Ok(())
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
/// * `dir` - Directory path to process recursively
/// * `verbose` - Whether to enable detailed tracing logs
///
/// # Errors
///
/// Returns an error if:
/// - Directory traversal fails
/// - Any file upload fails
///
/// # Examples
///
/// ```no_run
/// # use wechat_pub_rs::WeChatClient;
/// # use std::path::Path;
/// # async {
/// let client = WeChatClient::new("app_id".to_string(), "secret".to_string()).await?;
/// process_directory(&client, Path::new("./articles"), false).await?;
/// # Ok::<(), anyhow::Error>(())
/// # };
/// ```
async fn process_directory(client: &WeChatClient, dir: &Path, verbose: bool) -> Result<()> {
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
    {
        upload_file(client, entry.path(), false, verbose).await?;
    }
    Ok(())
}

/// Uploads a single markdown file to WeChat public account.
///
/// This function handles the complete upload workflow:
/// 1. Reads and parses the markdown file
/// 2. Checks publication status (unless forced)
/// 3. Uploads to WeChat if needed
/// 4. Updates the frontmatter with draft status
/// 5. Writes the updated content back to the file
///
/// # Arguments
///
/// * `client` - WeChat client for API communication
/// * `path` - Path to the markdown file
/// * `force` - If true, uploads regardless of published status
/// * `verbose` - Whether to enable detailed tracing logs
///
/// # Behavior
///
/// - If `force` is false and the file has `published: "true"`, it will be skipped
/// - After successful upload, the frontmatter is updated with `published: "draft"`
/// - The original file is modified to reflect the new status
/// - Output format depends on verbose flag: clean user-friendly messages or detailed logs
///
/// # Errors
///
/// Returns an error if:
/// - File reading fails
/// - Markdown parsing fails
/// - WeChat upload fails
/// - File writing fails
///
/// # Examples
///
/// ```no_run
/// # use wechat_pub_rs::WeChatClient;
/// # use std::path::Path;
/// # async {
/// let client = WeChatClient::new("app_id".to_string(), "secret".to_string()).await?;
///
/// // Force upload regardless of status with clean output
/// upload_file(&client, Path::new("article.md"), true, false).await?;
///
/// // Upload only if not already published with verbose logging
/// upload_file(&client, Path::new("article.md"), false, true).await?;
/// # Ok::<(), anyhow::Error>(())
/// # };
/// ```
async fn upload_file(client: &WeChatClient, path: &Path, force: bool, verbose: bool) -> Result<()> {
    // Read and parse the markdown file
    let content = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let (mut frontmatter, body) = parse_markdown(&content)?;

    // Check if already published
    if !force {
        if let Some(published) = &frontmatter.published {
            if published == "true" {
                if verbose {
                    tracing::info!("Skipping already published file: {}", path.display());
                } else {
                    println!(
                        "{} {}",
                        "⏭".bright_yellow(),
                        format!("skipped: {}", path.display()).dimmed()
                    );
                }
                return Ok(());
            }
        }
    }

    // Upload to WeChat using original file path to preserve relative image paths
    if verbose {
        tracing::info!("Uploading file: {}", path.display());
    } else {
        println!(
            "{} {}",
            "🔄".bright_blue(),
            format!("uploading: {}", path.display()).bright_white()
        );
    }

    match client.upload(path.to_str().unwrap()).await {
        Ok(draft_id) => {
            if verbose {
                tracing::info!("Successfully uploaded with draft ID: {}", draft_id);
            } else {
                println!(
                    "{} {} {}",
                    "✓".bright_green(),
                    "uploaded:".green(),
                    path.display()
                );
            }

            // Update frontmatter with published status
            frontmatter.published = Some("draft".to_string());

            let updated_content = format_markdown(&frontmatter, &body)?;
            tokio::fs::write(path, updated_content)
                .await
                .with_context(|| format!("Failed to update file: {}", path.display()))?;

            if verbose {
                tracing::info!("Updated frontmatter in: {}", path.display());
            }
        }
        Err(e) => {
            if verbose {
                tracing::error!("Failed to upload {}: {}", path.display(), e);
            } else {
                println!(
                    "{} {} {}",
                    "✗".bright_red(),
                    "failed:".red(),
                    path.display()
                );
                eprintln!("{} {}", "Error:".bright_red(), e);
            }
            return Err(e).context("Upload failed");
        }
    }

    Ok(())
}

/// Parses a markdown file with optional YAML frontmatter.
///
/// This function splits a markdown file into its frontmatter and body content.
/// It expects frontmatter to be delimited by `---` at the beginning and end,
/// followed by the markdown body.
///
/// # Arguments
///
/// * `content` - The complete markdown file content as a string
///
/// # Returns
///
/// A tuple containing:
/// - `Frontmatter` - Parsed YAML frontmatter (or default if none exists)
/// - `String` - The markdown body content
///
/// # Errors
///
/// Returns an error if:
/// - The regex compilation fails (should never happen with a valid pattern)
/// - The YAML frontmatter is malformed and cannot be parsed
///
/// # Examples
///
/// ```
/// # use wx_uploader::parse_markdown;
/// let content = r#"---
/// title: "My Article"
/// published: "draft"
/// ---
/// # Hello World
///
/// This is the content.
/// "#;
///
/// let (frontmatter, body) = parse_markdown(content).unwrap();
/// assert_eq!(frontmatter.title, Some("My Article".to_string()));
/// assert!(body.contains("Hello World"));
/// ```
///
/// Files without frontmatter are also supported:
///
/// ```
/// # use wx_uploader::parse_markdown;
/// let content = "# Just a title\n\nSome content.";
/// let (frontmatter, body) = parse_markdown(content).unwrap();
/// assert_eq!(frontmatter.title, None);
/// assert_eq!(body, content);
/// ```
fn parse_markdown(content: &str) -> Result<(Frontmatter, String)> {
    // Use (?s) flag to make . match newlines
    let re = Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$").context("Failed to create regex")?;

    if let Some(captures) = re.captures(content) {
        let yaml_str = captures.get(1).unwrap().as_str();
        let body = captures.get(2).unwrap().as_str();

        let frontmatter: Frontmatter =
            serde_yaml::from_str(yaml_str).context("Failed to parse frontmatter")?;

        Ok((frontmatter, body.to_string()))
    } else {
        // No frontmatter, create default
        let frontmatter = Frontmatter::default();
        Ok((frontmatter, content.to_string()))
    }
}

/// Formats frontmatter and body content back into a complete markdown file.
///
/// This function takes parsed frontmatter and markdown body content and
/// reconstructs the complete markdown file with proper YAML frontmatter
/// delimiters.
///
/// # Arguments
///
/// * `frontmatter` - The frontmatter structure to serialize
/// * `body` - The markdown body content
///
/// # Returns
///
/// A complete markdown file as a string with frontmatter and body
///
/// # Errors
///
/// Returns an error if the frontmatter cannot be serialized to YAML
///
/// # Examples
///
/// ```
/// # use wx_uploader::{Frontmatter, format_markdown};
/// let mut frontmatter = Frontmatter::default();
/// frontmatter.title = Some("My Article".to_string());
/// frontmatter.published = Some("draft".to_string());
///
/// let body = "# Hello World\n\nThis is content.";
/// let result = format_markdown(&frontmatter, body).unwrap();
///
/// assert!(result.starts_with("---\n"));
/// assert!(result.contains("title: My Article"));
/// assert!(result.contains("Hello World"));
/// ```
fn format_markdown(frontmatter: &Frontmatter, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(frontmatter).context("Failed to serialize frontmatter")?;

    Ok(format!("---\n{yaml}---\n{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_with_frontmatter() {
        let content = r#"---
title: "Test Article"
published: "draft"
author: "John Doe"
---
# Hello World

This is the content of the article.
"#;

        let (frontmatter, body) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, Some("Test Article".to_string()));
        assert_eq!(frontmatter.published, Some("draft".to_string()));
        assert_eq!(
            body,
            "# Hello World\n\nThis is the content of the article.\n"
        );

        // Check that additional fields are captured
        if let serde_yaml::Value::Mapping(map) = &frontmatter.other {
            assert!(map.contains_key(serde_yaml::Value::String("author".to_string())));
        } else {
            panic!("Expected mapping for other fields");
        }
    }

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let content = "# Just a Title\n\nSome content without frontmatter.";

        let (frontmatter, body) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, None);
        assert_eq!(frontmatter.published, None);
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_markdown_empty_frontmatter() {
        let content = r#"---

---
# Content Only

Just the body.
"#;

        let (frontmatter, body) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, None);
        assert_eq!(frontmatter.published, None);
        assert_eq!(body, "# Content Only\n\nJust the body.\n");
    }

    #[test]
    fn test_parse_markdown_minimal_frontmatter() {
        let content = r#"---
published: "true"
---
Content here.
"#;

        let (frontmatter, body) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, None);
        assert_eq!(frontmatter.published, Some("true".to_string()));
        assert_eq!(body, "Content here.\n");
    }

    #[test]
    fn test_parse_markdown_multiline_content() {
        let content = r#"---
title: "Multi-line Test"
---
# First Line

Second line

Third line with **markdown**.

```rust
fn test() {
    println!("Hello");
}
```
"#;

        let (frontmatter, body) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, Some("Multi-line Test".to_string()));
        assert!(body.contains("First Line"));
        assert!(body.contains("```rust"));
        assert!(body.contains("println!"));
    }

    #[test]
    fn test_format_markdown_basic() {
        let frontmatter = Frontmatter {
            title: Some("Test Title".to_string()),
            published: Some("draft".to_string()),
            ..Default::default()
        };

        let body = "# Content\n\nSome text.";

        let result = format_markdown(&frontmatter, body).unwrap();

        assert!(result.starts_with("---\n"));
        assert!(result.contains("title: Test Title"));
        assert!(result.contains("published: draft"));
        assert!(result.contains("---\n# Content"));
        assert!(result.ends_with("Some text."));
    }

    #[test]
    fn test_format_markdown_empty_frontmatter() {
        let frontmatter = Frontmatter::default();
        let body = "Just content.";

        let result = format_markdown(&frontmatter, body).unwrap();

        assert!(result.starts_with("---\n"));
        assert!(result.ends_with("---\nJust content."));
        // Should only contain the YAML null marker and body
        assert!(result.contains("null\n") || result.contains("{}\n"));
    }

    #[test]
    fn test_format_markdown_preserves_body_formatting() {
        let frontmatter = Frontmatter {
            title: Some("Formatting Test".to_string()),
            ..Default::default()
        };

        let body = r#"# Title

## Subtitle

- List item 1
- List item 2

```rust
fn main() {
    println!("Hello");
}
```

End of content."#;

        let result = format_markdown(&frontmatter, body).unwrap();

        assert!(result.contains("# Title"));
        assert!(result.contains("## Subtitle"));
        assert!(result.contains("- List item 1"));
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main()"));
        assert!(result.ends_with("End of content."));
    }

    #[test]
    fn test_roundtrip_parse_and_format() {
        let original_content = r#"---
title: "Roundtrip Test"
published: "draft"
author: "Test Author"
tags:
  - rust
  - testing
---
# Test Article

This is a test article with various content.

## Section

More content here.
"#;

        // Parse the content
        let (frontmatter, body) = parse_markdown(original_content).unwrap();

        // Format it back
        let formatted = format_markdown(&frontmatter, &body).unwrap();

        // Parse again to verify
        let (parsed_frontmatter, parsed_body) = parse_markdown(&formatted).unwrap();

        // Verify the data is preserved
        assert_eq!(frontmatter.title, parsed_frontmatter.title);
        assert_eq!(frontmatter.published, parsed_frontmatter.published);
        assert_eq!(body, parsed_body);
    }

    #[test]
    fn test_parse_markdown_invalid_yaml() {
        let content = r#"---
title: "Test"
invalid: [unclosed bracket
---
Content
"#;

        let result = parse_markdown(content);
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Failed to parse frontmatter"));
    }
}
