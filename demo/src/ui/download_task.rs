use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    mpsc, Arc, Mutex,
};

use anime_mod_manager::{
    DownloadCheckpointPhase, GameBananaClient, LocalModDetail, ModCard, ModDetail, ModFile,
    ModFileDownloader, ModManager, DOWNLOAD_STATUS_FAILED_ACCESS_DENIED,
    DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND, DOWNLOAD_STATUS_FAILED_INSTALL,
    DOWNLOAD_STATUS_FAILED_INVALID_RANGE, DOWNLOAD_STATUS_FAILED_NETWORK,
    DOWNLOAD_STATUS_FAILED_PREPARE, DOWNLOAD_STATUS_FAILED_READ_PERMISSION,
    DOWNLOAD_STATUS_FAILED_REMOTE_META, DOWNLOAD_STATUS_FAILED_TRANSFER,
    DOWNLOAD_STATUS_FAILED_UNEXPECTED_EOF, DOWNLOAD_STATUS_FAILED_UNSUPPORTED_FORMAT,
    DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION, DOWNLOAD_STATUS_STARTED,
};

use super::download_scheduler::DownloadTaskPhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskControlRequest {
    Pause = 1,
    Remove = 2,
    Shutdown = 3,
}

#[derive(Clone)]
struct TaskControlHandle {
    request: Arc<AtomicU8>,
}

impl TaskControlHandle {
    fn new() -> Self {
        Self {
            request: Arc::new(AtomicU8::new(0)),
        }
    }

    fn request_pause(&self) {
        self.request
            .store(TaskControlRequest::Pause as u8, Ordering::SeqCst);
    }

    fn request_remove(&self) {
        self.request
            .store(TaskControlRequest::Remove as u8, Ordering::SeqCst);
    }

    fn request_shutdown(&self) {
        self.request
            .store(TaskControlRequest::Shutdown as u8, Ordering::SeqCst);
    }

    fn requested_action(&self) -> Option<TaskControlRequest> {
        match self.request.load(Ordering::SeqCst) {
            1 => Some(TaskControlRequest::Pause),
            2 => Some(TaskControlRequest::Remove),
            3 => Some(TaskControlRequest::Shutdown),
            _ => None,
        }
    }

    fn should_abort(&self) -> bool {
        self.requested_action().is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadTaskStatusCode {
    Queued,
    Paused,
    Preparing,
    Downloading,
    RetryPending,
    ReusingArchive,
    Installing,
    Completed,
    Failed,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DownloadTaskUpdate {
    pub phase: DownloadTaskPhase,
    pub progress: u8,
    pub status_code: DownloadTaskStatusCode,
    pub status_text: String,
    pub image_path: Option<String>,
}

impl DownloadTaskUpdate {
    pub fn new(
        phase: DownloadTaskPhase,
        progress: u8,
        status_code: DownloadTaskStatusCode,
        status_text: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            progress: progress.min(100),
            status_code,
            status_text: status_text.into(),
            image_path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DownloadTaskEvent {
    Update(DownloadTaskUpdate),
    Finished(DownloadTaskUpdate),
    Failed(DownloadTaskUpdate),
}

#[derive(Debug, Clone)]
pub struct FreshDownloadTask {
    pub task_id: u64,
    pub card: ModCard,
    pub detail: Option<ModDetail>,
    pub file: ModFile,
    pub folder_name: String,
}

#[derive(Debug, Clone)]
pub struct ResumeDownloadTask {
    pub task_id: u64,
    pub mod_id: u64,
    pub file_id: u64,
    pub file_name: String,
    pub folder_name: String,
    pub is_r18: bool,
}

#[derive(Debug, Clone)]
pub enum DownloadTaskRequest {
    Fresh(FreshDownloadTask),
    Resume(ResumeDownloadTask),
}

impl DownloadTaskRequest {
    pub fn task_id(&self) -> u64 {
        match self {
            Self::Fresh(task) => task.task_id,
            Self::Resume(task) => task.task_id,
        }
    }

    pub fn mod_id(&self) -> u64 {
        match self {
            Self::Fresh(task) => task.card.id,
            Self::Resume(task) => task.mod_id,
        }
    }

    pub fn folder_name(&self) -> &str {
        match self {
            Self::Fresh(task) => &task.folder_name,
            Self::Resume(task) => &task.folder_name,
        }
    }

    pub fn queued_update(&self) -> DownloadTaskUpdate {
        DownloadTaskUpdate::new(
            DownloadTaskPhase::Queued,
            0,
            DownloadTaskStatusCode::Queued,
            "等待下载",
        )
    }

    pub fn paused_update(&self, progress: u8) -> DownloadTaskUpdate {
        DownloadTaskUpdate::new(
            DownloadTaskPhase::Paused,
            progress,
            DownloadTaskStatusCode::Paused,
            "已暂停",
        )
    }
}

#[derive(Clone)]
pub struct DownloadTaskExecutionHandle {
    status: Arc<Mutex<DownloadTaskUpdate>>,
    control: TaskControlHandle,
}

impl DownloadTaskExecutionHandle {
    pub fn current_status(&self) -> DownloadTaskUpdate {
        self.status.lock().unwrap().clone()
    }

    pub fn has_pending_control_request(&self) -> bool {
        self.control.requested_action().is_some()
    }

    pub fn pause(&self) -> DownloadTaskUpdate {
        self.control.request_pause();
        DownloadTaskUpdate::new(
            DownloadTaskPhase::Paused,
            self.current_status().progress,
            DownloadTaskStatusCode::Paused,
            "正在暂停",
        )
    }

    pub fn remove(&self) -> DownloadTaskUpdate {
        self.control.request_remove();
        DownloadTaskUpdate::new(
            DownloadTaskPhase::Paused,
            self.current_status().progress,
            DownloadTaskStatusCode::Removed,
            "正在删除",
        )
    }

    pub fn shutdown(&self) -> DownloadTaskUpdate {
        self.control.request_shutdown();
        DownloadTaskUpdate::new(
            DownloadTaskPhase::Queued,
            self.current_status().progress,
            DownloadTaskStatusCode::Queued,
            "等待续传",
        )
    }
}

pub struct DownloadTaskExecution {
    pub handle: DownloadTaskExecutionHandle,
    pub receiver: mpsc::Receiver<DownloadTaskEvent>,
}

pub struct DownloadLifecycleTask {
    client: GameBananaClient,
    file_downloader: Arc<ModFileDownloader>,
    manager: ModManager,
    request: DownloadTaskRequest,
    status: Arc<Mutex<DownloadTaskUpdate>>,
    control: TaskControlHandle,
}

#[derive(Debug, Clone)]
pub struct PreparedInstallArtifact {
    pub archive_path: PathBuf,
    pub folder_name: String,
    pub mod_card: ModCard,
}

impl DownloadLifecycleTask {
    pub fn new(
        client: GameBananaClient,
        file_downloader: Arc<ModFileDownloader>,
        manager: ModManager,
        request: DownloadTaskRequest,
    ) -> Self {
        Self {
            client,
            file_downloader,
            manager,
            request,
            status: Arc::new(Mutex::new(DownloadTaskUpdate::new(
                DownloadTaskPhase::Queued,
                0,
                DownloadTaskStatusCode::Queued,
                "等待下载",
            ))),
            control: TaskControlHandle::new(),
        }
    }

    pub fn execution_handle(&self) -> DownloadTaskExecutionHandle {
        DownloadTaskExecutionHandle {
            status: self.status.clone(),
            control: self.control.clone(),
        }
    }

    pub fn prepare_metadata(&self) -> Result<(), String> {
        match &self.request {
            DownloadTaskRequest::Fresh(task) => {
                self.manager
                    .prepare_download(&task.card, &task.file, &task.folder_name)
                    .map_err(|err| format!("初始化元数据失败：{err}"))?;
                if let Some(detail) = task.detail.as_ref() {
                    self.manager
                        .update_local_detail(
                            &task.folder_name,
                            LocalModDetail::from_remote_detail(detail),
                        )
                        .map_err(|err| format!("写入模组详情失败：{err}"))?;
                }
                Ok(())
            }
            DownloadTaskRequest::Resume(_) => Ok(()),
        }
    }

    pub fn download_or_resume(
        &self,
        sender: &mpsc::Sender<DownloadTaskEvent>,
    ) -> Result<PreparedInstallArtifact, String> {
        if self.should_abort() {
            return Err(self.controlled_stop_message());
        }
        match &self.request {
            DownloadTaskRequest::Fresh(task) => self.transfer_fresh(task, sender),
            DownloadTaskRequest::Resume(task) => self.transfer_resume(task, sender),
        }
    }

    pub fn install_artifact(
        &self,
        sender: &mpsc::Sender<DownloadTaskEvent>,
        artifact: &PreparedInstallArtifact,
    ) -> Result<(), String> {
        self.emit_update(
            sender,
            DownloadTaskUpdate::new(
                DownloadTaskPhase::Installing,
                100,
                DownloadTaskStatusCode::Installing,
                "正在安装",
            ),
        );
        self.manager
            .update_download_progress(
                &artifact.folder_name,
                DownloadCheckpointPhase::Started,
                DOWNLOAD_STATUS_STARTED,
                0,
                0,
                Some(&artifact.archive_path),
                None,
            )
            .ok();

        if self.should_abort() {
            return Err(self.controlled_stop_message());
        }

        match self.manager.install(
            &artifact.archive_path,
            &artifact.mod_card,
            &artifact.folder_name,
        ) {
            Ok(()) => {
                let _ = std::fs::remove_file(&artifact.archive_path);
                self.emit_finished(
                    sender,
                    DownloadTaskUpdate::new(
                        DownloadTaskPhase::Completed,
                        100,
                        DownloadTaskStatusCode::Completed,
                        "已安装",
                    ),
                );
                Ok(())
            }
            Err(err) => {
                let removed_bad_archive = std::fs::remove_file(&artifact.archive_path).is_ok();
                let message = if removed_bad_archive {
                    format!("安装失败：{err}；已移除损坏压缩包，请重试下载")
                } else {
                    format!("安装失败：{err}")
                };
                self.manager
                    .mark_download_failed(
                        &artifact.folder_name,
                        classify_install_failure(&message),
                        &message,
                    )
                    .ok();
                self.emit_failed(
                    sender,
                    DownloadTaskUpdate::new(
                        DownloadTaskPhase::Failed,
                        self.execution_handle().current_status().progress,
                        DownloadTaskStatusCode::Failed,
                        message.clone(),
                    ),
                );
                Err(message)
            }
        }
    }

    pub fn delete_task_files(&self) -> Result<(), String> {
        self.manager
            .clear_active_download(self.request.folder_name())
            .map_err(|err| format!("删除任务失败：{err}"))
    }

    fn control_request(&self) -> Option<TaskControlRequest> {
        self.control.requested_action()
    }

    fn should_abort(&self) -> bool {
        self.control.should_abort()
    }

    fn controlled_stop_phase(&self) -> DownloadTaskPhase {
        match self.control_request() {
            Some(TaskControlRequest::Shutdown) => DownloadTaskPhase::Queued,
            _ => DownloadTaskPhase::Paused,
        }
    }

    fn controlled_stop_status_code(&self) -> DownloadTaskStatusCode {
        match self.control_request() {
            Some(TaskControlRequest::Remove) => DownloadTaskStatusCode::Removed,
            Some(TaskControlRequest::Shutdown) => DownloadTaskStatusCode::Queued,
            _ => DownloadTaskStatusCode::Paused,
        }
    }

    fn controlled_stop_message(&self) -> String {
        match self.control_request() {
            Some(TaskControlRequest::Remove) => "已移除".to_string(),
            Some(TaskControlRequest::Pause) => "已暂停".to_string(),
            Some(TaskControlRequest::Shutdown) => "等待续传".to_string(),
            None => "已停止".to_string(),
        }
    }

    pub fn spawn(self) -> DownloadTaskExecution {
        let (sender, receiver) = mpsc::channel::<DownloadTaskEvent>();
        let handle = self.execution_handle();
        std::thread::spawn({
            let sender = sender.clone();
            move || {
                if let Err(err) = self.prepare_metadata() {
                    if self.should_abort() {
                        self.emit_finished(
                            &sender,
                            DownloadTaskUpdate::new(
                                self.controlled_stop_phase(),
                                self.execution_handle().current_status().progress,
                                self.controlled_stop_status_code(),
                                self.controlled_stop_message(),
                            ),
                        );
                        return;
                    }
                    self.manager
                        .mark_download_failed(
                            self.request.folder_name(),
                            classify_prepare_failure(&err),
                            &err,
                        )
                        .ok();
                    self.emit_failed(
                        &sender,
                        DownloadTaskUpdate::new(
                            DownloadTaskPhase::Failed,
                            0,
                            DownloadTaskStatusCode::Failed,
                            err,
                        ),
                    );
                    return;
                }

                match self.download_or_resume(&sender) {
                    Ok(artifact) => {
                        if self.should_abort() {
                            self.emit_finished(
                                &sender,
                                DownloadTaskUpdate::new(
                                    self.controlled_stop_phase(),
                                    self.execution_handle().current_status().progress,
                                    self.controlled_stop_status_code(),
                                    self.controlled_stop_message(),
                                ),
                            );
                            return;
                        }
                        let _ = self.install_artifact(&sender, &artifact);
                    }
                    Err(err) => {
                        if self.should_abort() {
                            self.emit_finished(
                                &sender,
                                DownloadTaskUpdate::new(
                                    self.controlled_stop_phase(),
                                    self.execution_handle().current_status().progress,
                                    self.controlled_stop_status_code(),
                                    self.controlled_stop_message(),
                                ),
                            );
                            return;
                        }
                        self.emit_failed(
                            &sender,
                            DownloadTaskUpdate::new(
                                DownloadTaskPhase::Failed,
                                self.execution_handle().current_status().progress,
                                DownloadTaskStatusCode::Failed,
                                err,
                            ),
                        );
                    }
                }
            }
        });

        DownloadTaskExecution { handle, receiver }
    }

    fn transfer_fresh(
        &self,
        task: &FreshDownloadTask,
        sender: &mpsc::Sender<DownloadTaskEvent>,
    ) -> Result<PreparedInstallArtifact, String> {
        let target_dir = self.resolve_fresh_target_dir(task);
        if let Err(err) = std::fs::create_dir_all(&target_dir) {
            let message = format!("无法创建模组目录：{err}");
            self.manager
                .mark_download_failed(
                    &task.folder_name,
                    DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION,
                    &message,
                )
                .ok();
            return Err(message);
        }
        let archive_path = target_dir.join(sanitize_archive_file_name(&task.file.filename));

        if self.should_abort() {
            return Err(self.controlled_stop_message());
        }
        self.emit_cover_image_if_available(task, sender, &target_dir);
        self.manager
            .update_download_progress(
                &task.folder_name,
                DownloadCheckpointPhase::Started,
                DOWNLOAD_STATUS_STARTED,
                0,
                task.file.size,
                Some(&archive_path),
                None,
            )
            .ok();
        self.emit_update(
            sender,
            DownloadTaskUpdate::new(
                DownloadTaskPhase::Queued,
                0,
                DownloadTaskStatusCode::Queued,
                "等待下载",
            ),
        );

        if self.should_abort() {
            return Err(self.controlled_stop_message());
        }

        let archive_exists = std::fs::metadata(&archive_path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false);

        if archive_exists {
            self.emit_update(
                sender,
                DownloadTaskUpdate::new(
                    DownloadTaskPhase::Downloading,
                    0,
                    DownloadTaskStatusCode::ReusingArchive,
                    "复用已下载文件",
                ),
            );
        } else {
            self.emit_update(
                sender,
                DownloadTaskUpdate::new(
                    DownloadTaskPhase::Downloading,
                    0,
                    DownloadTaskStatusCode::Preparing,
                    "正在准备",
                ),
            );
        }
        self.emit_update(
            sender,
            DownloadTaskUpdate::new(
                DownloadTaskPhase::Downloading,
                0,
                DownloadTaskStatusCode::Downloading,
                "正在下载",
            ),
        );

        let manager = self.manager.clone();
        let folder_name = task.folder_name.clone();
        let archive_for_progress = archive_path.clone();
        let progress_sender = sender.clone();
        let status_cell = self.status.clone();
        let control = self.control.clone();
        let progress_control = self.control.clone();
        let should_abort = move || control.should_abort();
        match self.file_downloader.download_file(
            &task.file,
            &archive_path,
            &move |done, total| {
                if progress_control.should_abort() {
                    return;
                }
                manager
                    .update_download_progress(
                        &folder_name,
                        DownloadCheckpointPhase::Started,
                        DOWNLOAD_STATUS_STARTED,
                        done,
                        total,
                        Some(&archive_for_progress),
                        None,
                    )
                    .ok();
                if progress_control.should_abort() {
                    return;
                }
                let percent = if total > 0 {
                    ((done as f64 / total as f64) * 100.0).round() as u8
                } else {
                    100
                };
                send_event(
                    &progress_sender,
                    &status_cell,
                    DownloadTaskEvent::Update(DownloadTaskUpdate::new(
                        DownloadTaskPhase::Downloading,
                        percent.min(100),
                        DownloadTaskStatusCode::Downloading,
                        "正在下载",
                    )),
                );
            },
            &should_abort,
        ) {
            Ok(_) => {}
            Err(_err) if self.should_abort() => {
                return Err(self.controlled_stop_message());
            }
            Err(err) => {
                let message = format!("下载失败：{err}");
                self.manager
                    .mark_download_failed(
                        &task.folder_name,
                        classify_transfer_failure(&message),
                        &message,
                    )
                    .ok();
                return Err(message);
            }
        }

        Ok(PreparedInstallArtifact {
            archive_path,
            folder_name: task.folder_name.clone(),
            mod_card: task.card.clone(),
        })
    }

    fn transfer_resume(
        &self,
        task: &ResumeDownloadTask,
        sender: &mpsc::Sender<DownloadTaskEvent>,
    ) -> Result<PreparedInstallArtifact, String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_SECS: u64 = 30;
        let mut last_remote_error = None::<String>;

        for attempt in 0..MAX_RETRIES {
            if self.should_abort() {
                return Err(self.controlled_stop_message());
            }
            if attempt > 0 {
                self.emit_update(
                    sender,
                    DownloadTaskUpdate::new(
                        DownloadTaskPhase::Queued,
                        0,
                        DownloadTaskStatusCode::RetryPending,
                        format!("获取模组信息失败，{}/{} 次重试", attempt, MAX_RETRIES - 1),
                    ),
                );
                let mut waited = 0u64;
                let total_wait = RETRY_DELAY_SECS * 1000;
                while waited < total_wait {
                    if self.should_abort() {
                        return Err(self.controlled_stop_message());
                    }
                    let step = 250u64.min(total_wait - waited);
                    std::thread::sleep(std::time::Duration::from_millis(step));
                    waited += step;
                }
            }

            match self.client.get_mod(task.mod_id) {
                Ok(detail) => {
                    if self.should_abort() {
                        return Err(self.controlled_stop_message());
                    }
                    let Some(file) = detail
                        .files
                        .iter()
                        .find(|file| file.id == task.file_id)
                        .cloned()
                    else {
                        let message = "文件已不存在".to_string();
                        self.manager
                            .mark_download_failed(
                                &task.folder_name,
                                DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND,
                                &message,
                            )
                            .ok();
                        return Err(message);
                    };

                    let cover_url = detail
                        .preview_media
                        .as_ref()
                        .and_then(|media| media.images.first())
                        .map(|image| format!("{}/{}", image.base_url, image.file_530));
                    let target_dir = self.manager.entry_dir(&task.folder_name, true);
                    if let Err(err) = std::fs::create_dir_all(&target_dir) {
                        let message = format!("无法创建模组目录：{err}");
                        self.manager
                            .mark_download_failed(
                                &task.folder_name,
                                DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION,
                                &message,
                            )
                            .ok();
                        return Err(message);
                    }
                    let archive_path = target_dir.join(sanitize_archive_file_name(&file.filename));
                    let resume_bytes = std::fs::metadata(&archive_path)
                        .map(|metadata| metadata.len())
                        .unwrap_or(0);
                    let resume_progress = if file.size > 0 {
                        ((resume_bytes.min(file.size) as f64 / file.size as f64) * 100.0)
                            .round()
                            .clamp(0.0, 100.0) as u8
                    } else {
                        0
                    };

                    if self.should_abort() {
                        return Err(self.controlled_stop_message());
                    }
                    if let Some(url) = &cover_url {
                        if let Ok(bytes) = anime_mod_manager::download_image(url) {
                            if let Ok(relative_path) =
                                self.manager
                                    .store_local_cover(&task.folder_name, url, &bytes)
                            {
                                let absolute_path = target_dir.join(relative_path);
                                self.emit_image_ready(
                                    sender,
                                    absolute_path.to_string_lossy().to_string(),
                                );
                            }
                        }
                    }

                    let archive_exists = resume_bytes > 0;

                    if archive_exists {
                        self.emit_update(
                            sender,
                            DownloadTaskUpdate::new(
                                DownloadTaskPhase::Downloading,
                                resume_progress,
                                DownloadTaskStatusCode::ReusingArchive,
                                "复用已下载文件",
                            ),
                        );
                    } else {
                        self.emit_update(
                            sender,
                            DownloadTaskUpdate::new(
                                DownloadTaskPhase::Downloading,
                                0,
                                DownloadTaskStatusCode::Preparing,
                                "正在准备",
                            ),
                        );
                    }
                    self.emit_update(
                        sender,
                        DownloadTaskUpdate::new(
                            DownloadTaskPhase::Downloading,
                            resume_progress,
                            DownloadTaskStatusCode::Downloading,
                            "正在下载",
                        ),
                    );
                    let manager = self.manager.clone();
                    let folder_name = task.folder_name.clone();
                    let archive_for_progress = archive_path.clone();
                    let progress_sender = sender.clone();
                    let status_cell = self.status.clone();
                    let control = self.control.clone();
                    let progress_control = self.control.clone();
                    let should_abort = move || control.should_abort();
                    match self.file_downloader.download_file(
                        &file,
                        &archive_path,
                        &move |done, total| {
                            if progress_control.should_abort() {
                                return;
                            }
                            manager
                                .update_download_progress(
                                    &folder_name,
                                    DownloadCheckpointPhase::Started,
                                    DOWNLOAD_STATUS_STARTED,
                                    done,
                                    total,
                                    Some(&archive_for_progress),
                                    None,
                                )
                                .ok();
                            if progress_control.should_abort() {
                                return;
                            }
                            let percent = if total > 0 {
                                ((done as f64 / total as f64) * 100.0).round() as u8
                            } else {
                                100
                            };
                            send_event(
                                &progress_sender,
                                &status_cell,
                                DownloadTaskEvent::Update(DownloadTaskUpdate::new(
                                    DownloadTaskPhase::Downloading,
                                    percent.min(100),
                                    DownloadTaskStatusCode::Downloading,
                                    "正在下载",
                                )),
                            );
                        },
                        &should_abort,
                    ) {
                        Ok(_) => {}
                        Err(_err) if self.should_abort() => {
                            return Err(self.controlled_stop_message());
                        }
                        Err(err) => {
                            let message = format!("下载失败：{err}");
                            self.manager
                                .mark_download_failed(
                                    &task.folder_name,
                                    classify_transfer_failure(&message),
                                    &message,
                                )
                                .ok();
                            return Err(message);
                        }
                    }

                    let mod_card = ModCard {
                        id: detail.id,
                        name: if detail.name.is_empty() {
                            task.file_name.clone()
                        } else {
                            detail.name.clone()
                        },
                        author: detail
                            .submitter
                            .as_ref()
                            .map(|submitter| submitter.name.clone())
                            .unwrap_or_default(),
                        category: detail
                            .root_category
                            .as_ref()
                            .map(|category| category.name.clone())
                            .unwrap_or_default(),
                        subcategory: detail
                            .category
                            .as_ref()
                            .map(|category| category.name.clone()),
                        likes: 0,
                        views: 0,
                        date_added: 0,
                        date_modified: detail.date_modified,
                        thumbnail_url: cover_url.clone(),
                        cover_url: detail
                            .preview_media
                            .as_ref()
                            .and_then(|media| media.images.first())
                            .map(|image| format!("{}/{}", image.base_url, image.file)),
                        is_r18: task.is_r18,
                        has_files: !detail.files.is_empty(),
                        profile_url: format!("https://gamebanana.com/mods/{}", detail.id),
                        local_cover_path: None,
                    };

                    return Ok(PreparedInstallArtifact {
                        archive_path,
                        folder_name: task.folder_name.clone(),
                        mod_card,
                    });
                }
                Err(err) => {
                    last_remote_error = Some(err.to_string());
                    continue;
                }
            }
        }

        let message = last_remote_error
            .map(|detail| format!("获取模组信息失败，已重试3次：{detail}"))
            .unwrap_or_else(|| "获取模组信息失败，已重试3次".to_string());
        self.manager
            .mark_download_failed(
                &task.folder_name,
                classify_remote_meta_failure(&message),
                &message,
            )
            .ok();
        Err(message)
    }

    fn resolve_fresh_target_dir(&self, task: &FreshDownloadTask) -> PathBuf {
        self.manager
            .get_record(task.card.id)
            .ok()
            .flatten()
            .map(|record| self.manager.entry_dir(&record.folder, record.enabled))
            .unwrap_or_else(|| self.manager.mod_folder(&task.folder_name))
    }

    fn emit_cover_image_if_available(
        &self,
        task: &FreshDownloadTask,
        sender: &mpsc::Sender<DownloadTaskEvent>,
        target_dir: &std::path::Path,
    ) {
        let preview_url = task
            .detail
            .as_ref()
            .and_then(|detail| {
                LocalModDetail::from_remote_detail(detail)
                    .preview_urls
                    .first()
                    .cloned()
            })
            .or_else(|| task.card.cover_url.clone())
            .or_else(|| task.card.thumbnail_url.clone());

        let Some(url) = preview_url else {
            return;
        };

        if let Ok(bytes) = anime_mod_manager::download_image(&url) {
            if let Ok(relative_path) =
                self.manager
                    .store_local_cover(&task.folder_name, &url, &bytes)
            {
                let absolute_path = target_dir.join(relative_path);
                self.emit_image_ready(sender, absolute_path.to_string_lossy().to_string());
            }
        }
    }

    fn emit_update(&self, sender: &mpsc::Sender<DownloadTaskEvent>, update: DownloadTaskUpdate) {
        send_event(sender, &self.status, DownloadTaskEvent::Update(update));
    }

    fn emit_finished(&self, sender: &mpsc::Sender<DownloadTaskEvent>, update: DownloadTaskUpdate) {
        send_event(sender, &self.status, DownloadTaskEvent::Finished(update));
    }

    fn emit_failed(&self, sender: &mpsc::Sender<DownloadTaskEvent>, update: DownloadTaskUpdate) {
        send_event(sender, &self.status, DownloadTaskEvent::Failed(update));
    }

    fn emit_image_ready(&self, sender: &mpsc::Sender<DownloadTaskEvent>, image_path: String) {
        let mut update = self.execution_handle().current_status();
        update.image_path = Some(image_path.clone());
        send_event(sender, &self.status, DownloadTaskEvent::Update(update));
    }
}

fn send_event(
    sender: &mpsc::Sender<DownloadTaskEvent>,
    status: &Arc<Mutex<DownloadTaskUpdate>>,
    event: DownloadTaskEvent,
) {
    match &event {
        DownloadTaskEvent::Update(update)
        | DownloadTaskEvent::Finished(update)
        | DownloadTaskEvent::Failed(update) => {
            *status.lock().unwrap() = update.clone();
        }
    }
    let _ = sender.send(event);
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

fn classify_prepare_failure(message: &str) -> u16 {
    if is_permission_denied(message) {
        DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION
    } else {
        DOWNLOAD_STATUS_FAILED_PREPARE
    }
}

fn classify_remote_meta_failure(message: &str) -> u16 {
    if is_access_denied(message) {
        DOWNLOAD_STATUS_FAILED_ACCESS_DENIED
    } else if is_network_error(message) {
        DOWNLOAD_STATUS_FAILED_NETWORK
    } else {
        DOWNLOAD_STATUS_FAILED_REMOTE_META
    }
}

fn classify_transfer_failure(message: &str) -> u16 {
    if is_access_denied(message) {
        DOWNLOAD_STATUS_FAILED_ACCESS_DENIED
    } else if is_permission_denied(message) {
        DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION
    } else if is_invalid_range(message) {
        DOWNLOAD_STATUS_FAILED_INVALID_RANGE
    } else if is_unexpected_eof(message) {
        DOWNLOAD_STATUS_FAILED_UNEXPECTED_EOF
    } else if is_missing_file(message) {
        DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND
    } else if is_network_error(message) {
        DOWNLOAD_STATUS_FAILED_NETWORK
    } else {
        DOWNLOAD_STATUS_FAILED_TRANSFER
    }
}

fn classify_install_failure(message: &str) -> u16 {
    if is_unsupported_format(message) {
        DOWNLOAD_STATUS_FAILED_UNSUPPORTED_FORMAT
    } else if is_unexpected_eof(message) {
        DOWNLOAD_STATUS_FAILED_UNEXPECTED_EOF
    } else if is_missing_file(message) {
        DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND
    } else if is_permission_denied(message) {
        if indicates_read_operation(message) {
            DOWNLOAD_STATUS_FAILED_READ_PERMISSION
        } else {
            DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION
        }
    } else {
        DOWNLOAD_STATUS_FAILED_INSTALL
    }
}

fn is_access_denied(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("http 401")
        || lower.contains("http 403")
        || lower.contains("unexpected status: 401")
        || lower.contains("unexpected status: 403")
        || lower.contains("forbidden")
        || lower.contains("unauthorized")
        || lower.contains("access denied")
        || lower.contains("没有访问权限")
}

fn is_permission_denied(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("permission denied")
        || lower.contains("operation not permitted")
        || lower.contains("没有权限")
}

fn is_missing_file(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("文件不存在")
        || lower.contains("文件已不存在")
        || lower.contains("找不到")
}

fn is_unsupported_format(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("格式不支持")
        || lower.contains("unsupported method")
        || lower.contains("unsupported format")
        || lower.contains("暂不支持")
}

fn is_unexpected_eof(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("unexpected end of file")
        || lower.contains("unexpected eof")
        || lower.contains("stream ended early")
        || lower.contains("end of file")
        || lower.contains("文件流提前结束")
        || lower.contains("文件已截断")
}

fn is_network_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("network error")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("connection aborted")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("temporarily unavailable")
        || lower.contains("network is unreachable")
        || lower.contains("host unreachable")
        || lower.contains("dns")
        || lower.contains("io error")
        || lower.contains("网络异常")
        || lower.contains("网络错误")
        || lower.contains("连接失败")
}

fn is_invalid_range(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("unexpected status: 416")
        || lower.contains("range not satisfiable")
        || lower.contains("invalid range")
        || lower.contains("server ignored range resume request")
}

fn indicates_read_operation(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("读取压缩包失败")
        || lower.contains("读取 zip")
        || lower.contains("read")
        || lower.contains("open")
}
