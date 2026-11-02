//! Prepare SVGs and cover images without modifying the article or source images.

use crate::error::{Error, Result};
use crate::image_utils::encode_image;
use crate::models::Frontmatter;
use crate::output::{FORMATTER, OutputFormatter};
use crate::site::{CoverFit, SiteConfig, resolve_image};
use comrak::{Arena, Options, nodes::NodeValue, parse_document};
use image::ImageFormat;
use resvg::{tiny_skia, usvg};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use tempfile::{NamedTempFile, TempDir};
use tracing::info;

const COVER_RATIO: f64 = 2.35;
const MAX_SVG_EDGE: f32 = 4096.0;
const MAX_DIGEST_UNITS: usize = 120;
static FONTS: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_system_fonts();
    Arc::new(fonts)
});

/// Keeps the upload-only Markdown and rasterized images alive until upload ends.
pub struct TempUploadFiles {
    /// Upload-only Markdown in a private system temporary directory.
    pub markdown: NamedTempFile,
    /// Temporary cropped or normalized cover, if one was needed.
    pub cover: Option<NamedTempFile>,
    _images: Vec<NamedTempFile>,
    _directory: TempDir,
}

/// Bound the digest that the dependency actually sends, including YAML quoting.
/// UTF-16 units conservatively count supplementary characters (e.g. emoji) as two.
fn prepare_digest(description: &str) -> Result<(String, bool)> {
    let normalized = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let transmitted_length = |text: &str| -> Result<usize> {
        // wechat-pub-rs reads one line and strips double quotes without decoding YAML.
        Ok(serde_yaml::to_string(text)?
            .trim()
            .trim_matches('"')
            .encode_utf16()
            .count())
    };
    if transmitted_length(&normalized)? <= MAX_DIGEST_UNITS {
        return Ok((normalized, false));
    }
    let mut units = 0;
    let mut shortened: String = normalized
        .chars()
        .take_while(|ch| {
            units += ch.len_utf16();
            units < MAX_DIGEST_UNITS // Reserve one unit for the ellipsis.
        })
        .collect();
    shortened.push('…');
    while transmitted_length(&shortened)? > MAX_DIGEST_UNITS {
        shortened.pop(); // ellipsis
        shortened.pop(); // a whole Unicode scalar, never a partial UTF-8 sequence
        shortened.push('…');
    }
    Ok((shortened, true))
}

fn is_svg(path: &str) -> bool {
    let path = reqwest::Url::parse(path)
        .ok()
        .map(|url| url.path().to_owned())
        .unwrap_or_else(|| path.to_owned());
    Path::new(&path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg") || ext.eq_ignore_ascii_case("svgz"))
}

fn render_svg(bytes: &[u8], resources_dir: Option<&Path>) -> Result<Vec<u8>> {
    let options = usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        fontdb: Arc::clone(&FONTS),
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options)
        .map_err(|e| Error::generic(format!("Failed to parse SVG: {e}")))?;
    // Render charts at 2x for legible text, with bounded output allocation.
    let size = tree.size();
    let scale = 2.0_f32.min(MAX_SVG_EDGE / size.width().max(size.height()));
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| Error::generic("Failed to allocate SVG rendering buffer"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| Error::generic(format!("Failed to encode SVG as PNG: {e}")))
}

fn temp_image(bytes: &[u8], extension: &str, directory: &Path) -> Result<NamedTempFile> {
    let mut file = tempfile::Builder::new()
        .prefix("wx_image_")
        .suffix(extension)
        .tempfile_in(directory)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(file)
}

fn prepare_cover_with_fit(
    path: &Path,
    directory: &Path,
    fit: CoverFit,
) -> Result<Option<NamedTempFile>> {
    let original = std::fs::read(path)?;
    let format = image::guess_format(&original).ok();
    let bytes = if format.is_none() && is_svg(&path.to_string_lossy()) {
        render_svg(&original, path.parent())?
    } else {
        original
    };
    let image = image::load_from_memory(&bytes)
        .map_err(|e| Error::cover_image(path, format!("Failed to decode cover: {e}")))?;
    let ratio = image.width() as f64 / image.height() as f64;
    let expected = ImageFormat::from_path(path).ok();
    let needs_crop = match fit {
        CoverFit::Crop => ratio < COVER_RATIO,
        CoverFit::Contain => (ratio - COVER_RATIO).abs() > 0.01,
    };
    let needs_conversion =
        expected != format || !matches!(format, Some(ImageFormat::Png | ImageFormat::Jpeg));
    if !needs_crop && !needs_conversion {
        return Ok(None);
    }
    let image = if needs_crop && fit == CoverFit::Contain {
        let (width, height) = if ratio < COVER_RATIO {
            (
                (image.height() as f64 * COVER_RATIO).ceil() as u32,
                image.height(),
            )
        } else {
            (
                image.width(),
                (image.width() as f64 / COVER_RATIO).ceil() as u32,
            )
        };
        let mut padded = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([255, 255, 255, 255]),
        ));
        image::imageops::overlay(
            &mut padded,
            &image,
            ((width - image.width()) / 2) as i64,
            ((height - image.height()) / 2) as i64,
        );
        padded
    } else if needs_crop {
        let height = ((image.width() as f64 / COVER_RATIO).round() as u32)
            .max(1)
            .min(image.height());
        image.crop_imm(0, (image.height() - height) / 2, image.width(), height)
    } else {
        image
    };
    let (output_format, extension) = match expected {
        Some(ImageFormat::Jpeg) => (ImageFormat::Jpeg, ".jpg"),
        _ => (ImageFormat::Png, ".png"),
    };
    let file = temp_image(&encode_image(&image, output_format)?, extension, directory)?;
    info!(
        "Prepared cover for upload: {} -> {}",
        path.display(),
        file.path().display()
    );
    Ok(Some(file))
}

fn rewrite_images(body: &str, mapping: &HashMap<String, String>) -> Result<String> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.footnotes = true;
    options.extension.tasklist = true;
    let root = parse_document(&arena, body, &options);
    let image_nodes = root
        .descendants()
        .filter(|node| matches!(node.data.borrow().value, NodeValue::Image(_)))
        .collect::<Vec<_>>();
    for node in image_nodes {
        let replacement = match &node.data.borrow().value {
            NodeValue::Image(link) => mapping.get(&link.url).cloned(),
            _ => None,
        };
        if let Some(url) = replacement {
            // Emit a plain inline image so wechat-pub-rs can replace its URL after
            // upload, including images originally written as references or with titles.
            let alt = node
                .descendants()
                .filter_map(|child| match &child.data.borrow().value {
                    NodeValue::Text(text) => Some(text.to_string()),
                    NodeValue::Code(code) => Some(code.literal.clone()),
                    _ => None,
                })
                .collect::<String>();
            for child in node.children().collect::<Vec<_>>() {
                child.detach();
            }
            node.append(arena.alloc(comrak::nodes::AstNode::from(NodeValue::Text(alt.into()))));
            if let NodeValue::Image(link) = &mut node.data.borrow_mut().value {
                link.url = url;
                link.title.clear();
            }
        }
    }
    let mut output = String::new();
    comrak::format_commonmark(root, &options, &mut output)
        .map_err(|e| Error::generic(format!("Failed to prepare image references: {e}")))?;
    Ok(output)
}

/// Rasterize SVG image references and normalize the cover before WeChat sees them.
#[cfg(test)]
pub(crate) async fn prepare_upload_files(
    markdown_path: &Path,
    frontmatter: &Frontmatter,
    body: &str,
) -> Result<TempUploadFiles> {
    prepare_upload_files_with_site(markdown_path, frontmatter, body, None, false).await
}

pub(crate) async fn prepare_upload_files_with_site(
    markdown_path: &Path,
    frontmatter: &Frontmatter,
    body: &str,
    site: Option<&SiteConfig>,
    offline: bool,
) -> Result<TempUploadFiles> {
    let absolute = markdown_path.canonicalize()?;
    let directory = tempfile::Builder::new().prefix("wx_upload_").tempdir()?;
    let cover_path = frontmatter
        .cover
        .as_ref()
        .map(|filename| resolve_image(site, &absolute, filename))
        .transpose()?;
    let cover = if let Some(path) = cover_path.as_deref().filter(|path| path.exists()) {
        prepare_cover_with_fit(
            path,
            directory.path(),
            site.map_or(CoverFit::Crop, |site| site.cover.fit),
        )?
    } else {
        None
    };
    let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
        .parse(body)
        .map_err(|e| Error::generic(format!("Failed to inspect article images: {e}")))?;
    let description = if frontmatter.description.trim().is_empty() {
        // Extract the first paragraph before bounding it ourselves. The dependency's
        // bounded fallback slices bytes and can panic on Chinese text without a paragraph.
        parsed.get_summary(usize::MAX)
    } else {
        frontmatter.description.clone()
    };
    let (digest, shortened) = prepare_digest(&description)?;
    if shortened && !frontmatter.description.trim().is_empty() {
        FORMATTER.print_warning("微信摘要过长，上传副本已截断至 120 字以内（原文保留）");
    }
    let references = parsed.images;
    let mut mapping = HashMap::new();
    let mut images = Vec::new();
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    for reference in references {
        let url = &reference.original_url;
        if mapping.contains_key(url) {
            continue;
        }
        if !is_svg(url) {
            if reference.is_local {
                let source = resolve_image(site, &absolute, url)?;
                let bytes = std::fs::read(&source).map_err(|e| {
                    Error::generic(format!(
                        "Failed to read article image '{}': {e}",
                        source.display()
                    ))
                })?;
                if offline {
                    image::load_from_memory(&bytes).map_err(|e| {
                        Error::generic(format!(
                            "Failed to decode article image '{}': {e}",
                            source.display()
                        ))
                    })?;
                }
                let extension = source
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
                    .unwrap_or_default();
                let file = temp_image(&bytes, &extension, directory.path())?;
                mapping.insert(url.clone(), file.path().to_string_lossy().into_owned());
                images.push(file);
            }
            continue;
        }
        let (bytes, resource_dir) = if reference.is_local {
            let path = resolve_image(site, &absolute, url)?
                .canonicalize()
                .map_err(|e| Error::generic(format!("Failed to read SVG '{url}': {e}")))?;
            (std::fs::read(&path)?, path.parent().map(Path::to_path_buf))
        } else {
            if offline {
                return Err(Error::config(format!(
                    "Cannot check remote SVG offline: {url}"
                )));
            }
            let response = http.get(url).send().await?.error_for_status()?;
            (response.bytes().await?.to_vec(), None)
        };
        let png = render_svg(&bytes, resource_dir.as_deref())
            .map_err(|e| Error::generic(format!("Failed to convert SVG '{url}': {e}")))?;
        let file = temp_image(&png, ".png", directory.path())?;
        info!(
            "Converted SVG for upload: {} -> {}",
            url,
            file.path().display()
        );
        mapping.insert(url.clone(), file.path().to_string_lossy().into_owned());
        images.push(file);
    }
    let upload_body = if mapping.is_empty() {
        body.to_owned()
    } else {
        rewrite_images(body, &mapping)?
    };
    let mut upload_frontmatter = frontmatter.clone();
    upload_frontmatter.description = digest;
    if let Some(file) = &cover {
        upload_frontmatter.set_cover(file.path().to_string_lossy().to_string());
    } else if let Some(path) = cover_path {
        // A cover that needs no conversion still has to resolve from the article.
        upload_frontmatter.set_cover(path.to_string_lossy().into_owned());
    }
    let mut markdown = tempfile::Builder::new()
        // The enclosing directory is unique; keep the source basename for Mermaid.
        .prefix(
            absolute
                .file_name()
                .ok_or_else(|| Error::generic("Markdown file has no filename"))?,
        )
        .rand_bytes(0)
        .tempfile_in(directory.path())?;
    markdown.write_all(
        crate::markdown::format_markdown(&upload_frontmatter, &upload_body)?.as_bytes(),
    )?;
    markdown.flush()?;
    Ok(TempUploadFiles {
        markdown,
        cover,
        _images: images,
        _directory: directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgba};
    use std::fs;
    use tempfile::TempDir;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 60 40"><rect width="60" height="40" fill="#ff0000"/></svg>"##;

    fn article(dir: &Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("article.md");
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn digest_limits_preserve_valid_unicode_and_include_the_ellipsis() {
        for text in ["中".repeat(120), "a".repeat(120), "😀".repeat(60)] {
            assert_eq!(prepare_digest(&text).unwrap(), (text, false));
        }
        for text in ["中".repeat(121), "a".repeat(121), "😀".repeat(61)] {
            let (digest, shortened) = prepare_digest(&text).unwrap();
            assert!(shortened);
            assert!(digest.ends_with('…'));
            assert!(digest.encode_utf16().count() <= 120);
            assert!(text.starts_with(digest.trim_end_matches('…')));
        }
    }

    #[tokio::test]
    async fn sdk_receives_bounded_single_line_digest_without_changing_source() {
        let actual_description = "在大公司里，客服、公关、商务、行政是五个部门；在一人公司里，所有消息最终都涌向你一个人。本文拆解一人公司在对外沟通上的真实痛点与现有工具短板，并从架构层面设计两款核心产品：按语境自动切换角色的统一收件箱，以及帮中腰部创作者谈商单的 AI 商务经纪人。";
        assert_eq!(actual_description.chars().count(), 125);
        let cases = [
            actual_description.to_string(),
            "正常的短摘要".to_string(),
            "第一行\n第二行\r\n第三行".to_string(),
            "😀中文 mixed ".repeat(30),
            format!("摘要: {}", "中".repeat(120)),
            format!("\"{}\"", "\\".repeat(120)),
        ];
        let dir = TempDir::new().unwrap();
        for description in cases {
            let frontmatter = Frontmatter {
                description: description.clone(),
                ..Default::default()
            };
            let original = crate::markdown::format_markdown(&frontmatter, "正文").unwrap();
            let path = article(dir.path(), &original);
            let prepared = prepare_upload_files(&path, &frontmatter, "正文")
                .await
                .unwrap();
            let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
                .parse_file(prepared.markdown.path())
                .await
                .unwrap();
            let digest = parsed.description.unwrap();
            assert!(digest.encode_utf16().count() <= 120, "{digest:?}");
            assert!(!digest.contains(['\n', '\r']));
            assert_ne!(digest, "|-");
            if description == "正常的短摘要" {
                assert_eq!(digest, description);
            }
            assert_eq!(frontmatter.description, description);
            assert_eq!(fs::read_to_string(path).unwrap(), original);
        }
    }

    #[tokio::test]
    async fn missing_description_uses_a_safe_digest_even_without_a_paragraph() {
        let dir = TempDir::new().unwrap();
        for body in ["正文内容".repeat(100), format!("# {}", "摘要".repeat(100))] {
            let path = article(dir.path(), &body);
            let prepared = prepare_upload_files(&path, &Frontmatter::default(), &body)
                .await
                .unwrap();
            let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
                .parse_file(prepared.markdown.path())
                .await
                .unwrap();
            let digest = parsed.description.unwrap();
            assert!(digest.encode_utf16().count() <= 120);
            assert!(digest.ends_with('…'));
        }
    }

    #[tokio::test]
    async fn svg_is_converted_even_with_uncropped_cover_and_cleaned_after_upload() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("images")).unwrap();
        let svg_path = dir.path().join("images/chart.SVG");
        fs::write(&svg_path, SVG).unwrap();
        DynamicImage::new_rgb8(240, 100)
            .save(dir.path().join("cover.png"))
            .unwrap();
        let body = "# 标题\n\n![统计图](images/chart.SVG)\n\n![重复](images/chart.SVG)\n\n```md\n![示例](missing.svg)\n```\n";
        let path = article(dir.path(), body);
        let frontmatter = Frontmatter {
            cover: Some("cover.png".into()),
            ..Default::default()
        };
        let prepared = prepare_upload_files(&path, &frontmatter, body)
            .await
            .unwrap();
        assert!(prepared.cover.is_none());
        assert_eq!(prepared._images.len(), 1);
        assert_eq!(
            prepared.markdown.path().parent(),
            Some(prepared._directory.path())
        );
        assert_ne!(prepared._directory.path(), dir.path());
        assert_eq!(prepared.markdown.path().file_name(), path.file_name());
        let upload_directory = prepared._directory.path().to_owned();
        let png_path = prepared._images[0].path().to_owned();
        let markdown_path = prepared.markdown.path().to_owned();
        let (metadata, uploaded_body) = crate::markdown::parse_markdown_file(&markdown_path)
            .await
            .unwrap();
        assert_eq!(
            Path::new(metadata.cover.as_deref().unwrap()),
            dir.path().canonicalize().unwrap().join("cover.png")
        );
        let references = wechat_pub_rs::markdown::MarkdownParser::new()
            .parse(&uploaded_body)
            .unwrap()
            .images;
        assert_eq!(references.len(), 2);
        assert!(
            references
                .iter()
                .all(|r| Path::new(&r.original_url) == png_path)
        );
        let png = image::open(&png_path).unwrap().to_rgba8();
        assert_eq!(png.dimensions(), (120, 80));
        assert_eq!(png.get_pixel(60, 40), &Rgba([255, 0, 0, 255]));
        assert!(uploaded_body.contains("missing.svg"));
        assert_eq!(fs::read_to_string(&path).unwrap(), body);
        assert_eq!(fs::read_to_string(&svg_path).unwrap(), SVG);
        // Files created later by the upload backend (e.g. Mermaid) share the guard.
        fs::create_dir(upload_directory.join("images")).unwrap();
        fs::write(
            upload_directory.join("images/backend.png"),
            b"backend output",
        )
        .unwrap();
        drop(prepared);
        assert!(!png_path.exists());
        assert!(!markdown_path.exists());
        assert!(!upload_directory.exists());
    }

    #[tokio::test]
    async fn reference_svg_without_cover_becomes_uploadable_inline_png() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("chart.svg"), SVG).unwrap();
        let body = "![*图示*][chart]\n\n[chart]: chart.svg \"原标题\"\n";
        let path = article(dir.path(), body);
        let prepared = prepare_upload_files(&path, &Frontmatter::default(), body)
            .await
            .unwrap();
        let (_, upload_body) = crate::markdown::parse_markdown_file(prepared.markdown.path())
            .await
            .unwrap();
        let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
            .parse(&upload_body)
            .unwrap();
        assert_eq!(parsed.images.len(), 1);
        assert!(upload_body.contains(&format!("![图示]({})", parsed.images[0].original_url)));
        assert_eq!(
            image::guess_format(&fs::read(&parsed.images[0].original_url).unwrap()).unwrap(),
            ImageFormat::Png
        );
    }

    #[tokio::test]
    async fn upload_filename_passes_wechat_validation_outside_system_temp_directory() {
        let dir = tempfile::Builder::new()
            .prefix("wx_upload_test_")
            .tempdir_in(env!("CARGO_MANIFEST_DIR"))
            .unwrap();
        fs::write(dir.path().join("chart.svg"), SVG).unwrap();
        let body = "![图](chart.svg)";
        let path = article(dir.path(), body);
        let prepared = prepare_upload_files(&path, &Frontmatter::default(), body)
            .await
            .unwrap();
        let filename = prepared.markdown.path().file_name().unwrap();
        // System temp directories bypass the dependency's hidden-file restriction,
        // so check the generated name under an ordinary project directory as well.
        let project_path = Path::new("/projects/articles").join(filename);
        assert!(wechat_pub_rs::utils::is_safe_path(&project_path));
        assert!(wechat_pub_rs::utils::is_markdown_file(&project_path));
        assert!(wechat_pub_rs::utils::file_exists(prepared.markdown.path()).await);
        let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
            .parse_file(prepared.markdown.path())
            .await
            .unwrap();
        assert_eq!(parsed.images.len(), 1);
        assert!(!wechat_pub_rs::utils::is_safe_path(Path::new(
            "/projects/articles/.wx_upload_regression.md"
        )));
    }

    #[tokio::test]
    async fn png_named_jpeg_is_normalized_even_without_cropping() {
        let dir = TempDir::new().unwrap();
        let cover_path = dir.path().join("cover.jpg");
        let png = encode_image(&DynamicImage::new_rgba8(240, 100), ImageFormat::Png).unwrap();
        fs::write(&cover_path, &png).unwrap();
        let path = article(dir.path(), "正文");
        let frontmatter = Frontmatter {
            cover: Some("cover.jpg".into()),
            ..Default::default()
        };
        let prepared = prepare_upload_files(&path, &frontmatter, "正文")
            .await
            .unwrap();
        let cover = prepared.cover.as_ref().unwrap();
        assert_eq!(
            image::guess_format(&fs::read(cover.path()).unwrap()).unwrap(),
            ImageFormat::Jpeg
        );
        assert_eq!(image::image_dimensions(cover.path()).unwrap(), (240, 100));
        assert_eq!(fs::read(cover_path).unwrap(), png);
    }

    #[tokio::test]
    async fn relative_raster_images_and_uncropped_cover_resolve_from_source_directory() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("images")).unwrap();
        let cover_path = dir.path().join("cover.jpg");
        DynamicImage::new_rgb8(240, 100).save(&cover_path).unwrap();
        let raster = dir.path().join("images/ordinary image.png");
        DynamicImage::new_rgb8(24, 10).save(&raster).unwrap();
        let original_bytes = fs::read(&raster).unwrap();
        let body =
            "![普通图](<images/ordinary image.png>)\n\n![远程图](https://example.com/image.png)\n";
        let path = article(dir.path(), body);
        let frontmatter = Frontmatter {
            cover: Some("cover.jpg".into()),
            ..Default::default()
        };
        let prepared = prepare_upload_files(&path, &frontmatter, body)
            .await
            .unwrap();
        assert!(prepared.cover.is_none());
        let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
            .parse_file(prepared.markdown.path())
            .await
            .unwrap();
        assert_eq!(
            parsed.cover.as_deref().map(Path::new),
            Some(cover_path.canonicalize().unwrap().as_path())
        );
        assert_eq!(parsed.images.len(), 2);
        let image_path = Path::new(&parsed.images[0].original_url);
        assert_eq!(image_path.parent(), Some(prepared._directory.path()));
        assert_eq!(fs::read(image_path).unwrap(), original_bytes);
        assert_eq!(
            parsed.images[1].original_url,
            "https://example.com/image.png"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), body);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    #[test]
    fn svg_cover_is_rasterized_and_cropped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cover.svg");
        fs::write(&path, SVG).unwrap();
        let cover = prepare_cover_with_fit(&path, dir.path(), CoverFit::Crop)
            .unwrap()
            .unwrap();
        assert_eq!(cover.path().extension().unwrap(), "png");
        assert_eq!(image::image_dimensions(cover.path()).unwrap(), (120, 51));
        assert_eq!(fs::read_to_string(path).unwrap(), SVG);
    }

    #[tokio::test]
    async fn invalid_svg_reports_source_and_leaves_originals_unchanged() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("broken.svg"), "not SVG").unwrap();
        let body = "![图](broken.svg)";
        let path = article(dir.path(), body);
        let error = prepare_upload_files(&path, &Frontmatter::default(), body)
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("broken.svg"));
        assert_eq!(fs::read_to_string(path).unwrap(), body);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn remote_svg_with_query_is_downloaded_and_converted() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/chart.svg?v=1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut bytes = [0; 2048];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let count = stream.read(&mut bytes).await.unwrap();
                assert!(count > 0, "incomplete HTTP request");
                request.extend_from_slice(&bytes[..count]);
            }
            assert!(String::from_utf8_lossy(&request).starts_with("GET /chart.svg?v=1 "));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                SVG.len(),
                SVG
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let dir = TempDir::new().unwrap();
        let body = format!("![远程图]({url})");
        let path = article(dir.path(), &body);
        let prepared = prepare_upload_files(&path, &Frontmatter::default(), &body)
            .await
            .unwrap();
        assert_eq!(prepared._images.len(), 1);
        assert_eq!(
            image::guess_format(&fs::read(prepared._images[0].path()).unwrap()).unwrap(),
            ImageFormat::Png
        );
        server.await.unwrap();
    }

    #[test]
    fn relative_raster_resources_inside_svg_are_resolved_from_svg_directory() {
        let dir = TempDir::new().unwrap();
        image::RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255]))
            .save(dir.path().join("pixel.png"))
            .unwrap();
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><image href="pixel.png" width="2" height="2"/></svg>"#;
        let png = render_svg(svg, Some(dir.path())).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(2, 2), &Rgba([0, 255, 0, 255]));
    }
}
