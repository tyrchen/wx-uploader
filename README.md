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

# Optional: explicitly use the Gemini Developer API (AI Studio) instead of Vertex AI
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
model: nb2  # default: Gemini Nano Banana 2.1; nb: Gemini Pro; gpt: OpenAI
description: Article description
tags: [rust, concurrency]
cover_prompt: |  # optional: describe the scene directly
  Several readers share a large open ledger;
  a writer works alone behind a gate on the right.
cover_style: |  # optional: overrides the default editorial illustration style
  Black-and-white woodcut, clear silhouettes, restrained detail.
author: Author Name
theme: lapis  # optional theme
---

Your markdown content here...
```

## AI Cover Generation

Cover generation defaults to Gemini Nano Banana 2.1 (`gemini-nano-banana-2.1`). Omit `model` or set `model: nb2` in frontmatter. The existing `nb2` alias now selects Nano Banana 2.1. See the [official model documentation](https://ai.google.dev/gemini-api/docs/models/gemini-nano-banana-2.1).

Gemini uses **Vertex AI on Google Cloud with ADC** when `GEMINI_API_KEY` is unset or empty. Requests use `aiplatform.googleapis.com` and Cloud Billing for the selected resource project. Set up local access with these Nushell commands:

```nu
let project = "YOUR_PROJECT_ID"
^gcloud services enable aiplatform.googleapis.com --project $project
^gcloud auth application-default login
^gcloud auth application-default set-quota-project $project
$env.GOOGLE_CLOUD_PROJECT = $project
$env.GOOGLE_CLOUD_LOCATION = "global"
hide-env --ignore-errors GEMINI_API_KEY
```

The Vertex resource project is selected from `GOOGLE_CLOUD_PROJECT`, then `GCLOUD_PROJECT`, then the ADC quota project. Location comes from `GOOGLE_CLOUD_LOCATION`, defaulting to `global`; Nano Banana 2.1 is available on the global endpoint. Your account needs Vertex AI and service usage permissions for the project. Vertex AI uses the `cloud-platform` OAuth scope, which ordinary ADC login grants. Existing ADC with that scope can be reused. See the [Google Cloud ADC guide](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/start/gcp-auth).

The official Google Rust auth library discovers ADC from `GOOGLE_APPLICATION_CREDENTIALS`, the local ADC file, or the Google Cloud metadata service. It obtains, caches, and refreshes short-lived OAuth access tokens automatically; no token export or runtime `gcloud` subprocess is needed. Credentials and their token cache are shared across articles. The ADC quota project is sent as `x-goog-user-project`; override it with `GOOGLE_CLOUD_QUOTA_PROJECT` if needed. For service accounts without an ADC quota project, set `GOOGLE_CLOUD_PROJECT` explicitly. Applicable Google Cloud promotional credits are handled by Cloud Billing for the resource project's linked billing account.

A nonempty `GEMINI_API_KEY` explicitly selects the **Gemini Developer API** at `generativelanguage.googleapis.com`, with AI Studio billing (which may require a prepaid balance). ADC does not select that endpoint. Use `model: nb` for Gemini Pro with the same authentication options, or `model: gpt` for OpenAI's `gpt-image-2` with `OPENAI_API_KEY`.

Cover generation uses two different prompt templates. A nonempty `cover_prompt` directly specifies the scene: the model renders its subjects, actions, and relationships without reinterpreting the article. Without it (or if it is blank), the model designs a cover using only `title`, `description`, and `tags`. `desc` is also accepted as an alias for `description`. Tags may be a YAML list or a string. The article body, section headings, and unrelated frontmatter are excluded. If no scene or subject metadata is provided, generation is skipped with a warning.

`cover_style` applies in both modes and replaces the default visual style, so custom styles such as woodcut or 3D clay are supported. When omitted or blank, the default is a two-dimensional editorial illustration with gouache and screen-print texture. Both templates keep key objects near the center for wide WeChat covers and square thumbnails, and exclude text, logos, watermarks, and borders. Metadata mode asks for one main subject and at most two supporting elements. Scene descriptions are bounded to 4,000 Unicode characters, styles to 1,200; metadata limits are 320 for title, 1,600 for description, and 512 for tags (up to 16 list entries). Normal operation uses one image-model request, with no intermediate summarization model.

OpenAI uses `quality: medium`, `1280x544` (approximately WeChat's 2.35:1 ratio), and lossless PNG output. This avoids the large vertical crop of the previous 1536x1024 output and removes JPEG compression. Gemini uses 21:9 at 1K resolution.

Images are saved next to the Markdown file, and the filename is written into frontmatter. Existing covers are reused; to regenerate, remove the cover file or remove the `cover` field. If generation fails, the tool continues its existing upload flow.

Before upload, local and remote SVG/SVGZ image references are rendered to temporary PNGs with system fonts, at 2x resolution (up to 4096 pixels per edge). This runs even when the cover needs no cropping. Upload-only Markdown and image copies are grouped in a private system temporary directory. Local image paths are resolved from the original article directory before copying, and an unchanged cover uses its original absolute path. The Markdown basename is preserved for Mermaid; the whole temporary directory, including any Mermaid output, is removed after upload. Source SVGs and image references in the original Markdown stay unchanged. Generated covers use the encoding indicated by their filename; existing mismatched covers (such as PNG data named `.jpg`) are normalized in a temporary upload copy.

WeChat digests use `description` (or `desc`), falling back to article text when missing. The upload copy collapses whitespace and truncates long digests with an ellipsis. The final submitted value is bounded to 120 UTF-16 units, counting supplementary characters such as emoji as two and including the ellipsis and any YAML quotes retained by the upload parser. Source descriptions and full metadata used for cover generation stay unchanged.

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

## Site configuration

Place `wx-uploader.yaml` at your site root. The CLI discovers the nearest config
in the input path's ancestors; `--config PATH` overrides discovery. Paths and
content globs are relative to the config directory. Credentials stay in environment
variables. Without a config, the existing frontmatter workflow is unchanged.

For Astro-style sites such as QiaoPang:

```yaml
version: 1
content:
  include: ["src/posts/**/*.md"]
images:
  root: public
  relative_to: root
frontmatter:
  title: title
  description: excerpt
  cover: cover
cover:
  mode: existing
  fit: contain
state:
  storage: sidecar
  path: .wx-uploader/state.json
```

Both `/images/a.svg` and `images/a.svg` resolve under `public/`. HTTP(S) body
images retain their URL behavior. SVG covers and diagrams become temporary PNGs;
WebP body images retain their format. `contain` pads the cover on white to 2.35:1
instead of cropping text or artwork. `existing` fails on a missing cover and never
calls an image generator.

`sidecar` leaves articles and source images unchanged, ignores the site's
`published` field, and records WeChat draft IDs by account and article path. A
directory run skips unchanged articles; source, local image, or configuration
changes create a new draft. Explicit single-file uploads are always forced.
Changes to remote images alone are not detected. Add `.wx-uploader/` to your
site's `.gitignore`; retain the directory locally for deduplication. State is locked
across processes and saved atomically only after upload succeeds.

```sh
# Validate all selected articles without credentials, generation or upload
wx-uploader --check /path/to/site
# Upload one article using the automatically discovered site config
wx-uploader /path/to/site/src/posts/2026/article.md
# Upload all configured articles (the first run includes every selected article)
wx-uploader /path/to/site
```

Offline checks require existing local covers and cannot fetch remote SVGs. Normal
uploads may fetch remote SVGs. For full defaults, constraints and acceptance
criteria, see [the site configuration spec](specs/site-config.md). Custom field
mappings require sidecar state; sidecar state requires existing covers. Configs
reject unknown keys and unsupported versions.
