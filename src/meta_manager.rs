use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::models::{InstalledMod, MetaTemplate, MetaTemplateKind};

pub const META_FILE_NAME: &str = ".anime-mod.json";

#[derive(Debug, Clone)]
pub struct MetaManager {
    roots: Vec<PathBuf>,
    index: Arc<Mutex<MetaIndex>>,
}

#[derive(Debug, Default)]
struct MetaIndex {
    by_uuid: HashMap<String, MetaIndexEntry>,
    by_kind: HashMap<MetaTemplateKind, HashSet<String>>,
}

#[derive(Debug, Clone)]
struct MetaIndexEntry {
    dir: PathBuf,
    kinds: HashSet<MetaTemplateKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaDocument {
    #[serde(default = "new_uuid_string")]
    uuid: String,
    #[serde(default)]
    templates: HashMap<MetaTemplateKind, Value>,
}

impl MetaManager {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            index: Arc::new(Mutex::new(MetaIndex::default())),
        }
    }

    pub fn scan(&self) -> Result<()> {
        let mut next_index = MetaIndex::default();

        for root in &self.roots {
            if !root.exists() {
                continue;
            }

            for entry in fs::read_dir(root).with_context(|| format!("read_dir {:?}", root))? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }

                let dir = entry.path();
                let Some(document) = self.load_document_from_dir(&dir)? else {
                    continue;
                };
                if document.templates.is_empty() {
                    continue;
                }

                let kinds: HashSet<MetaTemplateKind> = document.templates.keys().copied().collect();
                let uuid = document.uuid.clone();
                for kind in &kinds {
                    next_index
                        .by_kind
                        .entry(*kind)
                        .or_default()
                        .insert(uuid.clone());
                }
                next_index
                    .by_uuid
                    .insert(uuid, MetaIndexEntry { dir, kinds });
            }
        }

        *self.index.lock().unwrap() = next_index;
        Ok(())
    }

    pub fn read<T: MetaTemplate>(&self, uuid: &str) -> Result<Option<T>> {
        let Some(dir) = self.dir_for_uuid(uuid)? else {
            return Ok(None);
        };
        Ok(self
            .read_template_from_dir::<T>(&dir)?
            .map(|(_, template)| template))
    }

    pub fn write<T: MetaTemplate>(&self, uuid: &str, template: &T) -> Result<bool> {
        let Some(dir) = self.dir_for_uuid(uuid)? else {
            return Ok(false);
        };
        let _ = self.write_template_at_dir(&dir, Some(uuid), template)?;
        Ok(true)
    }

    pub fn exists(&self, uuid: &str) -> bool {
        if self.index.lock().unwrap().by_uuid.is_empty() {
            let _ = self.scan();
        }
        self.index.lock().unwrap().by_uuid.contains_key(uuid)
    }

    pub fn uuids_for(&self, kind: MetaTemplateKind) -> HashSet<String> {
        if self.index.lock().unwrap().by_uuid.is_empty() {
            let _ = self.scan();
        }
        self.index
            .lock()
            .unwrap()
            .by_kind
            .get(&kind)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn read_template_from_dir<T: MetaTemplate>(
        &self,
        dir: &Path,
    ) -> Result<Option<(String, T)>> {
        let Some(document) = self.load_document_from_dir(dir)? else {
            return Ok(None);
        };
        let Some(value) = document.templates.get(&T::KIND).cloned() else {
            return Ok(None);
        };
        let template = serde_json::from_value::<T>(value)?;
        self.refresh_index_entry(dir, &document);
        Ok(Some((document.uuid, template)))
    }

    pub(crate) fn write_template_at_dir<T: MetaTemplate>(
        &self,
        dir: &Path,
        uuid: Option<&str>,
        template: &T,
    ) -> Result<String> {
        fs::create_dir_all(dir)?;
        let mut document = self
            .load_document_from_dir(dir)?
            .unwrap_or_else(|| MetaDocument {
                uuid: uuid
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(new_uuid_string),
                templates: HashMap::new(),
            });

        if let Some(uuid) = uuid.filter(|value| !value.is_empty()) {
            if document.uuid.is_empty() {
                document.uuid = uuid.to_string();
            } else if document.uuid != uuid {
                anyhow::bail!(
                    "meta uuid mismatch for {:?}: {} != {}",
                    dir,
                    document.uuid,
                    uuid
                );
            }
        }
        if document.uuid.is_empty() {
            document.uuid = new_uuid_string();
        }

        document
            .templates
            .insert(T::KIND, serde_json::to_value(template)?);
        self.write_document_to_dir(dir, &document)?;
        self.refresh_index_entry(dir, &document);
        Ok(document.uuid)
    }

    pub(crate) fn remove_template_at_dir<T: MetaTemplate>(&self, dir: &Path) -> Result<bool> {
        let Some(mut document) = self.load_document_from_dir(dir)? else {
            return Ok(false);
        };
        let removed = document.templates.remove(&T::KIND).is_some();
        if !removed {
            return Ok(false);
        }

        if document.templates.is_empty() {
            let metadata_path = dir.join(META_FILE_NAME);
            if metadata_path.exists() {
                fs::remove_file(&metadata_path)?;
            }
            self.remove_index_entry_by_dir(dir);
            return Ok(true);
        }

        self.write_document_to_dir(dir, &document)?;
        self.refresh_index_entry(dir, &document);
        Ok(true)
    }

    fn dir_for_uuid(&self, uuid: &str) -> Result<Option<PathBuf>> {
        if let Some(entry) = self.index.lock().unwrap().by_uuid.get(uuid).cloned() {
            return Ok(Some(entry.dir));
        }
        self.scan()?;
        Ok(self
            .index
            .lock()
            .unwrap()
            .by_uuid
            .get(uuid)
            .map(|entry| entry.dir.clone()))
    }

    fn load_document_from_dir(&self, dir: &Path) -> Result<Option<MetaDocument>> {
        let metadata_path = dir.join(META_FILE_NAME);
        if !metadata_path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&metadata_path)
            .with_context(|| format!("read meta file {:?}", metadata_path))?;
        if data.trim().is_empty() {
            return Ok(None);
        }

        if let Ok(mut document) = serde_json::from_str::<MetaDocument>(&data) {
            if document.uuid.is_empty() {
                document.uuid = new_uuid_string();
                self.write_document_to_dir(dir, &document)?;
            }
            return Ok(Some(document));
        }

        if let Ok(legacy) = serde_json::from_str::<InstalledMod>(&data) {
            let converted = legacy_to_document(legacy);
            self.write_document_to_dir(dir, &converted)?;
            return Ok(Some(converted));
        }

        Ok(None)
    }

    fn write_document_to_dir(&self, dir: &Path, document: &MetaDocument) -> Result<()> {
        fs::create_dir_all(dir)?;
        let metadata_path = dir.join(META_FILE_NAME);
        fs::write(metadata_path, serde_json::to_string_pretty(document)?)?;
        Ok(())
    }

    fn refresh_index_entry(&self, dir: &Path, document: &MetaDocument) {
        let mut index = self.index.lock().unwrap();
        index.remove_uuid(&document.uuid);

        let kinds: HashSet<MetaTemplateKind> = document.templates.keys().copied().collect();
        if kinds.is_empty() {
            return;
        }

        for kind in &kinds {
            index
                .by_kind
                .entry(*kind)
                .or_default()
                .insert(document.uuid.clone());
        }
        index.by_uuid.insert(
            document.uuid.clone(),
            MetaIndexEntry {
                dir: dir.to_path_buf(),
                kinds,
            },
        );
    }

    fn remove_index_entry_by_dir(&self, dir: &Path) {
        let mut index = self.index.lock().unwrap();
        let Some((uuid, _)) = index
            .by_uuid
            .iter()
            .find(|(_, entry)| entry.dir == dir)
            .map(|(uuid, entry)| (uuid.clone(), entry.clone()))
        else {
            return;
        };
        index.remove_uuid(&uuid);
    }
}

impl MetaIndex {
    fn remove_uuid(&mut self, uuid: &str) {
        if let Some(previous) = self.by_uuid.remove(uuid) {
            for kind in previous.kinds {
                if let Some(items) = self.by_kind.get_mut(&kind) {
                    items.remove(uuid);
                    if items.is_empty() {
                        self.by_kind.remove(&kind);
                    }
                }
            }
        }
    }
}

fn legacy_to_document(legacy: InstalledMod) -> MetaDocument {
    let uuid = if legacy.meta_uuid.trim().is_empty() {
        new_uuid_string()
    } else {
        legacy.meta_uuid.clone()
    };
    let (mod_meta, download_meta) = legacy.split_templates();
    let mut templates = HashMap::new();
    templates.insert(
        MetaTemplateKind::Mod,
        serde_json::to_value(mod_meta).unwrap_or(Value::Null),
    );
    if let Some(download_meta) = download_meta {
        templates.insert(
            MetaTemplateKind::Download,
            serde_json::to_value(download_meta).unwrap_or(Value::Null),
        );
    }
    MetaDocument { uuid, templates }
}

fn new_uuid_string() -> String {
    Uuid::new_v4().to_string()
}
