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

# 可选：用于默认自动生成封面图的 Gemini API 密钥
export GEMINI_API_KEY="your_gemini_api_key"

# 可选：用于 `model: gpt` 的 OpenAI API 密钥
export OPENAI_API_KEY="your_openai_api_key"
```

您可以在微信开发者控制台获取 `WECHAT_APP_ID` 和 `WECHAT_APP_SECRET`：
[https://developers.weixin.qq.com/console](https://developers.weixin.qq.com/console)

如果看到类似下面的错误：

```text
WeChat API error: WeChat upload failed: WeChat API error [40164]: invalid ip <ipv4 address> ipv6 <ipv6 address>, not in whitelist rid
```

请在同一个控制台中把当前的 API IP 添加到白名单后再重试。

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
3. 如果没有指定封面图片，工具会按所选图像模型自动生成封面。默认使用 `nb2`（Gemini Flash），也可以在 frontmatter 中选择 `nb`（Gemini Pro）或 `gpt`（OpenAI）
4. 指定单个文件时，无论其发布状态如何都会被上传
5. 上传成功后，frontmatter 会更新为 `published: draft` 并包含生成的封面文件名（如果有）

## Frontmatter 示例

```yaml
---
title: 我的文章标题
published: draft  # 或 'true' 以跳过上传
cover: cover.png  # 可选，如果缺失且已配置所选模型对应的密钥则自动生成
model: nb2        # 可选，默认是 nb2；可用值：nb2、nb、gpt
description: 文章描述
author: 作者姓名
theme: lapis  # 可选主题
---

您的 Markdown 内容在这里...
```

## AI 封面生成

当设置了 `GEMINI_API_KEY` 环境变量时，工具会为没有指定封面的文章自动生成封面图片。Gemini 是默认后端。如果您想对某篇文章改用 OpenAI，可在该文件的 frontmatter 中设置 `model: gpt`，并提供 `OPENAI_API_KEY`。

### 模型选择

- `nb2`（默认）：Gemini Flash 图像模型
- `nb`：Gemini Pro 图像模型
- `gpt`：OpenAI 图像生成

### 工作原理

1. **上下文构建**：使用文章标题和描述作为图像生成上下文
2. **模型路由**：根据 frontmatter 中的 `model` 选择后端；未指定时默认使用 `nb2`
3. **图像生成**：由 Gemini 或 OpenAI 生成高质量的 16:9 封面图片
4. **自动保存**：下载并保存图片到与 Markdown 文件相同的目录
5. **元数据更新**：使用生成的封面文件名更新 frontmatter

### 特性

- **多后端支持**：默认使用 Gemini，也支持按文章切换到 OpenAI
- **内容感知**：提示词基于文章标题和描述构建
- **高质量**：生成适合文章缩略图展示的 16:9 宽幅封面
- **自动命名**：生成的文件使用唯一名称以防止冲突
- **优雅降级**：如果图像生成失败，继续正常上传流程
- **Base64 支持**：同时处理 URL 和 base64 编码的图像响应

### 输出示例

对于一篇关于“构建 Rust 应用程序”的文章，工具会结合标题和描述生成一张宽幅封面图，用视觉方式表达文章的核心主题。

## 功能特性

- 📝 **批量上传**：处理整个目录的 Markdown 文件
- 🎨 **AI 封面生成**：默认使用 Gemini 自动生成封面，也支持可选 OpenAI
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
