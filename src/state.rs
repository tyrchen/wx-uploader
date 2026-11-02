//! Account-scoped draft state. Hold an OS lock through upload and atomic save.
use crate::{
    error::{Error, Result},
    site::SiteConfig,
    upload_images::TempUploadFiles,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Store {
    version: u32,
    accounts: BTreeMap<String, BTreeMap<String, Record>>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub fingerprint: String,
    pub draft_id: String,
    pub uploaded_at: u64,
}
pub(crate) struct UploadState {
    path: PathBuf,
    _lock: File,
    store: Store,
}
impl UploadState {
    pub fn open(site: &SiteConfig) -> Result<Self> {
        let path = site.directory.join(&site.state.path);
        let parent = path
            .parent()
            .ok_or_else(|| Error::config("State file has no parent"))?;
        std::fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("json.lock"))?;
        lock.try_lock().map_err(|e| {
            Error::config(format!(
                "State store is locked or unavailable ({}): {e}",
                path.display()
            ))
        })?;
        let store = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Store>(&bytes)
                .map_err(|e| Error::config(format!("Invalid state {}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store {
                version: 1,
                accounts: BTreeMap::new(),
            },
            Err(e) => return Err(e.into()),
        };
        if store.version != 1 {
            return Err(Error::config("Unsupported state version"));
        }
        Ok(Self {
            path,
            _lock: lock,
            store,
        })
    }
    pub fn key(site: &SiteConfig, article: &Path) -> Result<String> {
        Ok(article
            .canonicalize()?
            .strip_prefix(&site.directory)
            .map_err(|_| Error::config("Sidecar articles must be inside the config directory"))?
            .to_string_lossy()
            .replace('\\', "/"))
    }
    pub fn unchanged(&self, account: &str, key: &str, fingerprint: &str) -> bool {
        self.store
            .accounts
            .get(account)
            .and_then(|entries| entries.get(key))
            .is_some_and(|record| record.fingerprint == fingerprint)
    }
    pub fn save(
        &mut self,
        account: &str,
        key: String,
        fingerprint: String,
        draft_id: String,
    ) -> Result<()> {
        self.store
            .accounts
            .entry(account.into())
            .or_default()
            .insert(
                key,
                Record {
                    fingerprint,
                    draft_id: draft_id.clone(),
                    uploaded_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| Error::generic(e.to_string()))?
                        .as_secs(),
                },
            );
        let result = (|| -> Result<()> {
            let mut file = tempfile::NamedTempFile::new_in(self.path.parent().unwrap())?;
            file.write_all(&serde_json::to_vec_pretty(&self.store)?)?;
            file.as_file().sync_all()?;
            file.persist(&self.path).map_err(|e| Error::Io(e.error))?;
            Ok(())
        })();
        result.map_err(|e| {
            Error::generic(format!(
                "Draft {draft_id} was uploaded, but state could not be saved: {e}"
            ))
        })
    }
}

pub(crate) async fn fingerprint(
    article: &Path,
    site: &SiteConfig,
    files: &TempUploadFiles,
) -> Result<String> {
    let mut hash = blake3::Hasher::new();
    let mut add = |bytes: &[u8]| {
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    // A rendering change must invalidate drafts even when the article is unchanged.
    add(b"wechat-code-layout-v1");
    add(&std::fs::read(article)?);
    add(&serde_json::to_vec(site)?);
    let parsed = wechat_pub_rs::markdown::MarkdownParser::new()
        .parse_file(files.markdown.path())
        .await
        .map_err(|e| Error::generic(e.to_string()))?;
    if let Some(cover) = parsed.cover {
        add(&std::fs::read(cover)?);
    }
    for image in parsed.images {
        if image.is_local {
            add(&std::fs::read(image.original_url)?);
        }
    }
    Ok(hash.finalize().to_hex().to_string())
}
