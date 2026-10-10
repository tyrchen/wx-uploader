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

# Optional: OpenAI cover generation (frontmatter model: gpt)
export OPENAI_API_KEY="your_openai_api_key"

# Optional: Gemini cover generation (model: nb2, default, or nb)
export GEMINI_API_KEY="your_gemini_api_key"
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
3. If the cover is missing, generates an editorial illustration using the selected image model and its API key
4. When specifying a single file, it will be uploaded regardless of its publish status
5. After successful upload, the frontmatter is updated with `published: draft` and the cover filename (if generated)

## Frontmatter Example

```yaml
---
title: My Article Title
published: draft  # or 'true' to skip upload
cover: cover.png  # optional; a missing image is generated
model: gpt  # OpenAI; nb2 (default) and nb use Gemini
description: Article description
author: Author Name
theme: lapis  # optional theme
---

Your markdown content here...
```

## AI Cover Generation

Set `model: gpt` in frontmatter and configure `OPENAI_API_KEY` to use OpenAI's `gpt-image-2`. Gemini remains available through `nb2` (default) and `nb`, with `GEMINI_API_KEY`.

The title, description, section headings, and article prose are sent directly to the image model in one request. Code blocks and embedded image links are removed. Long articles retain excerpts from the beginning, middle, and conclusion within a Unicode character budget, so the model can use the argument rather than just the title.

The prompt asks for a specific visual mechanism grounded in the article, with one focal subject and a consistent editorial illustration style. It keeps the important objects near the center for both the wide WeChat cover and square thumbnail, and excludes text, logos, and generic technology decoration.

OpenAI uses `quality: medium`, `1280x544` (approximately WeChat's 2.35:1 ratio), and lossless PNG output. This avoids the large vertical crop of the previous 1536x1024 output and removes JPEG compression. Gemini uses 21:9 at its existing 2K setting.

Images are saved next to the Markdown file, and the filename is written into frontmatter. Existing covers are reused; to regenerate, remove the cover file or remove the `cover` field. If generation fails, the tool continues its existing upload flow.

For an article about Rust borrowing, a suitable concept would show several reading paths sharing one resource, contrasted with a single exclusive writing path. This depicts the borrowing mechanism rather than using a generic workshop or gears.

## Features

- 📝 **Batch Upload**: Process entire directories of markdown files
- 🎨 **AI Cover Generation**: Automatic cover images using OpenAI or Gemini
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
