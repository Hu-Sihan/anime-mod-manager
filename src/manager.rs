use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

use crate::meta_manager::{MetaManager, META_FILE_NAME};
use crate::models::*;

const MOD_MEDIA_DIR_NAME: &str = ".anime-mod-media";

#[derive(Debug, Clone)]
pub struct ModManager {
    mods_dir: PathBuf,
    legacy_installed_json: PathBuf,
    meta_manager: MetaManager,
}

impl ModManager {
    pub fn new(mods_dir: impl Into<PathBuf>, meta_manager: MetaManager) -> Self {
        let mods_dir = mods_dir.into();
        let legacy_installed_json = mods_dir.join(".installed.json");
        Self {
            meta_manager,
            mods_dir,
            legacy_installed_json,
        }
    }

    pub fn init(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.mods_dir)?;
        fs::create_dir_all(self.disabled_mods_dir())?;
        self.migrate_legacy_installed_index()?;
        Ok(())
    }

    pub fn initialize_from_meta(&self) -> anyhow::Result<()> {
        let _ = self.meta_manager.uuids_for(MetaTemplateKind::Mod);
        Ok(())
    }

    pub fn meta_roots_for(mods_dir: impl AsRef<Path>) -> Vec<PathBuf> {
        let mods_dir = mods_dir.as_ref();
        vec![mods_dir.to_path_buf(), disabled_root_for(mods_dir)]
    }

    pub fn mods_dir(&self) -> &Path {
        &self.mods_dir
    }

    pub fn disabled_mods_dir(&self) -> PathBuf {
        disabled_root_for(&self.mods_dir)
    }

    pub fn install(
        &self,
        archive_path: &Path,
        mod_card: &ModCard,
        folder_name: &str,
    ) -> anyhow::Result<()> {
        let existing = self.read_entry_for_folder(folder_name)?;
        let enabled = existing
            .as_ref()
            .map(|(item, _)| item.enabled)
            .unwrap_or(true);
        let local_cover_path = existing
            .as_ref()
            .and_then(|(item, _)| item.local_cover_path.clone());
        let local_detail = existing
            .as_ref()
            .and_then(|(item, _)| item.local_detail.clone());
        let current_file_name = existing.as_ref().and_then(|(item, _)| {
            item.active_download
                .as_ref()
                .map(|download| download.file_name.clone())
                .or_else(|| item.current_file_name.clone())
        });
        let target_dir = self.target_dir_for_enabled(folder_name, enabled);
        let other_dir = self.target_dir_for_enabled(folder_name, !enabled);

        if other_dir.exists() {
            fs::remove_dir_all(&other_dir)?;
        }
        fs::create_dir_all(&target_dir)?;
        self.clear_install_target(&target_dir, archive_path)?;

        let archive_name = current_file_name
            .clone()
            .or_else(|| {
                archive_path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .unwrap_or_default();
        match detect_archive_kind(&archive_name) {
            Some(ArchiveKind::Zip) => extract_zip_archive(archive_path, &target_dir)?,
            Some(ArchiveKind::SevenZip) => extract_with_7z(archive_path, &target_dir)?,
            Some(ArchiveKind::Rar) => extract_rar_archive(archive_path, &target_dir)?,
            None => {
                anyhow::bail!("格式不支持：{}", archive_name);
            }
        }

        let now = now_unix();
        self.write_entry_at(
            &target_dir,
            &InstalledMod {
                meta_uuid: existing
                    .as_ref()
                    .map(|(item, _)| item.meta_uuid.clone())
                    .unwrap_or_default(),
                name: mod_card.name.clone(),
                mod_id: mod_card.id,
                installed_at: now,
                folder: folder_name.to_string(),
                remote_date_modified: mod_card.date_modified,
                author: mod_card.author.clone(),
                category: mod_card.category.clone(),
                subcategory: mod_card.subcategory.clone(),
                is_r18: mod_card.is_r18,
                local_cover_path,
                thumbnail_url: mod_card.thumbnail_url.clone(),
                cover_url: mod_card.cover_url.clone(),
                profile_url: mod_card.profile_url.clone(),
                enabled,
                update_available: false,
                state: ModEntryState::Installed,
                current_file_name,
                active_download: None,
                local_detail,
            },
        )?;

        Ok(())
    }

    pub fn prepare_download(
        &self,
        mod_card: &ModCard,
        file: &ModFile,
        folder_name: &str,
    ) -> anyhow::Result<()> {
        let existing = self.read_entry_for_folder(folder_name)?.or_else(|| {
            self.get_record(mod_card.id).ok().flatten().map(|item| {
                let dir = self.target_dir_for_enabled(&item.folder, item.enabled);
                (item, dir)
            })
        });

        let enabled = existing
            .as_ref()
            .map(|(item, _)| item.enabled)
            .unwrap_or(true);
        let state = if existing
            .as_ref()
            .map(|(item, _)| item.is_installed())
            .unwrap_or(false)
        {
            ModEntryState::Installed
        } else {
            ModEntryState::Pending
        };
        let target_dir = existing
            .as_ref()
            .map(|(_, dir)| dir.clone())
            .unwrap_or_else(|| self.target_dir_for_enabled(folder_name, enabled));
        fs::create_dir_all(&target_dir)?;

        let now = now_unix();
        let mut item = existing.map(|(item, _)| item).unwrap_or(InstalledMod {
            meta_uuid: String::new(),
            name: mod_card.name.clone(),
            mod_id: mod_card.id,
            installed_at: now,
            folder: folder_name.to_string(),
            remote_date_modified: mod_card.date_modified,
            author: mod_card.author.clone(),
            category: mod_card.category.clone(),
            subcategory: mod_card.subcategory.clone(),
            is_r18: mod_card.is_r18,
            local_cover_path: None,
            thumbnail_url: mod_card.thumbnail_url.clone(),
            cover_url: mod_card.cover_url.clone(),
            profile_url: mod_card.profile_url.clone(),
            enabled,
            update_available: false,
            state,
            current_file_name: Some(file.filename.clone()),
            active_download: None,
            local_detail: None,
        });

        item.name = mod_card.name.clone();
        item.mod_id = mod_card.id;
        item.folder = folder_name.to_string();
        item.remote_date_modified = mod_card.date_modified;
        item.author = mod_card.author.clone();
        item.category = mod_card.category.clone();
        item.subcategory = mod_card.subcategory.clone();
        item.is_r18 = mod_card.is_r18;
        item.thumbnail_url = mod_card.thumbnail_url.clone();
        item.cover_url = mod_card.cover_url.clone();
        item.profile_url = mod_card.profile_url.clone();
        item.enabled = enabled;
        item.update_available = false;
        item.state = state;
        item.current_file_name = Some(file.filename.clone());
        item.active_download = Some(DownloadCheckpoint {
            file_id: file.id,
            file_name: file.filename.clone(),
            temp_file_path: None,
            downloaded_bytes: 0,
            total_bytes: file.size,
            phase: DownloadCheckpointPhase::Started,
            status_code: DOWNLOAD_STATUS_STARTED,
            debug_detail: None,
            updated_at: now,
        });

        self.write_entry_at(&target_dir, &item)
    }

    pub fn update_local_detail(
        &self,
        folder_name: &str,
        detail: LocalModDetail,
    ) -> anyhow::Result<()> {
        let Some((mut item, dir)) = self.read_entry_for_folder(folder_name)? else {
            anyhow::bail!("mod metadata not found for folder {}", folder_name);
        };
        item.local_detail = Some(detail);
        self.write_entry_at(&dir, &item)
    }

    pub fn store_local_cover(
        &self,
        folder_name: &str,
        source_url: &str,
        bytes: &[u8],
    ) -> anyhow::Result<String> {
        let Some((mut item, dir)) = self.read_entry_for_folder(folder_name)? else {
            anyhow::bail!("mod metadata not found for folder {}", folder_name);
        };

        let ext = cover_extension_from_url(source_url);
        let relative_path = format!(".anime-mod-media/cover.{ext}");
        let absolute_path = dir.join(&relative_path);
        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&absolute_path, bytes)?;

        item.local_cover_path = Some(relative_path.clone());
        self.write_entry_at(&dir, &item)?;
        Ok(relative_path)
    }

    pub fn update_download_progress(
        &self,
        folder_name: &str,
        phase: DownloadCheckpointPhase,
        status_code: u16,
        downloaded_bytes: u64,
        total_bytes: u64,
        temp_file_path: Option<&Path>,
        debug_detail: Option<&str>,
    ) -> anyhow::Result<()> {
        let Some((mut item, dir)) = self.read_entry_for_folder(folder_name)? else {
            anyhow::bail!("mod metadata not found for folder {}", folder_name);
        };

        let file_name = item
            .active_download
            .as_ref()
            .map(|download| download.file_name.clone())
            .or_else(|| item.current_file_name.clone())
            .unwrap_or_default();
        let file_id = item
            .active_download
            .as_ref()
            .map(|download| download.file_id)
            .unwrap_or_default();

        item.active_download = Some(DownloadCheckpoint {
            file_id,
            file_name,
            temp_file_path: temp_file_path.map(|path| path.to_string_lossy().to_string()),
            downloaded_bytes,
            total_bytes,
            phase,
            status_code,
            debug_detail: debug_detail.map(|value| value.to_string()),
            updated_at: now_unix(),
        });

        self.write_entry_at(&dir, &item)
    }

    pub fn mark_download_failed(
        &self,
        folder_name: &str,
        status_code: u16,
        debug_detail: &str,
    ) -> anyhow::Result<()> {
        let Some((mut item, dir)) = self.read_entry_for_folder(folder_name)? else {
            anyhow::bail!("mod metadata not found for folder {}", folder_name);
        };

        if let Some(download) = item.active_download.as_mut() {
            download.phase = DownloadCheckpointPhase::Failed;
            download.status_code = status_code;
            download.debug_detail = Some(debug_detail.to_string());
            download.updated_at = now_unix();
        }

        self.write_entry_at(&dir, &item)
    }

    pub fn clear_active_download(&self, folder_name: &str) -> anyhow::Result<()> {
        let Some((mut item, dir)) = self.read_entry_for_folder(folder_name)? else {
            return Ok(());
        };

        if let Some(temp_file_path) = item
            .active_download
            .as_ref()
            .and_then(|download| download.temp_file_path.as_ref())
        {
            let temp_path = PathBuf::from(temp_file_path);
            if temp_path.exists() {
                let _ = fs::remove_file(temp_path);
            }
        }

        item.active_download = None;
        self.write_entry_at(&dir, &item)
    }

    pub fn uninstall(&self, folder_name: &str) -> anyhow::Result<()> {
        let dest = self.mods_dir.join(folder_name);
        if dest.exists() {
            fs::remove_dir_all(&dest)?;
        }
        let disabled_dest = self.disabled_mods_dir().join(folder_name);
        if disabled_dest.exists() {
            fs::remove_dir_all(&disabled_dest)?;
        }
        Ok(())
    }

    pub fn disable_mod(&self, folder_name: &str) -> anyhow::Result<()> {
        self.move_between_roots(folder_name, false)
    }

    pub fn enable_mod(&self, folder_name: &str) -> anyhow::Result<()> {
        self.move_between_roots(folder_name, true)
    }

    pub fn list_installed(&self) -> anyhow::Result<Vec<InstalledMod>> {
        Ok(self
            .list_all_entries()?
            .into_iter()
            .filter(InstalledMod::is_installed)
            .collect())
    }

    pub fn list_active_downloads(&self) -> anyhow::Result<Vec<InstalledMod>> {
        Ok(self
            .list_all_entries()?
            .into_iter()
            .filter(InstalledMod::has_active_download)
            .collect())
    }

    pub fn is_installed(&self, mod_id: u64) -> anyhow::Result<bool> {
        Ok(self.list_installed()?.iter().any(|m| m.mod_id == mod_id))
    }

    pub fn get_installed(&self, mod_id: u64) -> anyhow::Result<Option<InstalledMod>> {
        Ok(self
            .list_installed()?
            .into_iter()
            .find(|m| m.mod_id == mod_id))
    }

    pub fn get_record(&self, mod_id: u64) -> anyhow::Result<Option<InstalledMod>> {
        Ok(self
            .list_all_entries()?
            .into_iter()
            .find(|m| m.mod_id == mod_id))
    }

    pub fn status_all(&self) -> anyhow::Result<Vec<ModStatus>> {
        Ok(self
            .list_installed()?
            .into_iter()
            .map(|m| {
                if m.update_available {
                    ModStatus::UpdateAvailable {
                        latest_date: m.remote_date_modified,
                        installed: m,
                    }
                } else {
                    ModStatus::Installed(m)
                }
            })
            .collect())
    }

    pub fn check_updates(
        &self,
        client: &crate::gamebanana::GameBananaClient,
    ) -> anyhow::Result<Vec<InstalledMod>> {
        let mut updates = Vec::new();
        for mut m in self.list_installed()? {
            if let Ok(detail) = client.get_mod(m.mod_id) {
                if detail.date_modified > m.remote_date_modified {
                    m.update_available = true;
                    updates.push(m);
                }
            }
        }
        Ok(updates)
    }

    pub fn mod_folder(&self, folder_name: &str) -> PathBuf {
        self.target_dir_for_enabled(folder_name, true)
    }

    pub fn entry_dir(&self, folder_name: &str, enabled: bool) -> PathBuf {
        self.target_dir_for_enabled(folder_name, enabled)
    }

    fn move_between_roots(&self, folder_name: &str, enabled: bool) -> anyhow::Result<()> {
        let Some((mut item, source)) = self.read_entry_for_folder(folder_name)? else {
            return Ok(());
        };

        let destination = self.target_dir_for_enabled(folder_name, enabled);
        if source != destination {
            if destination.exists() {
                fs::remove_dir_all(&destination)?;
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&source, &destination)?;
        }

        item.enabled = enabled;
        self.write_entry_at(&destination, &item)
    }

    fn list_all_entries(&self) -> anyhow::Result<Vec<InstalledMod>> {
        let mut output = Vec::new();

        for uuid in self.meta_manager.uuids_for(MetaTemplateKind::Mod) {
            let Some(mod_meta) = self.meta_manager.read::<ModMetaTemplate>(&uuid)? else {
                continue;
            };
            let download_meta = self.meta_manager.read::<DownloadMetaTemplate>(&uuid)?;
            output.push(InstalledMod::from_templates(uuid, mod_meta, download_meta));
        }

        Ok(output)
    }

    fn read_entry_for_folder(
        &self,
        folder_name: &str,
    ) -> anyhow::Result<Option<(InstalledMod, PathBuf)>> {
        let primary = self.mods_dir.join(folder_name);
        if let Some(item) = self.read_entry_at(&primary, true)? {
            return Ok(Some((item, primary)));
        }

        let secondary = self.disabled_mods_dir().join(folder_name);
        if let Some(item) = self.read_entry_at(&secondary, false)? {
            return Ok(Some((item, secondary)));
        }

        Ok(None)
    }

    fn read_entry_at(&self, dir: &Path, enabled: bool) -> anyhow::Result<Option<InstalledMod>> {
        let Some((uuid, mod_meta)) = self
            .meta_manager
            .read_template_from_dir::<ModMetaTemplate>(dir)?
        else {
            return Ok(None);
        };
        let download_meta = self
            .meta_manager
            .read_template_from_dir::<DownloadMetaTemplate>(dir)?
            .map(|(_, template)| template);

        let mut item = InstalledMod::from_templates(uuid, mod_meta, download_meta);
        item.folder = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| item.folder.clone());
        item.enabled = enabled;
        Ok(Some(item))
    }

    fn write_entry_at(&self, dir: &Path, item: &InstalledMod) -> anyhow::Result<()> {
        fs::create_dir_all(dir)?;
        let mut normalized = item.clone();
        normalized.folder = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| normalized.folder.clone());
        let (mod_meta, download_meta) = normalized.split_templates();
        let uuid = self.meta_manager.write_template_at_dir(
            dir,
            if normalized.meta_uuid.trim().is_empty() {
                None
            } else {
                Some(normalized.meta_uuid.as_str())
            },
            &mod_meta,
        )?;
        if let Some(download_meta) = download_meta {
            let _ = self
                .meta_manager
                .write_template_at_dir(dir, Some(&uuid), &download_meta)?;
        } else {
            let _ = self
                .meta_manager
                .remove_template_at_dir::<DownloadMetaTemplate>(dir)?;
        }
        Ok(())
    }

    fn clear_install_target(&self, dir: &Path, archive_path: &Path) -> anyhow::Result<()> {
        let preserve_archive = archive_path.file_name().map(|name| name.to_os_string());

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let path = entry.path();

            if name == META_FILE_NAME || name == MOD_MEDIA_DIR_NAME {
                continue;
            }
            if preserve_archive
                .as_ref()
                .is_some_and(|archive_name| archive_name == &name)
            {
                continue;
            }

            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }

        Ok(())
    }

    fn migrate_legacy_installed_index(&self) -> anyhow::Result<()> {
        if !self.legacy_installed_json.exists() {
            return Ok(());
        }

        let data = fs::read_to_string(&self.legacy_installed_json)?;
        let legacy_items: Vec<InstalledMod> = serde_json::from_str(&data).unwrap_or_default();
        for mut item in legacy_items {
            item.meta_uuid = String::new();
            item.state = ModEntryState::Installed;
            item.active_download = None;
            let dir = self.target_dir_for_enabled(&item.folder, item.enabled);
            if !dir.exists() {
                continue;
            }
            if dir.join(META_FILE_NAME).exists() {
                continue;
            }
            self.write_entry_at(&dir, &item)?;
        }

        Ok(())
    }

    fn target_dir_for_enabled(&self, folder_name: &str, enabled: bool) -> PathBuf {
        if enabled {
            self.mods_dir.join(folder_name)
        } else {
            self.disabled_mods_dir().join(folder_name)
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn disabled_root_for(mods_dir: &Path) -> PathBuf {
    mods_dir.parent().unwrap_or(mods_dir).join(".disabled_mods")
}

fn cover_extension_from_url(source_url: &str) -> &'static str {
    let lower = source_url.to_ascii_lowercase();
    if lower.contains(".png") {
        "png"
    } else if lower.contains(".webp") {
        "webp"
    } else if lower.contains(".gif") {
        "gif"
    } else if lower.contains(".bmp") {
        "bmp"
    } else {
        "jpg"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    SevenZip,
    Rar,
}

fn detect_archive_kind(file_name: &str) -> Option<ArchiveKind> {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())?
        .to_ascii_lowercase();

    match ext.as_str() {
        "zip" => Some(ArchiveKind::Zip),
        "7z" => Some(ArchiveKind::SevenZip),
        "rar" => Some(ArchiveKind::Rar),
        _ => None,
    }
}

fn extract_zip_archive(archive_path: &Path, target_dir: &Path) -> anyhow::Result<()> {
    let file = fs::File::open(archive_path)
        .with_context(|| format!("读取压缩包失败：{}", archive_path.display()))?;
    let mut archive = zip::ZipArchive::new(file).context("读取 zip 目录失败")?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("读取 zip 条目失败")?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.mangled_name();
        let entry_path = target_dir.join(&name);

        if !entry_path.starts_with(target_dir) {
            tracing::warn!("Skipping path outside mod directory: {:?}", name);
            continue;
        }

        if let Some(parent) = entry_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("写入解压目录失败：{}", parent.display()))?;
        }
        let mut out = fs::File::create(&entry_path)
            .with_context(|| format!("写入解压文件失败：{}", entry_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .with_context(|| format!("写入解压文件失败：{}", entry_path.display()))?;
    }

    Ok(())
}

fn extract_with_7z(archive_path: &Path, target_dir: &Path) -> anyhow::Result<()> {
    let output = Command::new("7z")
        .arg("x")
        .arg("-y")
        .arg(format!("-o{}", target_dir.to_string_lossy()))
        .arg(archive_path.as_os_str())
        .output()
        .map_err(|err| anyhow::anyhow!("无法启动 7z：{err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "未知错误".to_string()
    };
    anyhow::bail!("7z 解压失败：{message}");
}

fn extract_with_bsdtar(archive_path: &Path, target_dir: &Path) -> anyhow::Result<()> {
    let output = Command::new("bsdtar")
        .arg("-xf")
        .arg(archive_path.as_os_str())
        .arg("-C")
        .arg(target_dir.as_os_str())
        .output()
        .map_err(|err| anyhow::anyhow!("无法启动 bsdtar：{err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "未知错误".to_string()
    };
    anyhow::bail!("bsdtar 解压失败：{message}");
}

fn extract_rar_archive(archive_path: &Path, target_dir: &Path) -> anyhow::Result<()> {
    match extract_with_bsdtar(archive_path, target_dir) {
        Ok(()) => Ok(()),
        Err(bsdtar_err) => match extract_with_7z(archive_path, target_dir) {
            Ok(()) => Ok(()),
            Err(seven_zip_err) => anyhow::bail!(
                "RAR 解压失败。bsdtar: {}; 7z: {}",
                bsdtar_err,
                seven_zip_err
            ),
        },
    }
}
