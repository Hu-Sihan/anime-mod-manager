use crate::tr;
use std::rc::Rc;

use anime_mod_manager::{
    DownloadCheckpointPhase, DownloadMetaTemplate, InstalledMod, MetaTemplateKind, ModCard,
    ModDetail, ModFile, ModMetaTemplate, DOWNLOAD_STATUS_STARTED,
};

use super::{
    download_scheduler::{DownloadQueue, DownloadScheduler, DownloadTask, DownloadTaskPhase},
    download_task::{DownloadTaskStatusCode, DownloadTaskUpdate},
    AppState,
};

#[derive(Clone)]
pub struct DownloadModule {
    queue: Rc<DownloadQueue>,
    scheduler: DownloadScheduler,
}

impl DownloadModule {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            queue: Rc::new(DownloadQueue::new()),
            scheduler: DownloadScheduler::new(max_concurrent),
        }
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) {
        self.queue.subscribe(listener);
    }

    pub fn snapshot(&self) -> Vec<DownloadTask> {
        self.queue.snapshot()
    }

    pub fn initialize_from_meta(&self, state: &Rc<AppState>) {
        let mut failed_items = Vec::new();
        let mut started_items = Vec::new();
        let mut paused_items = Vec::new();

        for uuid in state.meta_manager.uuids_for(MetaTemplateKind::Download) {
            let Some(mod_meta) = state
                .meta_manager
                .read::<ModMetaTemplate>(&uuid)
                .ok()
                .flatten()
            else {
                continue;
            };
            let Some(download_meta) = state
                .meta_manager
                .read::<DownloadMetaTemplate>(&uuid)
                .ok()
                .flatten()
            else {
                continue;
            };

            let item = InstalledMod::from_templates(uuid, mod_meta, Some(download_meta));
            match item
                .active_download
                .as_ref()
                .map(|download| download.phase)
                .unwrap_or(DownloadCheckpointPhase::Started)
            {
                DownloadCheckpointPhase::Failed => failed_items.push(item),
                DownloadCheckpointPhase::Paused => paused_items.push(item),
                DownloadCheckpointPhase::Started => started_items.push(item),
            }
        }

        failed_items.sort_by_key(|item| {
            item.active_download
                .as_ref()
                .map(|download| download.updated_at)
                .unwrap_or_default()
        });
        paused_items.sort_by_key(|item| {
            item.active_download
                .as_ref()
                .map(|download| download.updated_at)
                .unwrap_or_default()
        });
        started_items.sort_by_key(|item| {
            item.active_download
                .as_ref()
                .map(|download| download.updated_at)
                .unwrap_or_default()
        });

        for item in failed_items {
            self.enqueue_failed_from_record(state, &item);
        }

        for item in paused_items {
            self.enqueue_paused_from_record(state, &item);
        }

        for item in started_items {
            self.enqueue_resume_from_record(state, &item);
        }
    }

    pub fn submit_fresh(
        &self,
        state: &Rc<AppState>,
        card: ModCard,
        detail: Option<ModDetail>,
        file: ModFile,
    ) -> u64 {
        let folder_name = state
            .manager
            .get_record(card.id)
            .ok()
            .flatten()
            .map(|existing| existing.folder)
            .unwrap_or_else(|| default_mod_folder_name(&card));
        let initial_image_source = resolve_initial_image_source(state, &card);
        let task_id = self.create_task_entry(
            card.id,
            card.name.clone(),
            file.id,
            file.filename.clone(),
            initial_image_source,
        );
        self.apply_task_update(
            task_id,
            DownloadTaskPhase::Queued,
            0,
            DownloadTaskStatusCode::Queued,
            tr!("download_task.queued"),
            None,
        );
        self.scheduler
            .insert_fresh(state, task_id, card, detail, file, folder_name);
        task_id
    }

    pub fn set_max_concurrent(&self, state: &Rc<AppState>, max_concurrent: usize) {
        self.scheduler.set_max_concurrent(state, max_concurrent);
    }

    pub fn start_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        self.scheduler.start_task(state, task_id)
    }

    pub fn pause_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        self.scheduler.pause_task(state, task_id)
    }

    pub fn restart_failed_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        let Some(task) = self
            .queue
            .snapshot()
            .into_iter()
            .find(|task| task.id == task_id && matches!(task.phase, DownloadTaskPhase::Failed))
        else {
            return false;
        };

        let Ok(Some(record)) = state.manager.get_record(task.mod_id) else {
            return false;
        };
        let Some(download) = record.active_download.as_ref() else {
            return false;
        };

        let archive_path = download
            .temp_file_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                state
                    .manager
                    .entry_dir(&record.folder, record.enabled)
                    .join(sanitize_archive_file_name(&download.file_name))
            });
        let _ = std::fs::remove_file(&archive_path);

        let _ = state.manager.update_download_progress(
            &record.folder,
            DownloadCheckpointPhase::Started,
            DOWNLOAD_STATUS_STARTED,
            0,
            download.total_bytes,
            Some(&archive_path),
            None,
        );

        self.scheduler.reset_retry_state_for(task_id);
        self.scheduler.insert_resume(
            state,
            task_id,
            record.mod_id,
            download.file_id,
            download.file_name.clone(),
            record.folder.clone(),
            record.is_r18,
        );
        true
    }

    #[allow(dead_code)]
    pub fn remove_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        self.scheduler.remove_task(state, task_id)
    }

    #[allow(dead_code)]
    pub fn pause_all(&self, state: &Rc<AppState>) -> usize {
        self.scheduler.pause_all(state)
    }

    pub fn has_running_tasks(&self) -> bool {
        self.scheduler.has_running_tasks()
    }

    pub fn begin_shutdown(&self, state: &Rc<AppState>) -> usize {
        self.scheduler.begin_shutdown(state)
    }

    #[allow(dead_code)]
    pub fn task_status(&self, task_id: u64) -> Option<DownloadTaskUpdate> {
        self.scheduler.task_status(task_id)
    }

    pub(crate) fn create_task_entry(
        &self,
        mod_id: u64,
        mod_name: String,
        file_id: u64,
        file_name: String,
        image_url: Option<String>,
    ) -> u64 {
        self.queue
            .create_task(mod_id, mod_name, file_id, file_name, image_url)
    }

    pub(crate) fn apply_task_update(
        &self,
        id: u64,
        phase: DownloadTaskPhase,
        progress: u8,
        status_code: DownloadTaskStatusCode,
        status_text: impl Into<String>,
        image_url: Option<String>,
    ) {
        self.queue
            .apply_update(id, phase, progress, status_code, status_text, image_url);
    }

    pub(crate) fn remove_task_entry(&self, id: u64) {
        self.queue.remove_task(id);
    }

    fn enqueue_resume_from_record(&self, state: &Rc<AppState>, item: &InstalledMod) -> u64 {
        let Some(download) = item.active_download.as_ref() else {
            return 0;
        };
        let image_source = item
            .local_cover_path
            .as_ref()
            .map(|relative| {
                state
                    .manager
                    .entry_dir(&item.folder, item.enabled)
                    .join(relative)
            })
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| item.cover_url.clone())
            .or_else(|| item.thumbnail_url.clone());
        let task_id = self.create_task_entry(
            item.mod_id,
            item.name.clone(),
            download.file_id,
            download.file_name.clone(),
            image_source,
        );
        let progress = if download.total_bytes > 0 {
            ((download.downloaded_bytes as f64 / download.total_bytes as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        } else {
            0
        };
        self.apply_task_update(
            task_id,
            DownloadTaskPhase::Queued,
            progress,
            DownloadTaskStatusCode::Queued,
            tr!("download_task.wait_resume"),
            None,
        );
        self.scheduler.insert_resume(
            state,
            task_id,
            item.mod_id,
            download.file_id,
            download.file_name.clone(),
            item.folder.clone(),
            item.is_r18,
        );
        task_id
    }

    fn enqueue_paused_from_record(&self, state: &Rc<AppState>, item: &InstalledMod) -> u64 {
        let Some(download) = item.active_download.as_ref() else {
            return 0;
        };
        let image_source = item
            .local_cover_path
            .as_ref()
            .map(|relative| {
                state
                    .manager
                    .entry_dir(&item.folder, item.enabled)
                    .join(relative)
            })
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| item.cover_url.clone())
            .or_else(|| item.thumbnail_url.clone());
        let task_id = self.create_task_entry(
            item.mod_id,
            item.name.clone(),
            download.file_id,
            download.file_name.clone(),
            image_source,
        );
        let progress = if download.total_bytes > 0 {
            ((download.downloaded_bytes as f64 / download.total_bytes as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        } else {
            0
        };
        self.apply_task_update(
            task_id,
            DownloadTaskPhase::Paused,
            progress,
            DownloadTaskStatusCode::Paused,
            tr!("download_task.paused"),
            None,
        );
        self.scheduler.store_paused(
            state,
            task_id,
            super::download_task::DownloadTaskRequest::Resume(
                super::download_task::ResumeDownloadTask {
                    task_id,
                    mod_id: item.mod_id,
                    file_id: download.file_id,
                    file_name: download.file_name.clone(),
                    folder_name: item.folder.clone(),
                    is_r18: item.is_r18,
                },
            ),
        );
        task_id
    }

    fn enqueue_failed_from_record(&self, state: &Rc<AppState>, item: &InstalledMod) -> u64 {
        let Some(download) = item.active_download.as_ref() else {
            return 0;
        };
        let image_source = item
            .local_cover_path
            .as_ref()
            .map(|relative| {
                state
                    .manager
                    .entry_dir(&item.folder, item.enabled)
                    .join(relative)
            })
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| item.cover_url.clone())
            .or_else(|| item.thumbnail_url.clone());
        let task_id = self.create_task_entry(
            item.mod_id,
            item.name.clone(),
            download.file_id,
            download.file_name.clone(),
            image_source,
        );
        let progress = if download.total_bytes > 0 {
            ((download.downloaded_bytes as f64 / download.total_bytes as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        } else {
            0
        };
        let status_text = download
            .debug_detail
            .clone()
            .unwrap_or_else(|| tr!("download_task.download_failed"));
        self.apply_task_update(
            task_id,
            DownloadTaskPhase::Failed,
            progress,
            DownloadTaskStatusCode::Failed,
            status_text,
            None,
        );
        task_id
    }
}

fn resolve_initial_image_source(state: &Rc<AppState>, card: &ModCard) -> Option<String> {
    state
        .manager
        .get_record(card.id)
        .ok()
        .flatten()
        .and_then(|item| {
            item.local_cover_path
                .as_ref()
                .map(|relative| {
                    state
                        .manager
                        .entry_dir(&item.folder, item.enabled)
                        .join(relative)
                })
                .filter(|path| path.exists())
                .map(|path| path.to_string_lossy().to_string())
        })
        .or_else(|| card.cover_url.clone())
        .or_else(|| card.thumbnail_url.clone())
}

fn sanitize_archive_file_name(file_name: &str) -> String {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return "downloaded_mod.zip".to_string();
    }

    trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn default_mod_folder_name(card: &ModCard) -> String {
    format!("{}-{}", card.id, sanitize_path_component(&card.name).to_string())
}

fn sanitize_path_component(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut last_was_dash = false;
    for ch in input.chars() {
        let keep = ch.is_alphanumeric() || ch == '_' || ch == '-';
        if keep {
            output.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            output.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = output.trim_matches('-');
    if trimmed.is_empty() {
        "mod".to_string()
    } else {
        trimmed.to_string()
    }
}
