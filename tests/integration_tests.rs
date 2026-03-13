use std::fs;
use tempfile::TempDir;
use wx_uploader::{
    error::Result,
    markdown::{parse_markdown_file, write_markdown_file},
    models::Frontmatter,
    wechat::resolve_and_check_cover_path,
};

/// Integration tests for file processing functionality.
/// These tests focus on file I/O and directory traversal without
/// making actual WeChat API calls.

#[tokio::test]
async fn test_markdown_parsing_and_frontmatter_handling() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_article.md");

    let original_content = r#"---
title: "Integration Test Article"
published: "draft"
description: "A test article for integration testing"
cover: "test_cover.png"
---
# Test Article

This is a test article for integration testing.

## Features

- Markdown parsing
- Frontmatter handling
- File I/O operations
"#;

    // Write the test file
    fs::write(&file_path, original_content).unwrap();

    // Parse the markdown file
    let (frontmatter, body) = parse_markdown_file(&file_path).await?;

    // Verify frontmatter parsing
    assert_eq!(
        frontmatter.title,
        Some("Integration Test Article".to_string())
    );
    assert_eq!(frontmatter.published, Some("draft".to_string()));
    assert_eq!(
        frontmatter.description,
        "A test article for integration testing"
    );
    assert_eq!(frontmatter.cover, Some("test_cover.png".to_string()));

    // Verify body parsing
    assert!(body.contains("# Test Article"));
    assert!(body.contains("## Features"));
    assert!(body.contains("- Markdown parsing"));

    // Test frontmatter modification
    let mut modified_frontmatter = frontmatter.clone();
    modified_frontmatter.set_published("true");
    modified_frontmatter.set_cover("new_cover.png".to_string());

    // Write back the modified file
    write_markdown_file(&file_path, &modified_frontmatter, &body).await?;

    // Re-parse to verify changes
    let (updated_frontmatter, updated_body) = parse_markdown_file(&file_path).await?;
    assert_eq!(updated_frontmatter.published, Some("true".to_string()));
    assert_eq!(updated_frontmatter.cover, Some("new_cover.png".to_string()));
    assert_eq!(updated_body, body); // Body should remain unchanged

    Ok(())
}

#[test]
fn test_cover_image_path_resolution() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create a markdown file
    let md_file = base_path.join("article.md");
    fs::write(&md_file, "# Test Article").unwrap();

    // Create directory structure with cover images
    let images_dir = base_path.join("images");
    fs::create_dir(&images_dir).unwrap();

    let existing_cover = images_dir.join("existing_cover.png");
    fs::write(&existing_cover, "fake image data").unwrap();

    // Test with existing relative path
    let (resolved_path, exists) =
        resolve_and_check_cover_path(&md_file, "images/existing_cover.png");
    assert_eq!(resolved_path, existing_cover);
    assert!(exists);

    // Test with non-existing relative path
    let (resolved_path, exists) =
        resolve_and_check_cover_path(&md_file, "images/missing_cover.png");
    assert_eq!(resolved_path, images_dir.join("missing_cover.png"));
    assert!(!exists);

    // Test with absolute path
    let abs_path = existing_cover.to_string_lossy().to_string();
    let (resolved_path, exists) = resolve_and_check_cover_path(&md_file, &abs_path);
    assert_eq!(resolved_path, existing_cover);
    assert!(exists);

    // Test with simple filename (should resolve relative to markdown file)
    let simple_cover = base_path.join("simple_cover.png");
    fs::write(&simple_cover, "simple cover data").unwrap();

    let (resolved_path, exists) = resolve_and_check_cover_path(&md_file, "simple_cover.png");
    assert_eq!(resolved_path, simple_cover);
    assert!(exists);
}

#[test]
fn test_cover_path_resolution_without_file() {
    let temp_dir = TempDir::new().unwrap();
    let md_file = temp_dir.path().join("test.md");
    fs::write(&md_file, "# Test Article").unwrap();

    // Test cover path resolution
    let (path, exists) = resolve_and_check_cover_path(&md_file, "test_cover.png");
    assert_eq!(path, temp_dir.path().join("test_cover.png"));
    assert!(!exists);
}

#[tokio::test]
async fn test_markdown_file_discovery_comprehensive() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create complex directory structure
    let dirs = [
        "articles",
        "articles/tech",
        "articles/personal",
        "docs",
        "docs/tutorials",
        "drafts",
        "drafts/ideas",
    ];

    for dir in &dirs {
        fs::create_dir_all(base_path.join(dir)).unwrap();
    }

    // Create markdown files with different frontmatter states
    let files = [
        (
            "articles/published.md",
            "---\ntitle: Published\npublished: true\ndescription: Published article\n---\n# Published",
        ),
        (
            "articles/draft.md",
            "---\ntitle: Draft\npublished: draft\ndescription: Draft article\n---\n# Draft",
        ),
        (
            "articles/unpublished.md",
            "---\ntitle: Unpublished\ndescription: Unpublished article\n---\n# Unpublished",
        ),
        (
            "articles/tech/advanced.md",
            "---\ntitle: Advanced\npublished: false\ndescription: Advanced article\n---\n# Advanced",
        ),
        (
            "articles/personal/story.md",
            "---\ntitle: Story\ndescription: Personal story\n---\n# Personal Story",
        ),
        ("docs/readme.md", "# Documentation"),
        (
            "docs/tutorials/guide.md",
            "---\ntitle: Guide\ndescription: Tutorial guide\n---\n# Tutorial Guide",
        ),
        (
            "drafts/ideas/concept.md",
            "---\ntitle: Concept\ncover: concept_cover.png\ndescription: Concept article\n---\n# Concept",
        ),
    ];

    for (file_path, content) in &files {
        fs::write(base_path.join(file_path), content).unwrap();
    }

    // Create non-markdown files that should be ignored
    fs::write(base_path.join("articles/readme.txt"), "Not markdown").unwrap();
    fs::write(base_path.join("docs/config.json"), "{}").unwrap();
    fs::write(base_path.join("drafts/.hidden.md"), "# Hidden file").unwrap();

    // Use walkdir to find markdown files (same logic as main application)
    let mut md_files = Vec::new();
    for entry in walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
    {
        md_files.push(entry.path().to_path_buf());
    }

    // Should find exactly 8 markdown files (including the .hidden.md file)
    assert_eq!(md_files.len(), 9);

    // Verify specific files are found
    let file_names: Vec<String> = md_files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

    assert!(file_names.contains(&"published.md".to_string()));
    assert!(file_names.contains(&"draft.md".to_string()));
    assert!(file_names.contains(&"unpublished.md".to_string()));
    assert!(file_names.contains(&"advanced.md".to_string()));
    assert!(file_names.contains(&"story.md".to_string()));
    assert!(file_names.contains(&"readme.md".to_string()));
    assert!(file_names.contains(&"guide.md".to_string()));
    assert!(file_names.contains(&"concept.md".to_string()));
    assert!(file_names.contains(&".hidden.md".to_string()));

    // Test parsing different frontmatter states
    for (file_path, _) in &files {
        let full_path = base_path.join(file_path);
        let parse_result = parse_markdown_file(&full_path).await;
        assert!(parse_result.is_ok(), "Failed to parse {}", file_path);

        let (frontmatter, _body) = parse_result.unwrap();

        // Verify specific frontmatter properties based on file content
        match *file_path {
            "articles/published.md" => {
                assert!(
                    frontmatter.is_published(),
                    "File should be published: {:?}",
                    file_path
                );
            }
            "articles/draft.md" => {
                assert!(!frontmatter.is_published()); // "draft" is not considered published
            }
            "articles/tech/advanced.md" => {
                assert!(!frontmatter.is_published()); // "false" is not considered published
            }
            "drafts/ideas/concept.md" => {
                assert_eq!(frontmatter.cover, Some("concept_cover.png".to_string()));
            }
            _ => {
                // Other files should not be marked as published
                assert!(!frontmatter.is_published());
            }
        }
    }
}

#[test]
fn test_frontmatter_publication_states() {
    // Test different publication states
    let test_cases = [
        ("true", true),
        ("\"true\"", true),
        ("false", false),
        ("\"false\"", false),
        ("draft", false),
        ("\"draft\"", false),
        ("pending", false),
        ("", false),
    ];

    for (published_value, expected_published) in &test_cases {
        let mut frontmatter = Frontmatter::default();
        frontmatter.set_published(*published_value);

        assert_eq!(
            frontmatter.is_published(),
            *expected_published,
            "Failed for published value: '{}'",
            published_value
        );
    }
}

#[tokio::test]
async fn test_file_error_scenarios() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Test parsing invalid markdown file
    let invalid_file = base_path.join("invalid.md");
    fs::write(
        &invalid_file,
        "---\ninvalid yaml: [unclosed\n---\n# Content",
    )
    .unwrap();

    let result = parse_markdown_file(&invalid_file).await;
    assert!(
        result.is_err(),
        "Should fail to parse invalid YAML frontmatter"
    );

    // Test parsing file with no frontmatter
    let no_frontmatter_file = base_path.join("simple.md");
    fs::write(
        &no_frontmatter_file,
        "# Simple Markdown\n\nNo frontmatter here.",
    )
    .unwrap();

    let result = parse_markdown_file(&no_frontmatter_file).await;
    assert!(result.is_ok(), "Should handle files without frontmatter");

    let (frontmatter, body) = result.unwrap();
    assert!(frontmatter.title.is_none());
    assert!(!frontmatter.is_published());
    assert!(body.contains("# Simple Markdown"));

    // Test parsing non-existent file
    let non_existent = base_path.join("does_not_exist.md");
    let result = parse_markdown_file(&non_existent).await;
    assert!(result.is_err(), "Should fail to parse non-existent file");
}

#[test]
fn test_directory_vs_file_detection_edge_cases() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Test file with .md in the middle of filename
    let weird_name = base_path.join("file.md.backup");
    fs::write(&weird_name, "# Backup file").unwrap();

    // Should not be detected as markdown file
    let md_files: Vec<_> = walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md") && e.path().is_file()
        })
        .collect();

    assert_eq!(md_files.len(), 0);

    // Test directory named with .md extension
    let md_dir = base_path.join("directory.md");
    fs::create_dir(&md_dir).unwrap();

    // Should not be detected as markdown file (directory with .md extension)
    let md_files: Vec<_> = walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md") && e.path().is_file()
        })
        .collect();

    assert_eq!(md_files.len(), 0);

    // Test actual .md file
    let real_md = base_path.join("real.md");
    fs::write(&real_md, "# Real markdown").unwrap();

    let md_files: Vec<_> = walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().and_then(|s| s.to_str()) == Some("md") && e.path().is_file()
        })
        .collect();

    assert_eq!(md_files.len(), 1);
    assert_eq!(md_files[0].path().file_name().unwrap(), "real.md");
}

#[test]
fn test_unicode_and_special_characters_in_paths() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Test files with unicode characters
    let unicode_files = [
        "测试文章.md",
        "article_with_émojis_🚀.md",
        "файл.md",
        "artículo.md",
    ];

    for filename in &unicode_files {
        let file_path = base_path.join(filename);
        // Some filesystems may not support all unicode characters
        if let Ok(()) = fs::write(&file_path, "# Unicode test") {
            assert!(file_path.exists());

            // Test path resolution with unicode
            let (resolved, exists) = resolve_and_check_cover_path(&file_path, "cover.png");
            assert_eq!(resolved, base_path.join("cover.png"));
            assert!(!exists);
        }
    }
}

#[tokio::test]
async fn test_concurrent_file_operations() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Create multiple files for concurrent processing
    let files = (0..10)
        .map(|i| {
            let filename = format!("concurrent_test_{}.md", i);
            let filepath = base_path.join(&filename);
            let content = format!("---\ntitle: Test {}\n---\n# Concurrent Test {}", i, i);
            fs::write(&filepath, content).unwrap();
            filepath
        })
        .collect::<Vec<_>>();

    // Process files concurrently
    let tasks = files.into_iter().map(|filepath| {
        tokio::spawn(async move {
            let result = parse_markdown_file(&filepath).await;
            assert!(result.is_ok());
            let (frontmatter, _body) = result.unwrap();
            assert!(frontmatter.title.is_some());
        })
    });

    // Wait for all concurrent operations to complete
    for task in tasks {
        task.await.unwrap();
    }
}
