//! Per-site file configuration, resolved independently of API credentials.
use crate::error::{Error, Result};
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    pub version: u32,
    #[serde(default)]
    pub content: Content,
    #[serde(default)]
    pub images: Images,
    #[serde(default)]
    pub frontmatter: Fields,
    #[serde(default)]
    pub cover: Cover,
    #[serde(default)]
    pub state: State,
    #[serde(skip)]
    pub directory: PathBuf,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Content {
    pub include: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Images {
    pub root: PathBuf,
    pub relative_to: RelativeTo,
}
impl Default for Images {
    fn default() -> Self {
        Self {
            root: ".".into(),
            relative_to: RelativeTo::Article,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelativeTo {
    #[default]
    Article,
    Root,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Fields {
    pub title: String,
    pub description: String,
    pub cover: String,
}
impl Default for Fields {
    fn default() -> Self {
        Self {
            title: "title".into(),
            description: "description".into(),
            cover: "cover".into(),
        }
    }
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cover {
    pub mode: CoverMode,
    pub fit: CoverFit,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverMode {
    #[default]
    Auto,
    Existing,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverFit {
    #[default]
    Crop,
    Contain,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct State {
    pub storage: Storage,
    pub path: PathBuf,
}
impl Default for State {
    fn default() -> Self {
        Self {
            storage: Storage::Frontmatter,
            path: ".wx-uploader/state.json".into(),
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Storage {
    #[default]
    Frontmatter,
    Sidecar,
}

impl SiteConfig {
    pub fn load(input: &Path, explicit: Option<&Path>) -> Result<Option<Self>> {
        let path = if let Some(path) = explicit {
            Some(path.canonicalize()?)
        } else {
            let input = input.canonicalize()?;
            let start = if input.is_dir() {
                input.as_path()
            } else {
                input.parent().unwrap()
            };
            start
                .ancestors()
                .map(|dir| dir.join("wx-uploader.yaml"))
                .find(|path| path.is_file())
        };
        let Some(path) = path else { return Ok(None) };
        let mut config: Self = serde_yaml::from_str(&std::fs::read_to_string(&path)?)
            .map_err(|e| Error::config(format!("{}: {e}", path.display())))?;
        config.directory = path.parent().unwrap().to_path_buf();
        config.validate()?;
        Ok(Some(config))
    }
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(Error::config(
                "Unsupported site config version (expected 1)",
            ));
        }
        self.includes()?;
        let pairs = [
            ("title", &self.frontmatter.title),
            ("description", &self.frontmatter.description),
            ("cover", &self.frontmatter.cover),
        ];
        let reserved = [
            "title",
            "description",
            "desc",
            "cover",
            "published",
            "theme",
            "code",
            "model",
            "cover_prompt",
            "cover_style",
        ];
        for (i, (target, source)) in pairs.iter().enumerate() {
            if source.trim().is_empty()
                || pairs[..i].iter().any(|(_, s)| s == source)
                || (reserved.contains(&source.as_str()) && source.as_str() != *target)
            {
                return Err(Error::config(format!(
                    "Invalid or conflicting frontmatter field: {source}"
                )));
            }
        }
        if self.state.storage == Storage::Frontmatter && self.frontmatter != Fields::default() {
            return Err(Error::config(
                "Custom frontmatter mappings require state.storage: sidecar",
            ));
        }
        if self.state.storage == Storage::Sidecar && self.cover.mode != CoverMode::Existing {
            return Err(Error::config(
                "Sidecar state requires cover.mode: existing to preserve source files",
            ));
        }
        if self.state.path.as_os_str().is_empty() {
            return Err(Error::config("State path cannot be empty"));
        }
        if self.state.storage == Storage::Sidecar {
            let state_path = self.directory.join(&self.state.path);
            if state_path.is_dir() {
                return Err(Error::config("State path must be a file"));
            }
        }
        if self.images.root.as_os_str().is_empty() {
            return Err(Error::config("Image root cannot be empty"));
        }
        Ok(())
    }
    fn includes(&self) -> Result<globset::GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &self.content.include {
            if Path::new(pattern).is_absolute() || pattern.split('/').any(|part| part == "..") {
                return Err(Error::config(
                    "Content globs must be relative to the config directory",
                ));
            }
            builder.add(Glob::new(pattern).map_err(|e| Error::config(e.to_string()))?);
        }
        builder.build().map_err(|e| Error::config(e.to_string()))
    }
    pub fn matches(&self, path: &Path) -> Result<bool> {
        if self.content.include.is_empty() {
            return Ok(true);
        }
        let absolute = path.canonicalize()?;
        let globs = self.includes()?;
        Ok(absolute
            .strip_prefix(&self.directory)
            .is_ok_and(|relative| globs.is_match(relative)))
    }
    pub fn resolve_image(&self, article: &Path, reference: &str) -> Result<PathBuf> {
        if reference.starts_with("http://") || reference.starts_with("https://") {
            return Err(Error::config("Cover must be a local image"));
        }
        if self.images.relative_to == RelativeTo::Article {
            return Ok(article.parent().unwrap_or(Path::new(".")).join(reference));
        }
        let root = self.directory.join(&self.images.root).canonicalize()?;
        let candidate = root.join(reference.trim_start_matches('/'));
        // Check the nearest existing ancestor too, so missing files cannot bypass containment.
        let mut existing = candidate.as_path();
        while !existing.exists() {
            existing = existing
                .parent()
                .ok_or_else(|| Error::config("Invalid image path"))?;
        }
        if !existing.canonicalize()?.starts_with(&root)
            || candidate
                .components()
                .any(|part| part == std::path::Component::ParentDir)
        {
            return Err(Error::config(format!(
                "Image path escapes configured root: {reference}"
            )));
        }
        Ok(candidate)
    }
    pub fn sidecar(&self) -> bool {
        self.state.storage == Storage::Sidecar
    }
}

pub(crate) fn resolve_image(
    site: Option<&SiteConfig>,
    article: &Path,
    reference: &str,
) -> Result<PathBuf> {
    match site {
        Some(site) => site.resolve_image(article, reference),
        None => Ok(article.parent().unwrap_or(Path::new(".")).join(reference)),
    }
}
