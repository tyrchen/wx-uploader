# wx-uploader

一个用于上传 Markdown 文件到微信公众号的命令行工具，支持 AI 自动生成封面图片。

## 安装

从 crates.io 直接安装：

```bash
cargo install wx-uploader
```

或从源码构建：

```bash
git clone https://github.com/tyrchen/wx-uploader.git
cd wx-uploader
cargo install --path .
```

## 前置条件

在使用此工具之前，您需要设置以下环境变量：

```bash
# 必需：微信公众号凭证
export WECHAT_APP_ID="your_app_id"
export WECHAT_APP_SECRET="your_app_secret"

# 可选：OpenAI 题图生成（frontmatter 设置 model: gpt）
export OPENAI_API_KEY="your_openai_api_key"

# 可选：Gemini 题图生成（model: nb2，默认；或 nb）
export GEMINI_API_KEY="your_gemini_api_key"
```

## 使用方法

### 上传目录中的所有 Markdown 文件

```bash
# 上传所有 frontmatter 中没有 `published: true` 的 .md 文件
wx-uploader .

# 从指定目录上传
wx-uploader ./posts

# 启用详细输出
wx-uploader --verbose ./posts
```

### 上传指定文件

```bash
# 强制上传指定文件（忽略发布状态）
wx-uploader ./2025/08/01-chat-with-ai.md
```

## 工作原理

1. 工具扫描带有 YAML frontmatter 的 Markdown 文件
2. 如果文件的 frontmatter 中没有 `published: true`，则会被上传
3. 封面缺失时，使用所选图像模型及其 API 密钥生成杂志插画风格的题图
4. 指定单个文件时，无论其发布状态如何都会被上传
5. 上传成功后，frontmatter 会更新为 `published: draft` 并包含生成的封面文件名（如果有）

## Frontmatter 示例

```yaml
---
title: 我的文章标题
published: draft  # 或 'true' 以跳过上传
cover: cover.png  # 可选；文件缺失时自动生成
model: gpt  # OpenAI；nb2（默认）和 nb 使用 Gemini
description: 文章描述
author: 作者姓名
theme: lapis  # 可选主题
---

您的 Markdown 内容在这里...
```

## AI 封面生成

在 frontmatter 中设置 `model: gpt`，并配置 `OPENAI_API_KEY`，即可使用 OpenAI 的 `gpt-image-2`。也支持 Gemini 的 `nb2`（默认）和 `nb`，使用 `GEMINI_API_KEY`。

生成器通过一次请求读取标题、description、章节标题和正文。代码块和图片链接会被过滤；长文按 Unicode 字符预算保留开头、中段和结论摘录，避免只根据标题猜测主题。

提示词要求画面表达文章中的具体机制和对象关系，以一个主视觉为中心，采用统一的二维杂志插画工艺。关键元素集中在中央，兼顾微信宽幅封面和方形缩略图；禁止文字、商标和无关的泛化科技装饰。

OpenAI 使用 `quality: medium`、`1280×544`（接近微信的 2.35:1 比例）和无损 PNG。不提高分辨率，避免旧版 1536×1024 输出的大幅纵向裁切及 JPEG 压缩。Gemini 使用 21:9，保持原有 2K 设置。

图片保存在 Markdown 文件旁，文件名写回 frontmatter。已有封面会被复用；需要重新生成时，删除封面文件或移除 `cover` 字段。生成失败时继续现有上传流程。

例如，讲解 Rust 借用规则的文章，可以用多条读取路径共享一个资源、与单条独占写入路径对照的画面，具体呈现借用机制。

## 功能特性

- 📝 **批量上传**：处理整个目录的 Markdown 文件
- 🎨 **AI 封面生成**：使用 OpenAI 或 Gemini 自动生成封面图片
- 🔄 **智能处理**：跳过已发布的文章
- 📊 **进度跟踪**：带有彩色状态指示器的清晰控制台输出
- 🛡️ **错误恢复**：优雅地处理 API 失败
- 🔐 **安全**：API 密钥仅存储在环境变量中

## 开发

### 运行测试

项目包含全面的单元测试和集成测试：

```bash
# 运行所有测试
cargo test

# 带输出运行测试
cargo test -- --nocapture

# 运行特定测试模块
cargo test test_frontmatter

# 仅运行集成测试
cargo test --test integration_tests
```

### 代码质量

```bash
# 运行 clippy 进行代码检查
cargo clippy --all-targets --all-features

# 检查安全漏洞
cargo audit

# 格式化代码
cargo fmt

# 生成文档
cargo doc --open
```

### 项目结构

```
wx-uploader/
├── src/
│   ├── main.rs          # CLI 入口点
│   ├── lib.rs           # 公共 API
│   ├── cli.rs           # 命令行接口
│   ├── error.rs         # 错误处理
│   ├── models.rs        # 数据结构
│   ├── markdown.rs      # Markdown 解析
│   ├── openai.rs        # AI 集成
│   ├── output.rs        # 控制台输出格式化
│   └── wechat.rs        # 微信 API 集成
└── tests/
    └── integration_tests.rs  # 集成测试
```

## 注意事项

- 目录扫描时会跳过带有 `published: true` 的文件
- 单文件上传总是强制上传，无论发布状态如何
- 工具在更新时会保留所有其他 frontmatter 字段
- 封面图片保存在与 Markdown 文件相同的目录中
- 支持 published 字段的字符串（`"true"`）和布尔值（`true`）格式

## 许可证

MIT

## 贡献

欢迎贡献！请随时提交 Pull Request。
