//! Markdown parsing and formatting utilities
//!
//! This module provides functionality for parsing markdown files with YAML frontmatter
//! and formatting them back into complete markdown files.

use crate::error::{Error, Result};
use crate::models::Frontmatter;
use regex::Regex;
use std::path::Path;

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
/// use wx_uploader::markdown::parse_markdown;
///
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
/// use wx_uploader::markdown::parse_markdown;
///
/// let content = "# Just a title\n\nSome content.";
/// let (frontmatter, body) = parse_markdown(content).unwrap();
/// assert_eq!(frontmatter.title, None);
/// assert_eq!(body, content);
/// ```
pub fn parse_markdown(content: &str) -> Result<(Frontmatter, String)> {
    parse_markdown_with_site(content, None)
}

/// Parse site fields into canonical upload metadata without changing the source.
pub fn parse_markdown_with_site(
    content: &str,
    site: Option<&crate::site::SiteConfig>,
) -> Result<(Frontmatter, String)> {
    // Use (?s) flag to make . match newlines
    let re = Regex::new(r"(?s)^---\n(.*?)\n---\n(.*)$")?;

    let (mut frontmatter, body) = if let Some(captures) = re.captures(content) {
        let yaml_str = captures.get(1).unwrap().as_str();
        let body = captures.get(2).unwrap().as_str();

        let mapped;
        let yaml_str = if let Some(site) = site {
            let mut value: serde_yaml::Value = serde_yaml::from_str(yaml_str)?;
            if let Some(map) = value.as_mapping_mut() {
                let original = map.clone();
                for (target, source) in [
                    ("title", &site.frontmatter.title),
                    ("description", &site.frontmatter.description),
                    ("cover", &site.frontmatter.cover),
                ] {
                    if target != source {
                        map.remove(serde_yaml::Value::String(target.into()));
                        if target == "description" {
                            map.remove(serde_yaml::Value::String("desc".into()));
                        }
                        if let Some(value) = original.get(serde_yaml::Value::String(source.clone()))
                        {
                            map.insert(serde_yaml::Value::String(target.into()), value.clone());
                        }
                    }
                }
            }
            mapped = serde_yaml::to_string(&value)?;
            &mapped
        } else {
            yaml_str
        };
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml_str)?;

        (frontmatter, body.to_string())
    } else {
        // No frontmatter, create default
        let frontmatter = Frontmatter::default();
        (frontmatter, content.to_string())
    };
    if let Some(site) = site {
        frontmatter.theme = frontmatter.theme.or_else(|| site.theme.clone());
        frontmatter.code = frontmatter.code.or_else(|| site.code.clone());
    }
    frontmatter.validate()?;
    Ok((frontmatter, body))
}

/// Parses a markdown file from a file path
///
/// # Arguments
///
/// * `path` - Path to the markdown file
///
/// # Returns
///
/// A tuple containing the parsed frontmatter and body content
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed
pub async fn parse_markdown_file<P: AsRef<Path>>(path: P) -> Result<(Frontmatter, String)> {
    let path = path.as_ref();
    let content = tokio::fs::read_to_string(path).await?;

    parse_markdown(&content).map_err(|e| match e {
        Error::Yaml(_) => Error::markdown_parse(path, "Failed to parse YAML frontmatter"),
        Error::Regex(_) => Error::markdown_parse(path, "Failed to parse markdown structure"),
        other => other,
    })
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
/// use wx_uploader::{models::Frontmatter, markdown::format_markdown};
///
/// let mut frontmatter = Frontmatter::default();
/// frontmatter.set_title("My Article");
/// frontmatter.set_published("draft");
///
/// let body = "# Hello World\n\nThis is content.";
/// let result = format_markdown(&frontmatter, body).unwrap();
///
/// assert!(result.starts_with("---\n"));
/// assert!(result.contains("title: My Article"));
/// assert!(result.contains("Hello World"));
/// ```
pub fn format_markdown(frontmatter: &Frontmatter, body: &str) -> Result<String> {
    let yaml = serde_yaml::to_string(frontmatter)?;

    Ok(format!("---\n{yaml}---\n{body}"))
}

/// Writes a markdown file with frontmatter to disk
///
/// # Arguments
///
/// * `path` - Path where to write the file
/// * `frontmatter` - The frontmatter to include
/// * `body` - The markdown body content
///
/// # Errors
///
/// Returns an error if the file cannot be written or frontmatter cannot be serialized
pub async fn write_markdown_file<P: AsRef<Path>>(
    path: P,
    frontmatter: &Frontmatter,
    body: &str,
) -> Result<()> {
    let content = format_markdown(frontmatter, body)?;
    let path = path.as_ref();

    tokio::fs::write(path, content).await.map_err(Error::from)
}

/// Updates the frontmatter of a markdown file in place
///
/// This function reads a markdown file, updates its frontmatter with the provided
/// function, and writes it back to the same location.
///
/// # Arguments
///
/// * `path` - Path to the markdown file
/// * `updater` - Function that takes mutable reference to frontmatter for modification
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or written
pub async fn update_frontmatter<P: AsRef<Path>, F>(path: P, updater: F) -> Result<()>
where
    F: FnOnce(&mut Frontmatter) -> Result<()>,
{
    let path = path.as_ref();
    let (mut frontmatter, body) = parse_markdown_file(path).await?;

    updater(&mut frontmatter)?;

    write_markdown_file(path, &frontmatter, &body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Frontmatter;
    use tempfile::TempDir;

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
        let mut frontmatter = Frontmatter::default();
        frontmatter.set_title("Test Title");
        frontmatter.set_published("draft");

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
        let mut frontmatter = Frontmatter::default();
        frontmatter.set_title("Formatting Test");

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
        assert!(error_message.contains("YAML error"));
    }

    #[test]
    fn test_frontmatter_with_cover() {
        let content = r#"---
title: "Article with Cover"
published: "draft"
cover: "my-cover.png"
---
# Article Content
"#;

        let (frontmatter, body) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, Some("Article with Cover".to_string()));
        assert_eq!(frontmatter.published, Some("draft".to_string()));
        assert_eq!(frontmatter.cover, Some("my-cover.png".to_string()));
        assert!(body.contains("Article Content"));
    }

    #[test]
    fn test_frontmatter_without_cover() {
        let content = r#"---
title: "Article without Cover"
published: "draft"
---
# Article Content
"#;

        let (frontmatter, _) = parse_markdown(content).unwrap();

        assert_eq!(frontmatter.title, Some("Article without Cover".to_string()));
        assert_eq!(frontmatter.published, Some("draft".to_string()));
        assert_eq!(frontmatter.cover, None);
    }

    #[test]
    fn test_format_markdown_with_cover() {
        let mut frontmatter = Frontmatter::default();
        frontmatter.set_title("Test Article");
        frontmatter.set_published("draft");
        frontmatter.set_cover("test-cover.png");

        let body = "# Test Content";
        let result = format_markdown(&frontmatter, body).unwrap();

        assert!(result.contains("title: Test Article"));
        assert!(result.contains("published: draft"));
        assert!(result.contains("cover: test-cover.png"));
        assert!(result.contains("# Test Content"));
    }

    #[tokio::test]
    async fn test_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        let mut frontmatter = Frontmatter::default();
        frontmatter.set_title("File Test");
        frontmatter.set_published("draft");

        let body = "# Test Content\n\nThis is a test.";

        // Write file
        write_markdown_file(&file_path, &frontmatter, body)
            .await
            .unwrap();

        // Read file
        let (read_frontmatter, read_body) = parse_markdown_file(&file_path).await.unwrap();

        assert_eq!(frontmatter.title, read_frontmatter.title);
        assert_eq!(frontmatter.published, read_frontmatter.published);
        assert_eq!(body, read_body);
    }

    #[tokio::test]
    async fn test_update_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create initial file
        let mut frontmatter = Frontmatter::default();
        frontmatter.set_title("Original Title");
        let body = "# Content";

        write_markdown_file(&file_path, &frontmatter, body)
            .await
            .unwrap();

        // Update frontmatter
        update_frontmatter(&file_path, |fm| {
            fm.set_title("Updated Title");
            fm.set_published("draft");
            Ok(())
        })
        .await
        .unwrap();

        // Verify update
        let (updated_frontmatter, _) = parse_markdown_file(&file_path).await.unwrap();
        assert_eq!(updated_frontmatter.title, Some("Updated Title".to_string()));
        assert_eq!(updated_frontmatter.published, Some("draft".to_string()));
    }
}
