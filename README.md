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

# Optional: Gemini API key for default automatic cover image generation
export GEMINI_API_KEY="your_gemini_api_key"

# Optional: OpenAI API key for `model: gpt`
export OPENAI_API_KEY="your_openai_api_key"
```

You can get `WECHAT_APP_ID` and `WECHAT_APP_SECRET` from the WeChat developer console:
[https://developers.weixin.qq.com/console](https://developers.weixin.qq.com/console)

If you see an error like:

```text
WeChat API error: WeChat upload failed: WeChat API error [40164]: invalid ip <ipv4 address> ipv6 <ipv6 address>, not in whitelist rid
```

add your current API IP whitelist entry in the same console before retrying.

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
3. If no cover image is specified, the tool generates one with the selected image model. By default it uses `nb2` (Gemini Flash); you can also choose `nb` (Gemini Pro) or `gpt` (OpenAI) in frontmatter
4. When specifying a single file, it will be uploaded regardless of its publish status
5. After successful upload, the frontmatter is updated with `published: draft` and the cover filename (if generated)

## Frontmatter Example

```yaml
---
title: My Article Title
published: draft  # or 'true' to skip upload
cover: cover.png  # optional, auto-generated if missing and the selected model key is set
model: nb2        # optional, defaults to nb2; available: nb2, nb, gpt
description: Article description
author: Author Name
theme: lapis  # optional theme
---

Your markdown content here...
```

## AI Cover Generation

When the `GEMINI_API_KEY` environment variable is set, the tool will automatically generate cover images for articles that don't have one specified. Gemini is the default backend. If you prefer OpenAI for a specific article, set `model: gpt` in that file's frontmatter and provide `OPENAI_API_KEY`.

### Model Selection

- `nb2` (default): Gemini Flash image model
- `nb`: Gemini Pro image model
- `gpt`: OpenAI image generation

### How it works

1. **Context Building**: Uses the article title and description as the image-generation context
2. **Model Routing**: Resolves the backend from frontmatter `model`, defaulting to `nb2` when omitted
3. **Image Generation**: Gemini or OpenAI generates a high-quality 16:9 cover image
4. **Auto-Save**: Downloads and saves the image in the same directory as your markdown file
5. **Metadata Update**: Updates your frontmatter with the generated cover filename

### Features

- **Multiple Backends**: Gemini by default, with optional OpenAI per article
- **Content-Aware**: Prompts are built from your article title and description
- **High Quality**: Generates wide 16:9 cover images optimized for article thumbnails
- **Automatic Naming**: Generated files use unique names to prevent conflicts
- **Graceful Fallback**: Continues normal upload process if image generation fails
- **Base64 Support**: Handles both URL and base64-encoded image responses

### Example Output

For an article about "Building Rust Applications", the tool uses the title and description to generate a wide-format cover image that visually represents the article's core idea.

## Features

- 📝 **Batch Upload**: Process entire directories of markdown files
- 🎨 **AI Cover Generation**: Automatic cover images using Gemini by default, with optional OpenAI
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
