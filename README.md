# wx-uploader

A command-line tool to upload markdown files to WeChat public account with automatic AI-powered cover image generation. See [README_CN.md](README_CN.md) for Chinese version.

## Installation

Install directly from crates.io:

```bash
cargo install wx-uploader
```

Or build from source:

```bash
git clone https://github.com/tyrchen/wx-uploader.git
cd wx-uploader
cargo install --path .
```

## Prerequisites

Before using this tool, you need to set up the following environment variables:

```bash
# Required: WeChat public account credentials
export WECHAT_APP_ID="your_app_id"
export WECHAT_APP_SECRET="your_app_secret"

# Optional: OpenAI API key for automatic cover image generation
export OPENAI_API_KEY="your_openai_api_key"
```

## Usage

### Upload all markdown files in a directory

```bash
# Upload all .md files that don't have `published: true` in their frontmatter
wx-uploader .

# Upload from a specific directory
wx-uploader ./posts

# Enable verbose output
wx-uploader --verbose ./posts
```

### Upload a specific file

```bash
# Force upload a specific file (ignores publish status)
wx-uploader ./2025/08/01-chat-with-ai.md
```

## How it works

1. The tool scans for markdown files with YAML frontmatter
2. If a file doesn't have `published: true` in its frontmatter, it will be uploaded
3. If no cover image is specified and OpenAI API key is available, generates a Studio Ghibli-style cover image using GPT-5 and gpt-image-2
4. When specifying a single file, it will be uploaded regardless of its publish status
5. After successful upload, the frontmatter is updated with `published: draft` and the cover filename (if generated)

## Frontmatter Example

```yaml
---
title: My Article Title
published: draft  # or 'true' to skip upload
cover: cover.png  # optional, auto-generated if missing and OpenAI key is set
description: Article description
author: Author Name
theme: lapis  # optional theme
---

Your markdown content here...
```

## AI Cover Generation

When the `OPENAI_API_KEY` environment variable is set, the tool will automatically generate beautiful cover images for articles that don't have one specified.

### How it works

1. **Content Analysis**: GPT-5-mini analyzes your markdown content to create a vivid scene description
2. **Prompt Generation**: Creates an optimized prompt for image generation focusing on Studio Ghibli-style artwork
3. **Image Generation**: gpt-image-2 generates a high-quality 16:9 aspect ratio cover image
4. **Auto-Save**: Downloads and saves the image in the same directory as your markdown file
5. **Metadata Update**: Updates your frontmatter with the generated cover filename

### Features

- **Studio Ghibli Style**: Beautiful, artistic aesthetic with soft colors and natural elements
- **Content-Aware**: Scene descriptions are based on your actual article content
- **High Quality**: 1536x1024 resolution images optimized for web display
- **Automatic Naming**: Generated files use unique names to prevent conflicts
- **Graceful Fallback**: Continues normal upload process if image generation fails
- **Base64 Support**: Handles both URL and base64-encoded image responses

### Example Output

For an article about "Building Rust Applications", the AI might generate a scene like:
> "A cozy workshop filled with intricate gears and glowing mechanical tools, where a craftsman carefully assembles clockwork mechanisms. Warm golden light streams through tall windows, illuminating floating rust particles that sparkle like fireflies in the dusty air."

This becomes a beautiful Studio Ghibli-style cover image that visually represents your content.

## Features

- 📝 **Batch Upload**: Process entire directories of markdown files
- 🎨 **AI Cover Generation**: Automatic cover images using OpenAI's latest models
- 🔄 **Smart Processing**: Skip already published articles
- 📊 **Progress Tracking**: Clear console output with colored status indicators
- 🛡️ **Error Recovery**: Graceful handling of API failures
- 🔐 **Secure**: API keys stored in environment variables only

## Development

### Running Tests

The project includes comprehensive unit and integration tests:

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test module
cargo test test_frontmatter

# Run integration tests only
cargo test --test integration_tests
```

### Code Quality

```bash
# Run clippy for linting
cargo clippy --all-targets --all-features

# Check for security vulnerabilities
cargo audit

# Format code
cargo fmt

# Generate documentation
cargo doc --open
```

### Project Structure

```
wx-uploader/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── lib.rs           # Public API
│   ├── cli.rs           # Command-line interface
│   ├── error.rs         # Error handling
│   ├── models.rs        # Data structures
│   ├── markdown.rs      # Markdown parsing
│   ├── openai.rs        # AI integration
│   ├── output.rs        # Console output formatting
│   └── wechat.rs        # WeChat API integration
└── tests/
    └── integration_tests.rs  # Integration tests
```

## Notes

- Files with `published: true` will be skipped during directory scans
- Single file uploads always force upload regardless of publish status
- The tool preserves all other frontmatter fields when updating
- Cover images are saved in the same directory as the markdown file
- Supports both string (`"true"`) and boolean (`true`) values for the published field

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
