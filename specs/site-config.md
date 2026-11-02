# Site configuration (v0.9.0)

## Goal
Upload existing site Markdown without changing website metadata. QiaoPang is the
reference site: articles under `src/posts`, `/images/...` under `public`, SVG
covers, and `excerpt` summaries. Keep unconfigured behavior compatible.

## Configuration contract
Discover the nearest `wx-uploader.yaml` by walking ancestors of the canonical
input file/directory, or use `--config PATH`. A directory run uses one config;
all config paths and include globs are relative to its directory, never CWD.
Unknown fields, unsupported versions, invalid globs and invalid combinations fail
before credentials/network initialization. Secrets remain in environment variables.

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

Defaults: empty include list means all Markdown; images.root is `.` and
relative_to is `article`; field names are title/description/cover (legacy desc
alias remains); cover.mode is `auto`, fit is `crop`; state.storage is
`frontmatter`, path is `.wx-uploader/state.json`. Version is required and is 1.

Root image mode resolves BOTH `/images/x.svg` and `images/x.svg` under images.root.
It does not reinterpret HTTP(S) URLs. Reject paths escaping the configured image
root, including symlinks. Article mode preserves existing absolute/relative paths.
The same resolver is used for cover existence, generation and body images.
Explicit single files must also match configured content.include.

Mapped fields are read from raw YAML before decoding into the internal model;
website fields are retained separately. Mapping replaces the canonical field
in the upload copy. Empty/missing description falls back to the body summary.
Config rejects empty or duplicate source field names and reserved field collisions.
Custom mappings require sidecar mode, avoiding ambiguous source writeback.

## Cover and temporary assets
`existing` requires a readable local cover and NEVER resolves a generation
backend. Missing/invalid covers fail before upload. `auto` retains generation.
SVG covers/body diagrams reuse resvg and system fonts; conversion only changes
temporary files. `contain` pads to 2.35:1 on white and preserves all source pixels;
`crop` retains the legacy crop behavior. Sidecar mode requires existing covers,
so original articles/assets remain byte-identical. Raster body images retain
their format (including WebP); this release does not change animated image policy.

## Upload state
Sidecar mode ignores the website published/status fields, never rewrites source,
and records versioned JSON after a successful upload only. Keys are scoped by
WeChat app ID and config-relative article path. Each record includes draft ID,
upload time (Unix seconds), and BLAKE3 fingerprint of source bytes, effective
configuration and referenced local cover/body image bytes. Remote asset changes
alone are not tracked. Changed content produces a new draft; unchanged content
is skipped in directory mode. Explicit single-file upload remains forced.

Use a process-level file lock per sidecar store, held through read/upload/write;
fail clearly if another process owns it. Persist via atomic replacement. Invalid
state fails before upload. A failed state write reports the successfully created
draft ID so the user can recover without blindly retrying. Frontmatter mode
retains the legacy published skip and published: draft writeback.

## Offline validation
`--check` discovers config, selects files and prepares upload copies (including
image decoding and SVG rendering), then cleans them up. It does not require API
credentials, generate covers, update source/state, or contact WeChat. In auto mode,
missing covers report an error with instructions to run a normal upload. Remote
SVGs cannot be checked offline and report an error. Parse/read errors propagate;
they must never be mistaken for a published-file skip.

## Acceptance checks
- Existing tests and unconfigured path/cover/digest behavior pass.
- Config discovery from nested article paths, explicit override, strict validation,
  include globs, field mapping, and root resolution (including traversal) pass.
- QiaoPang fixture with boolean published, excerpt, SVG cover/diagram and WebP
  prepares successfully, preserving source bytes and complete cover artwork.
- Mock uploader proves sidecar skip, changed local assets/config, account isolation,
  forced upload, failed-upload state behavior, and lock/invalid-state handling.
- CLI offline validation succeeds on all 260 current QiaoPang Chinese articles.
- Format, clippy and test suite pass; version is bumped to 0.9.0; PR is created;
  local release build is installed; QiaoPang receives the config and state ignore.
