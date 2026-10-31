# Spec: Multi-Model Image Generation

## Overview

Replace the hardcoded OpenAI image generation with a pluggable backend supporting Gemini (default) and OpenAI. Users select the model via frontmatter `model` field.

## Model Aliases

| Alias           | API Model ID                     | Provider      | Notes                       |
| --------------- | -------------------------------- | ------------- | --------------------------- |
| `nb2` (default) | `gemini-nano-banana-2.1`         | Google Gemini | Nano Banana 2.1             |
| `nb`            | `gemini-3-pro-image-preview`     | Google Gemini | Highest quality             |
| `gpt`           | `gpt-image-2`                    | OpenAI        | Current behavior            |

## Frontmatter Change

Add optional `model` field to `Frontmatter`:

```yaml
---
title: "My Article"
model: nb2        # optional, defaults to "nb2"
cover: cover.png
cover_prompt: |  # optional explicit scene; overrides metadata-based composition
  Several readers share an open ledger, while a writer works alone behind a gate.
cover_style: Black-and-white woodcut  # optional visual style for either mode
tags: [rust, concurrency]
---
```

Valid values: `nb2`, `nb`, `gpt`. Invalid values produce a validation error (same pattern as `theme`/`code` validation).

## Environment Variables

| Variable         | Required When              |
| ---------------- | -------------------------- |
| `GEMINI_API_KEY` | Optional explicit Gemini Developer API key; otherwise use Vertex AI with ADC |
| `GOOGLE_CLOUD_PROJECT` | Optional Vertex resource project; then GCLOUD_PROJECT, then ADC quota project |
| `GOOGLE_CLOUD_LOCATION` | Optional Vertex location, default global; Nano Banana 2.1 uses global |
| `GOOGLE_APPLICATION_CREDENTIALS` | Optional ADC credentials file; otherwise discover local ADC or metadata service |
| `GOOGLE_CLOUD_QUOTA_PROJECT` | Optional override for the ADC quota project |
| `OPENAI_API_KEY` | Using `gpt` model          |

`Config` struct gains `gemini_api_key: Option<String>`, read from `GEMINI_API_KEY` in `Config::from_env()`.

For Gemini, a nonempty `GEMINI_API_KEY` explicitly selects the Gemini Developer API and AI Studio billing; otherwise use Vertex AI with Application Default Credentials (ADC). Load credentials lazily on the first generation request. The official Google Rust auth library obtains, caches, and refreshes short-lived OAuth access tokens. Share credentials across articles and both Gemini aliases, and fetch authentication headers on every request attempt. Preserve the `x-goog-user-project` quota header. Do not shell out to `gcloud` or require a manually exported access token. ADC selects the Vertex endpoint; it does not use the Gemini Developer API.

Local setup: enable `aiplatform.googleapis.com`, run ordinary `gcloud auth application-default login`, and set the quota project with `gcloud auth application-default set-quota-project PROJECT_ID`. Vertex AI needs the `cloud-platform` scope and appropriate project IAM permissions. Existing ADC with that scope can be reused. Select the resource project from `GOOGLE_CLOUD_PROJECT`, then `GCLOUD_PROJECT`, then the ADC quota header. Default the location to `global`. Cloud Billing handles applicable GCP credits for the resource project's billing account. Google Cloud deployments can use metadata service credentials; set `GOOGLE_CLOUD_PROJECT` when no quota project is available. Missing or unusable credentials produce an authentication error and cover generation is skipped for that file, following the existing upload behavior. OpenAI still requires `OPENAI_API_KEY` for `model: gpt`.

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

The shared prompt builder selects the scene or metadata template before backend dispatch. Each backend sends the complete prompt directly to its image model and saves the returned image.

### New module: `src/gemini.rs`

Implements `CoverImageGenerator` for Gemini. Key design decisions:

- **Single API call**: Gemini's image generation models accept text prompts and return images directly via `generateContent`. No separate "scene description" step is needed — the model handles it in one shot.
- **Request format**: ADC uses `https://aiplatform.googleapis.com/v1/projects/{project}/locations/global/publishers/google/models/{model_id}:generateContent` with Bearer authentication and quota headers. For regional Vertex locations use `{location}-aiplatform.googleapis.com`. An explicit API key uses `https://generativelanguage.googleapis.com/v1beta/models/{model_id}:generateContent` with `x-goog-api-key`.
- **Request body**:
  ```json
  {
    "contents": [{"parts": [{"text": "<prompt>"}], "role": "user"}],
    "generationConfig": {
      "responseModalities": ["TEXT", "IMAGE"],
      "imageConfig": {
        "aspectRatio": "16:9",
        "imageSize": "1K"
      }
    }
  }
  ```
- **Prompt selection**: A nonempty `cover_prompt` selects a direct-rendering template, using the supplied scene without reinterpreting title/description/tags. Otherwise a separate template constructs a visual from only `title`, `description`, and `tags`; body and headings never enter the image request. A blank `cover_prompt` falls back to metadata mode. Tags accept a string or a list of strings, preserving their existing flattened frontmatter format. If no scene or subject metadata exists, warn and skip generation.
- **Style and composition**: `cover_style` replaces the default two-dimensional editorial/gouache/screen-print style in either mode; it must not be contradicted by default-style exclusions. Both templates enforce centered wide-cover composition and no text/logos/watermarks/borders. Bound Unicode characters: scene 4,000, style 1,200, title 320, description 1,600, tags 512 (up to 16 list entries). Build the complete prompt once before selecting a provider; Gemini and OpenAI clients transmit it without adding a second template. Keep one image-model request; no preprocessing model is needed.
- **Response handling**: Extract `inlineData` parts, base64-decode, write to disk. Retry once if model returns text-only response (same pattern as reference code).
- **Retry/backoff**: Exponential backoff on 429/5xx, max 5 retries, initial 2s backoff, max 60s. Respect `Retry-After` header.
- **No rate limiter needed**: wx-uploader processes files sequentially, so RPM limiting is unnecessary (unlike the reference code which runs batch operations).

```rust
pub struct GeminiClient {
    auth: GeminiAuth, // Explicit API key or shared, lazily initialized ADC credentials
    http_client: Client,
    model_id: &'static str,  // "gemini-nano-banana-2.1" or "gemini-3-pro-image-preview"
    vertex_project: Option<String>, // Falls back to the ADC quota project
    vertex_location: String, // Defaults to global
}
```

### Refactor `src/openai.rs`

- Send the complete selected prompt directly to `gpt-image-2`, then decode or download the generated image. No separate scene-description model is called.
- Keep `ImageGenerator` for provider-specific generation and image decoding/download.

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
            let model_id = match model {
                "nb" => "gemini-3-pro-image-preview",
                _ => "gemini-nano-banana-2.1",
            };
            let client = match config.gemini_api_key.as_ref().filter(|key| !key.trim().is_empty()) {
                Some(api_key) => GeminiClient::new(api_key.clone(), model_id),
                None => GeminiClient::from_adc(model_id),
            };
            Box::new(client)
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

Use `google-cloud-auth` for ADC discovery, access token caching, and refresh, plus `http` for credential provider extensions. Keep `reqwest` + `serde_json` + `base64` for the Gemini API request and response.

## Migration & Compatibility

- **No breaking changes**: Existing articles without `model` field default to `nb2` (Gemini Nano Banana 2.1), which is a behavior change from the original `gpt` default. The existing `nb2` alias now selects Nano Banana 2.1.
- **Existing `OPENAI_API_KEY`-only users**: Gemini attempts ADC when `GEMINI_API_KEY` is absent. Configure ADC or add `model: gpt` to frontmatter to keep using OpenAI.

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
| `Cargo.toml`    | Add `google-cloud-auth` and `http` for ADC authentication                      |
