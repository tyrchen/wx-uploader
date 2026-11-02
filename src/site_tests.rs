//! Site configuration acceptance tests using local fixtures and a mock uploader.
use crate::{
    Config, Result,
    markdown::parse_markdown_with_site,
    site::SiteConfig,
    state::UploadState,
    upload_images::prepare_upload_files_with_site,
    wechat::{WeChatUploader, check_path, upload_file},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;

const CONFIG: &str = "version: 1\ncontent:\n  include: ['src/posts/**/*.md']\nimages:\n  root: public\n  relative_to: root\nfrontmatter:\n  description: excerpt\ncover:\n  mode: existing\n  fit: contain\nstate:\n  storage: sidecar\n  path: .wx-uploader/state.json\n";
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="63"><rect width="120" height="63" fill="#ff0000"/><rect y="60" width="120" height="3" fill="#00ff00"/></svg>"##;
const ARTICLE: &str = "---\ntitle: 测试文章\npublished: true\nstatus: published\nexcerpt: 网站的摘要\ncover: /images/cover.svg\n---\n\n正文。\n\n![图](/images/diagram.svg)\n\n![截图](/images/photo.webp)\n";
fn fixture() -> (TempDir, PathBuf, SiteConfig) {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src/posts/2026")).unwrap();
    fs::create_dir_all(dir.path().join("public/images")).unwrap();
    for name in ["cover.svg", "diagram.svg"] {
        fs::write(dir.path().join("public/images").join(name), SVG).unwrap();
    }
    image::DynamicImage::new_rgb8(8, 8)
        .save(dir.path().join("public/images/photo.webp"))
        .unwrap();
    fs::write(dir.path().join("wx-uploader.yaml"), CONFIG).unwrap();
    let article = dir.path().join("src/posts/2026/test.md");
    fs::write(&article, ARTICLE).unwrap();
    let site = SiteConfig::load(&article, None).unwrap().unwrap();
    (dir, article, site)
}
struct Mock {
    calls: AtomicUsize,
    fail: bool,
}
#[async_trait::async_trait]
impl WeChatUploader for Mock {
    async fn upload(&self, path: &str) -> Result<String> {
        assert!(Path::new(path).is_file());
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(crate::Error::wechat("mock upload failed"))
        } else {
            Ok("draft-123".into())
        }
    }
}
fn config(site: &SiteConfig) -> Config {
    Config::new("account-a".into(), "secret".into(), None, None, false)
        .with_site(Some(site.clone()))
}

#[test]
fn discovery_explicit_override_and_strict_validation() {
    let (dir, article, _) = fixture();
    assert!(SiteConfig::load(&article, None).unwrap().is_some());
    let override_path = dir.path().join("other.yaml");
    fs::write(&override_path, "version: 1").unwrap();
    assert!(
        SiteConfig::load(&article, Some(&override_path))
            .unwrap()
            .unwrap()
            .content
            .include
            .is_empty()
    );
    for invalid in [
        "version: 2",
        "version: 1\nunknown: true",
        "version: 1\ncover:\n  typo: true",
        "version: 1\ncontent:\n  include: ['[']",
        "version: 1\nfrontmatter:\n  description: excerpt",
        "version: 1\nstate:\n  storage: sidecar",
        "version: 1\nfrontmatter:\n  title: published",
        "version: 1\ntheme: invalid",
        "version: 1\ncode: invalid",
        "version: 1\ntheme: ''",
        "version: 1\ncode: ''",
    ] {
        fs::write(&override_path, invalid).unwrap();
        assert!(
            SiteConfig::load(&article, Some(&override_path)).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn site_theme_defaults_and_article_overrides() {
    let (dir, article, _) = fixture();
    fs::write(
        dir.path().join("wx-uploader.yaml"),
        "version: 1\ntheme: lapis\ncode: solarized-light\n",
    )
    .unwrap();
    let site = SiteConfig::load(&article, None).unwrap().unwrap();
    for (content, theme, code) in [
        ("body", "lapis", "solarized-light"),
        ("---\ntitle: Test\n---\nbody", "lapis", "solarized-light"),
        (
            "---\ntheme: null\ncode: null\n---\nbody",
            "lapis",
            "solarized-light",
        ),
        ("---\ntheme: maize\n---\nbody", "maize", "solarized-light"),
        ("---\ncode: github\n---\nbody", "lapis", "github"),
        (
            "---\ntheme: default\ncode: github\n---\nbody",
            "default",
            "github",
        ),
    ] {
        let (frontmatter, body) = parse_markdown_with_site(content, Some(&site)).unwrap();
        assert_eq!(frontmatter.theme.as_deref(), Some(theme));
        assert_eq!(frontmatter.code.as_deref(), Some(code));
        assert_eq!(body, "body");
    }
    assert!(parse_markdown_with_site("---\ntheme: invalid\n---\nbody", Some(&site)).is_err());
    assert!(parse_markdown_with_site("---\ncode: invalid\n---\nbody", Some(&site)).is_err());
    for site in [None, Some(serde_yaml::from_str("version: 1").unwrap())] {
        let (frontmatter, _) = parse_markdown_with_site("body", site.as_ref()).unwrap();
        assert!(frontmatter.theme.is_none());
        assert!(frontmatter.code.is_none());
    }
}

#[test]
fn root_resolution_contains_paths_and_symlinks() {
    let (dir, article, site) = fixture();
    let expected = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("public/images/cover.svg");
    assert_eq!(
        site.resolve_image(&article, "/images/cover.svg").unwrap(),
        expected
    );
    assert_eq!(
        site.resolve_image(&article, "images/cover.svg").unwrap(),
        expected
    );
    assert!(site.resolve_image(&article, "../outside.svg").is_err());
    assert!(
        site.resolve_image(&article, "images/../../missing.svg")
            .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("src"), dir.path().join("public/outside"))
            .unwrap();
        assert!(
            site.resolve_image(&article, "/outside/posts/2026/test.md")
                .is_err()
        );
        assert!(
            site.resolve_image(&article, "/outside/missing.svg")
                .is_err()
        );
    }
}

#[tokio::test]
async fn qiaopang_upload_copy_preserves_source_and_entire_cover() {
    let (dir, article, mut site) = fixture();
    site.theme = Some("lapis".into());
    site.code = Some("solarized-light".into());
    let (frontmatter, body) = parse_markdown_with_site(ARTICLE, Some(&site)).unwrap();
    assert_eq!(frontmatter.description, "网站的摘要");
    assert!(frontmatter.is_published());
    let files = prepare_upload_files_with_site(&article, &frontmatter, &body, Some(&site), true)
        .await
        .unwrap();
    let cover = image::open(files.cover.as_ref().unwrap().path())
        .unwrap()
        .to_rgba8();
    assert_eq!(cover.dimensions(), (297, 126));
    assert_eq!(cover.get_pixel(148, 125).0, [0, 255, 0, 255]);
    assert_eq!(cover.get_pixel(0, 0).0, [255, 255, 255, 255]);
    let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
        .parse_file(files.markdown.path())
        .await
        .unwrap();
    assert_eq!(parsed.description.as_deref(), Some("网站的摘要"));
    assert_eq!(parsed.theme.as_deref(), Some("lapis"));
    assert_eq!(parsed.code.as_deref(), Some("solarized-light"));
    assert_eq!(parsed.images.len(), 2);
    assert!(parsed.images[0].original_url.ends_with(".png"));
    assert!(parsed.images[1].original_url.ends_with(".webp"));
    assert_eq!(fs::read_to_string(&article).unwrap(), ARTICLE);
    assert_eq!(
        fs::read_to_string(dir.path().join("public/images/cover.svg")).unwrap(),
        SVG
    );
    assert!(!dir.path().join(".wx-uploader").exists());
    assert_eq!(check_path(dir.path(), Some(&site)).await.unwrap(), 1);
    let outside = dir.path().join("notes.md");
    fs::write(&outside, ARTICLE).unwrap();
    assert!(check_path(&outside, Some(&site)).await.is_err());
}

#[tokio::test]
async fn sidecar_tracks_changes_force_and_account_without_source_writeback() {
    let (dir, article, site) = fixture();
    let mock = Mock {
        calls: AtomicUsize::new(0),
        fail: false,
    };
    let mut cfg = config(&site);
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    assert_eq!(mock.calls.load(Ordering::SeqCst), 1);
    fs::write(
        dir.path().join("public/images/diagram.svg"),
        SVG.replace("ff0000", "0000ff"),
    )
    .unwrap();
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    assert_eq!(mock.calls.load(Ordering::SeqCst), 2);
    cfg.site.as_mut().unwrap().cover.fit = crate::site::CoverFit::Crop;
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    assert_eq!(mock.calls.load(Ordering::SeqCst), 3);
    fs::write(&article, ARTICLE.replace("正文", "修改后的正文")).unwrap();
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    upload_file(&mock, &cfg, &article, true, false)
        .await
        .unwrap();
    cfg.wechat_app_id = "account-b".into();
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    assert_eq!(mock.calls.load(Ordering::SeqCst), 6);
    assert_eq!(
        fs::read_to_string(&article).unwrap(),
        ARTICLE.replace("正文", "修改后的正文")
    );
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join(".wx-uploader/state.json")).unwrap())
            .unwrap();
    assert_eq!(
        state["accounts"]["account-b"]["src/posts/2026/test.md"]["draft_id"],
        "draft-123"
    );
}

#[tokio::test]
async fn errors_never_upload_or_record_success() {
    let (dir, article, site) = fixture();
    let fail = Mock {
        calls: AtomicUsize::new(0),
        fail: true,
    };
    assert!(
        upload_file(&fail, &config(&site), &article, false, false)
            .await
            .is_err()
    );
    assert!(!dir.path().join(".wx-uploader/state.json").exists());
    let lock = UploadState::open(&site).unwrap();
    assert!(UploadState::open(&site).is_err());
    drop(lock);
    fs::write(dir.path().join(".wx-uploader/state.json"), "broken").unwrap();
    assert!(
        upload_file(&fail, &config(&site), &article, false, false)
            .await
            .is_err()
    );
    assert_eq!(fail.calls.load(Ordering::SeqCst), 1);
    fs::remove_file(dir.path().join(".wx-uploader/state.json")).unwrap();
    fs::remove_file(dir.path().join("public/images/cover.svg")).unwrap();
    assert!(
        upload_file(&fail, &config(&site), &article, true, false)
            .await
            .is_err()
    );
    assert_eq!(fail.calls.load(Ordering::SeqCst), 1);
    fs::write(&article, "---\ntitle: [invalid\n---\nbody").unwrap();
    assert!(
        upload_file(&fail, &config(&site), &article, true, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn legacy_upload_writeback_and_skip_remain_compatible() {
    let dir = TempDir::new().unwrap();
    image::DynamicImage::new_rgb8(240, 100)
        .save(dir.path().join("cover.png"))
        .unwrap();
    let article = dir.path().join("article.md");
    fs::write(
        &article,
        "---\ntitle: Legacy\npublished: true\ncover: cover.png\n---\nBody",
    )
    .unwrap();
    let cfg = Config::new("app".into(), "secret".into(), None, None, false);
    let mock = Mock {
        calls: AtomicUsize::new(0),
        fail: false,
    };
    upload_file(&mock, &cfg, &article, false, false)
        .await
        .unwrap();
    assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
    upload_file(&mock, &cfg, &article, true, false)
        .await
        .unwrap();
    assert_eq!(mock.calls.load(Ordering::SeqCst), 1);
    let (fm, _) = crate::markdown::parse_markdown_file(&article)
        .await
        .unwrap();
    assert_eq!(fm.published.as_deref(), Some("draft"));
}

#[tokio::test]
async fn offline_checks_reject_bad_rasters_and_remote_svgs_without_state() {
    let (dir, article, site) = fixture();
    fs::write(dir.path().join("public/images/photo.webp"), "not an image").unwrap();
    assert!(check_path(&article, Some(&site)).await.is_err());
    fs::write(
        &article,
        ARTICLE
            .replace("/images/diagram.svg", "http://127.0.0.1:1/diagram.svg")
            .replace("![截图](/images/photo.webp)", ""),
    )
    .unwrap();
    assert!(
        check_path(&article, Some(&site))
            .await
            .unwrap_err()
            .to_string()
            .contains("remote SVG offline")
    );
    assert!(!dir.path().join(".wx-uploader").exists());
}

#[test]
fn failed_atomic_save_includes_created_draft_id() {
    let (dir, _, site) = fixture();
    let mut state = UploadState::open(&site).unwrap();
    fs::create_dir(dir.path().join(".wx-uploader/state.json")).unwrap();
    let error = state
        .save(
            "account",
            "article.md".into(),
            "hash".into(),
            "recover-this-draft".into(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("recover-this-draft"));
}
