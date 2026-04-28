# Spec: Multi-Model Image Generation

## Overview

Replace the hardcoded OpenAI image generation with a pluggable backend supporting Gemini (default) and OpenAI. Users select the model via frontmatter `model` field.

## Model Aliases

| Alias           | API Model ID                     | Provider      | Notes                       |
| --------------- | -------------------------------- | ------------- | --------------------------- |
| `nb2` (default) | `gemini-3.1-flash-image-preview` | Google Gemini | Fast, cheap, strong quality |
| `nb`            | `gemini-3-pro-image-preview`     | Google Gemini | Highest quality             |
| `gpt`           | `gpt-image-2`                    | OpenAI        | Current behavior            |

## Frontmatter Change

Add optional `model` field to `Frontmatter`:

```yaml
---
title: "My Article"
model: nb2        # optional, defaults to "nb2"
cover: cover.png
---
```

Valid values: `nb2`, `nb`, `gpt`. Invalid values produce a validation error (same pattern as `theme`/`code` validation).

## Environment Variables

| Variable         | Required When              |
| ---------------- | -------------------------- |
| `GEMINI_API_KEY` | Using `nb2` or `nb` models |
| `OPENAI_API_KEY` | Using `gpt` model          |

`Config` struct gains `gemini_api_key: Option<String>`, read from `GEMINI_API_KEY` in `Config::from_env()`.

At startup, if neither key is set, cover generation is skipped (current behavior for missing `OPENAI_API_KEY`). If a frontmatter specifies a model whose key is missing, emit an error and skip generation for that file.

## Architecture

### New trait: `CoverImageGenerator`

Replace the current tight coupling to `OpenAIClient` with a single unified trait:

```rust
#[async_trait::async_trait]
pub trait CoverImageGenerator: Send + Sync {
    /// Generate a cover image from article content, save to `target_path`.
    /// Returns the bytes written.
    async fn generate_cover(&self, content: &str, target_path: &Path) -> Result<()>;
}
```

This trait combines the current 3-step flow (scene description → prompt → image generation → download) into one method. Each backend implements the full pipeline internally since the steps differ significantly between providers.

### New module: `src/gemini.rs`

Implements `CoverImageGenerator` for Gemini. Key design decisions:

- **Single API call**: Gemini's image generation models accept text prompts and return images directly via `generateContent`. No separate "scene description" step is needed — the model handles it in one shot.
- **Request format**: POST to `https://generativelanguage.googleapis.com/v1beta/models/{model_id}:generateContent` with `x-goog-api-key` header.
- **Request body**:
  ```json
  {
    "contents": [{"parts": [{"text": "<prompt>"}], "role": "user"}],
    "generationConfig": {
      "responseModalities": ["TEXT", "IMAGE"],
      "imageConfig": {
        "aspectRatio": "16:9",
        "imageSize": "2K"
      }
    }
  }
  ```
- **Prompt**: Use the same Ghibli-style prompt template, but include the article content (truncated to 2000 chars) directly: `"Based on this article, create a wide Ghibli-style cover image: {content}"`.
- **Response handling**: Extract `inlineData` parts, base64-decode, write to disk. Retry once if model returns text-only response (same pattern as reference code).
- **Retry/backoff**: Exponential backoff on 429/5xx, max 5 retries, initial 2s backoff, max 60s. Respect `Retry-After` header.
- **No rate limiter needed**: wx-uploader processes files sequentially, so RPM limiting is unnecessary (unlike the reference code which runs batch operations).

```rust
pub struct GeminiClient {
    api_key: String,
    http_client: Client,
    model_id: &'static str,  // "gemini-3.1-flash-image-preview" or "gemini-3-pro-image-preview"
}
```

### Refactor `src/openai.rs`

- Implement `CoverImageGenerator` for `OpenAIClient` by wrapping the existing 3-step flow (scene description → DALL-E prompt → image generation).
- Keep existing `SceneDescriptionGenerator`, `ImageGenerator`, `PromptBuilder` traits and impls as internal details.

### Changes to `src/models.rs`

1. Add `model` field to `Frontmatter`:
   ```rust
   #[serde(skip_serializing_if = "Option::is_none")]
   pub model: Option<String>,
   ```

2. Add validation for `model` field in `Frontmatter::validate()`, same pattern as theme/code validation.

3. Add `VALID_MODELS` constant:
   ```rust
   pub const VALID_MODELS: &[&str] = &["nb2", "nb", "gpt"];
   ```

4. Add `gemini_api_key: Option<String>` to `Config`, read from `GEMINI_API_KEY`.

5. Add helper:
   ```rust
   impl Frontmatter {
       /// Returns the effective model alias, defaulting to "nb2"
       pub fn effective_model(&self) -> &str {
           self.model.as_deref().unwrap_or("nb2")
       }
   }
   ```

### Changes to `src/wechat.rs`

The `process_cover_image` function currently takes `Option<&OpenAIClient>`. Change it to resolve the right client per-file based on frontmatter:

```rust
async fn process_cover_image(
    frontmatter: &mut Frontmatter,
    path: &Path,
    config: &Config,
    verbose: bool,
) -> Result<bool> {
    // ... existing should_generate_cover check ...

    let model = frontmatter.effective_model();
    let generator: Box<dyn CoverImageGenerator> = match model {
        "nb2" | "nb" => {
            let api_key = config.gemini_api_key.as_ref()
                .ok_or_else(|| Error::config("GEMINI_API_KEY required for Gemini models"))?;
            let model_id = match model {
                "nb" => "gemini-3-pro-image-preview",
                _ => "gemini-3.1-flash-image-preview",
            };
            Box::new(GeminiClient::new(api_key.clone(), model_id))
        }
        "gpt" => {
            let api_key = config.openai_api_key.as_ref()
                .ok_or_else(|| Error::config("OPENAI_API_KEY required for GPT model"))?;
            Box::new(OpenAIClient::new(api_key.clone()))
        }
        _ => unreachable!("validated in frontmatter"),
    };

    // ... use generator.generate_cover(...) ...
}
```

This means `process_file` and `process_directory` pass `&Config` instead of `Option<&OpenAIClient>`. The OpenAI client construction in `main.rs` is removed — clients are now created on-demand per file based on frontmatter.

### Changes to `src/main.rs`

- Remove `OpenAIClient` construction at startup.
- Pass `&config` to upload functions instead of `Option<&OpenAIClient>`.
- Read `GEMINI_API_KEY` into config (already handled by `Config::from_env()` change).

### Changes to `src/error.rs`

Add `Gemini` variant to the error enum:
```rust
#[error("Gemini API error: {0}")]
Gemini(String),
```

### Changes to `src/output.rs`

- Update image generation status messages to include model name: `"Generating cover image (nb2)..."`.
- Add Gemini-specific error formatting if needed.

## Cargo.toml

No new crate dependencies needed. The Gemini API is simple enough to call with `reqwest` + `serde_json` + `base64` (all already in deps).

## Migration & Compatibility

- **No breaking changes**: Existing articles without `model` field default to `nb2` (Gemini Flash), which is a behavior change from the current `gpt` default. This is intentional — user explicitly wants Gemini as default.
- **Existing `OPENAI_API_KEY`-only users**: If they don't set `GEMINI_API_KEY`, cover generation will fail with a clear error telling them to either set `GEMINI_API_KEY` or add `model: gpt` to frontmatter. This is acceptable since the user (sole maintainer) is driving this change.

## File Changes Summary

| File            | Change                                                                       |
| --------------- | ---------------------------------------------------------------------------- |
| `src/models.rs` | Add `model` field to `Frontmatter`, `gemini_api_key` to `Config`, validation |
| `src/gemini.rs` | **New file** — `GeminiClient` implementing `CoverImageGenerator`             |
| `src/openai.rs` | Implement `CoverImageGenerator` trait, keep internals                        |
| `src/wechat.rs` | Replace `Option<&OpenAIClient>` with `&Config`, resolve client per-file      |
| `src/main.rs`   | Remove OpenAI client construction, pass config                               |
| `src/error.rs`  | Add `Gemini` error variant                                                   |
| `src/output.rs` | Model name in status messages                                                |
| `src/lib.rs`    | Add `pub mod gemini;`                                                        |
| `Cargo.toml`    | No changes needed                                                            |
