# wechat-pub-rs

![Build Status](https://github.com/tyrchen/wechat-pub-rs/workflows/CI/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/wechat-pub-rs.svg)](https://crates.io/crates/wechat-pub-rs)
[![Documentation](https://docs.rs/wechat-pub-rs/badge.svg)](https://docs.rs/wechat-pub-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A simple, high-performance WeChat Official Account Rust SDK for uploading articles and managing drafts.

## Features

- **Simple API**: One function to upload entire articles: `client.upload("./article.md").await?`
- **Smart Deduplication**:
  - Images deduplicated by BLAKE3 content hash
  - Drafts deduplicated by title (updates existing drafts)
- **Robust**: Comprehensive error handling and retry mechanisms
- **Fast**: Async/await with concurrent image uploads
- **Type Safe**: Compile-time guarantees and runtime reliability
- **Theme System**: Built-in themes with syntax highlighting support
- **Markdown Support**: Full markdown parsing with frontmatter

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
wechat-pub-rs = "0.2"
tokio = { version = "1.0", features = ["full"] }
```

### Basic Usage

```rust
use wechat_pub_rs::{WeChatClient, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Create client with your WeChat Official Account credentials
    let client = WeChatClient::new("your_app_id", "your_app_secret").await?;

    // Upload markdown file (theme and metadata from frontmatter)
    let draft_id = client.upload("./article.md").await?;

    println!("Draft created with ID: {}", draft_id);
    Ok(())
}
```

### Advanced Usage

```rust
use wechat_pub_rs::{WeChatClient, UploadOptions, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let client = WeChatClient::new("your_app_id", "your_app_secret").await?;

    // Upload with custom options
    let options = UploadOptions::with_theme("lapis")
        .title("Custom Title")
        .author("Custom Author")
        .cover_image("./cover.jpg")
        .show_cover(true)
        .comments(true, false)
        .source_url("https://example.com");

    let draft_id = client.upload_with_options("./article.md", options).await?;

    // Manage drafts
    let draft_info = client.get_draft(&draft_id).await?;
    client.update_draft(&draft_id, "./updated_article.md").await?;

    // List all drafts
    let drafts = client.list_drafts(0, 10).await?;

    println!("Created draft: {}", draft_id);
    Ok(())
}
```

## Markdown Format

Your markdown files should include frontmatter with metadata:

```markdown
---
title: "Your Article Title"
author: "Author Name"
cover: "images/cover.jpg"    # Required: Cover image path
theme: "lapis"               # Optional: Theme name
code: "github"               # Optional: Code highlighting theme
---

# Your Article Content

Your markdown content here with images:

![Alt text](images/example.jpg)

## Code Blocks

```rust
fn main() {
    println!("Hello, WeChat!");
}
```

## Available Themes

| Theme | Description |
|-------|-------------|
| `default` | Simple, clean default theme |
| `lapis` | Blue accents with elegant styling |
| `maize` | Warm yellow tones |
| `orangeheart` | Orange accents |
| `phycat` | Unique phycat styling |
| `pie` | Sweet pie theme |
| `purple` | Purple accents |
| `rainbow` | Colorful rainbow theme |

## Code Highlighting Themes

| Theme | Description |
|-------|-------------|
| `github` | GitHub style (light) |
| `github-dark` | GitHub style (dark) |
| `vscode` | VS Code style |
| `atom-one-light` | Atom One Light |
| `atom-one-dark` | Atom One Dark |
| `solarized-light` | Solarized Light |
| `solarized-dark` | Solarized Dark |
| `monokai` | Monokai |
| `dracula` | Dracula |
| `xcode` | Xcode |

## API Reference

### WeChatClient

#### Main Methods

```rust
// Create a new client
pub async fn new(app_id: impl Into<String>, app_secret: impl Into<String>) -> Result<Self>

// Upload a markdown file
pub async fn upload(&self, markdown_path: &str) -> Result<String>

// Upload with custom options
pub async fn upload_with_options(&self, markdown_path: &str, options: UploadOptions) -> Result<String>
```

### Draft Management

```rust
// Get draft information
pub async fn get_draft(&self, media_id: &str) -> Result<DraftInfo>

// Update existing draft
pub async fn update_draft(&self, media_id: &str, markdown_path: &str) -> Result<()>

// Delete draft
pub async fn delete_draft(&self, media_id: &str) -> Result<()>

// List drafts with pagination
pub async fn list_drafts(&self, offset: u32, count: u32) -> Result<Vec<DraftInfo>>
```

#### Utility Methods

```rust
// Upload single image
pub async fn upload_image(&self, image_path: &str) -> Result<String>

// Get available themes
pub fn available_themes(&self) -> Vec<&String>

// Check if theme exists
pub fn has_theme(&self, theme: &str) -> bool

// Get token info for debugging
pub async fn get_token_info(&self) -> Option<TokenInfo>
```

### UploadOptions

```rust
pub struct UploadOptions {
    pub theme: String,                    // Theme name
    pub title: Option<String>,            // Custom title
    pub author: Option<String>,           // Custom author
    pub cover_image: Option<String>,      // Cover image path
    pub show_cover: bool,                 // Show cover in content
    pub enable_comments: bool,            // Enable comments
    pub fans_only_comments: bool,         // Fans only comments
    pub source_url: Option<String>,       // Source URL
}
```

Builder methods:

```rust
UploadOptions::with_theme("lapis")
    .title("Custom Title")
    .author("Author")
    .cover_image("cover.jpg")
    .show_cover(true)
    .comments(true, false)
    .source_url("https://example.com")
```

## Environment Variables

For running examples, set these environment variables:

```bash
export WECHAT_APP_ID="your_wechat_app_id"
export WECHAT_APP_SECRET="your_wechat_app_secret"
```

## Error Handling

The library provides comprehensive error handling:

```rust
use wechat_pub_rs::{WeChatError, Result};

match client.upload("article.md").await {
    Ok(draft_id) => println!("Success: {}", draft_id),
    Err(WeChatError::FileNotFound { path }) => {
        eprintln!("File not found: {}", path);
    }
    Err(WeChatError::ThemeNotFound { theme }) => {
        eprintln!("Theme not found: {}", theme);
    }
    Err(WeChatError::Network(err)) => {
        eprintln!("Network error: {}", err);
    }
    Err(err) => eprintln!("Other error: {}", err),
}
```

## Performance

- **Concurrent Uploads**: Images are uploaded concurrently (max 5 concurrent)
- **Deduplication**: Images are deduplicated using BLAKE3 hash
- **Memory Efficient**: Streaming file operations
- **Async**: Non-blocking operations throughout

## Requirements

- Rust 1.70+
- WeChat Official Account with API access
- Valid App ID and App Secret

## Examples

Check the `examples/` directory for more usage examples:

```bash
# Run the simple example
WECHAT_APP_ID=your_id WECHAT_APP_SECRET=your_secret cargo run --example simple
```

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Testing

```bash
# Run unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

## License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for a list of changes and version history.

---

**Note**: This SDK focuses specifically on article publishing to WeChat Official Accounts. For other WeChat functionalities (messaging, user management, etc.), consider using a more comprehensive WeChat SDK.
