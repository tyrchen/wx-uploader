## [0.7.0] - 2026-03-13

### 🚀 Features

- Add Gemini image backend, Chinese prompts, and display original filename on upload
## [0.6.1] - 2025-12-27

### 🐛 Bug Fixes

- Crop cover image to WeChat 2.35:1 ratio before upload
- Create temp markdown in same dir as original
- Use absolute path for temp markdown file
- Use tempfile crate for proper temp file management
- Put both temp files in system temp dir

### ⚙️ Miscellaneous Tasks

- Use gpt-image-1.5
## [0.6.0] - 2025-09-13

### ⚙️ Miscellaneous Tasks

- Use wechat-pub-rs 0.6
## [0.5.2] - 2025-08-16

### 🚀 Features

- Allow clear wechat token cache
## [0.5.1] - 2025-08-10

### 📚 Documentation

- Update README with latest project features

### ⚙️ Miscellaneous Tasks

- Add license
- Bump dep version
## [0.5.0] - 2025-08-09

### 🚀 Features

- Integrate OpenAI SDK for automatic cover image generation
- Update to latest OpenAI models
- Add base64 image handling for gpt-image-1 model
- Refactor the code based on code review

### 🐛 Bug Fixes

- Preserve cover field in frontmatter when image is missing
- Use correct OpenAI model names for API calls
- Resolve OpenAI API parameter compatibility issues

### ⚙️ Miscellaneous Tasks

- Remove unnecessary debug output from OpenAI integration
## [0.4.2] - 2025-08-03

### ⚙️ Miscellaneous Tasks

- Update dep and bump version
## [0.4.1] - 2025-08-03

### 🐛 Bug Fixes

- Help message could be better
- Help message could be better
- Help message could be better
## [0.4.0] - 2025-08-03

### 🚀 Features

- Support basic wx uploader functionality
- Better output

### ⚙️ Miscellaneous Tasks

- Fix gh action
- Bump wechart pub dep
- Bump deps
