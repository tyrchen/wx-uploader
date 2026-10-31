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

# 可选：显式使用 Gemini Developer API（AI Studio），替代 Vertex AI
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
model: nb2  # 默认：Gemini Nano Banana 2.1；nb：Gemini Pro；gpt：OpenAI
description: 文章描述
tags: [Rust, 并发]
cover_prompt: |  # 可选：直接描述想要的画面
  多个读者围着一本展开的共享账本阅读；
  右侧一名写作者在隔开的区域独自书写。
cover_style: |  # 可选：覆盖默认的杂志插画风格
  黑白木刻，轮廓清晰，细节克制。
author: 作者姓名
theme: lapis  # 可选主题
---

您的 Markdown 内容在这里...
```

## AI 封面生成

封面生成默认使用 Gemini Nano Banana 2.1（`gemini-nano-banana-2.1`），在 frontmatter 中省略 `model` 或设置 `model: nb2` 即可。原有 `nb2` 别名现已指向 Nano Banana 2.1，详见[官方模型文档](https://ai.google.dev/gemini-api/docs/models/gemini-nano-banana-2.1)。

未设置 `GEMINI_API_KEY` 或其值为空时，Gemini 默认通过 **Google Cloud Vertex AI + ADC** 调用，使用 `aiplatform.googleapis.com`，由选定资源项目的 Cloud Billing 计费。以下为 Nushell 配置命令：

```nu
let project = "YOUR_PROJECT_ID"
^gcloud services enable aiplatform.googleapis.com --project $project
^gcloud auth application-default login
^gcloud auth application-default set-quota-project $project
$env.GOOGLE_CLOUD_PROJECT = $project
$env.GOOGLE_CLOUD_LOCATION = "global"
hide-env --ignore-errors GEMINI_API_KEY
```

Vertex 资源项目依次取自 `GOOGLE_CLOUD_PROJECT`、`GCLOUD_PROJECT`、ADC 配额项目。区域由 `GOOGLE_CLOUD_LOCATION` 指定，默认 `global`；Nano Banana 2.1 使用 global 端点。账号需要具有该项目的 Vertex AI 和服务使用权限。Vertex AI 使用普通 ADC 登录即可获得的 `cloud-platform` scope；已有 ADC 包含此 scope 时可直接复用。详见 [Google Cloud ADC 官方指南](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/start/gcp-auth)。

Google 官方 Rust 认证库会依次查找 `GOOGLE_APPLICATION_CREDENTIALS` 指定的文件、本地 ADC 文件和 Google Cloud 元数据服务，自动获取、缓存、刷新短期 OAuth access token，无需导出 token，运行时也无需调用 `gcloud`。多篇文章共享认证凭据和 token 缓存。ADC 配额项目通过 `x-goog-user-project` 发送，也可用 `GOOGLE_CLOUD_QUOTA_PROJECT` 覆盖；服务账号没有 ADC 配额项目时，需要显式设置 `GOOGLE_CLOUD_PROJECT`。适用的 GCP 赠金由资源项目所关联账单账号的 Cloud Billing 处理。

显式设置非空 `GEMINI_API_KEY` 会选择 **Gemini Developer API**（`generativelanguage.googleapis.com`），走 AI Studio 账单规则，可能要求预付余额；ADC 默认使用 Vertex AI。设置 `model: nb` 可使用 Gemini Pro，认证方式相同；设置 `model: gpt` 可使用 OpenAI 的 `gpt-image-2`（`OPENAI_API_KEY`）。

生图使用两套不同的提示词：非空 `cover_prompt` 直接指定画面，模型按描述呈现主体、动作和关系；省略或留空时，仅根据 `title`、`description`、`tags` 构思封面。`description` 也可写作 `desc`；`tags` 支持 YAML 列表或字符串。正文、章节标题和其他 frontmatter 字段不进入生图请求。画面描述和主题元数据都缺失时，提示警告并跳过生图。

`cover_style` 在两种模式下都生效，替换默认画风，因此可自定义木刻、3D 黏土等风格。省略或留空时，默认使用二维杂志插画、水粉与丝网印刷质感。两套提示词均要求关键元素居中，兼顾微信宽幅封面和方形缩略图，禁止文字、商标、水印和边框；元数据模式要求一个主视觉、最多两个辅助元素。画面描述最多取 4,000 个 Unicode 字符，风格最多 1,200 个；标题、description、tags 分别最多 320、1,600、512 个字符，列表 tags 最多取 16 项。正常情况下只需一次生图请求，无需额外的摘要模型。

OpenAI 使用 `quality: medium`、`1280×544`（接近微信的 2.35:1 比例）和无损 PNG。不提高分辨率，避免旧版 1536×1024 输出的大幅纵向裁切及 JPEG 压缩。Gemini 使用 21:9、1K 分辨率。

图片保存在 Markdown 文件旁，文件名写回 frontmatter。已有封面会被复用；需要重新生成时，删除封面文件或移除 `cover` 字段。生成失败时继续现有上传流程。

上传前会将本地及远程 SVG/SVGZ 图片引用渲染为临时 PNG，使用系统字体、2 倍分辨率，单边最大 4096 像素；封面无需裁切时也会执行。上传副本和临时图片统一放在独立的系统临时目录中。本地图片按原文目录解析后复制，未转换的封面使用原文件的绝对路径；Markdown 保留原文件名以兼容 Mermaid。上传结束后删除整个临时目录，包括 Mermaid 生成的文件。原 SVG 和原文中的图片引用保持不变。生成封面的实际编码与扩展名一致；已有格式不匹配的封面（如 PNG 内容保存成 `.jpg`）会在上传副本中转换为正确格式。

微信摘要使用 `description`（或 `desc`），缺失时从正文提取。上传副本会合并摘要中的换行和多余空白，并将超长摘要截断、添加省略号。最终提交值限制在 120 个 UTF-16 单元内（普通汉字计 1、增补字符如 emoji 计 2，包含省略号及上传解析器保留的 YAML 引号）。原文摘要和生图使用的完整描述保持不变。

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
