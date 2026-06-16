use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use anime_mod_manager::{
    DownloadCheckpointPhase, ModCard, ModDetail, ModFile, DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND,
    DOWNLOAD_STATUS_FAILED_INVALID_RANGE, DOWNLOAD_STATUS_FAILED_NETWORK,
    DOWNLOAD_STATUS_FAILED_NETWORK_ENVIRONMENT, DOWNLOAD_STATUS_FAILED_UNEXPECTED_EOF,
    DOWNLOAD_STATUS_PAUSED, DOWNLOAD_STATUS_STARTED,
};

use super::download_task::{
    DownloadLifecycleTask, DownloadTaskEvent, DownloadTaskExecutionHandle, DownloadTaskRequest,
    DownloadTaskStatusCode, DownloadTaskUpdate, FreshDownloadTask, ResumeDownloadTask,
};
use super::AppState;

type QueueListener = Rc<dyn Fn()>;

#[derive(Debug, Default, Clone, Copy)]
struct RetryCounters {
    unexpected_eof: u8,
    file_not_found: u8,
    network: u8,
    invalid_range: u8,
    generation: u64,
}

#[derive(Debug, Clone, Copy)]
enum RetryKind {
    UnexpectedEof,
    FileNotFound,
    Network,
    InvalidRange,
}

#[derive(Debug, Clone, Copy)]
enum UserTaskAction {
    Start(u64),
    Pause(u64),
    #[allow(dead_code)]
    Remove(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadTaskPhase {
    Queued,
    Paused,
    Downloading,
    Installing,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct DownloadTask {
    pub id: u64,
    pub mod_id: u64,
    pub file_id: u64,
    pub mod_name: String,
    pub file_name: String,
    pub image_url: Option<String>,
    pub progress: u8,
    pub phase: DownloadTaskPhase,
    pub status_code: DownloadTaskStatusCode,
    pub status_text: String,
}

pub struct DownloadQueue {
    next_id: Cell<u64>,
    tasks: RefCell<Vec<DownloadTask>>,
    listeners: RefCell<Vec<QueueListener>>,
}

#[derive(Clone)]
pub struct DownloadScheduler {
    inner: Rc<DownloadSchedulerInner>,
}

struct DownloadSchedulerInner {
    max_concurrent: Cell<usize>,
    shutdown_in_progress: Cell<bool>,
    running_handles: RefCell<HashMap<u64, DownloadTaskExecutionHandle>>,
    retry_reserved: RefCell<HashSet<u64>>,
    pending_order: RefCell<VecDeque<u64>>,
    pending_jobs: RefCell<HashMap<u64, DownloadTaskRequest>>,
    paused_jobs: RefCell<HashMap<u64, DownloadTaskRequest>>,
    user_actions: RefCell<VecDeque<UserTaskAction>>,
    retry_counts: RefCell<HashMap<u64, RetryCounters>>,
}

impl DownloadQueue {
    pub fn new() -> Self {
        Self {
            next_id: Cell::new(1),
            tasks: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
        }
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Rc::new(listener));
    }

    pub fn snapshot(&self) -> Vec<DownloadTask> {
        self.tasks.borrow().clone()
    }

    pub fn create_task(
        &self,
        mod_id: u64,
        mod_name: String,
        file_id: u64,
        file_name: String,
        image_url: Option<String>,
    ) -> u64 {
        {
            let mut tasks = self.tasks.borrow_mut();
            if let Some(task) = tasks
                .iter_mut()
                .find(|task| task.mod_id == mod_id && task.file_id == file_id)
            {
                task.mod_name = mod_name;
                task.file_name = file_name;
                task.image_url = image_url;
                task.progress = 0;
                task.phase = DownloadTaskPhase::Queued;
                task.status_code = DownloadTaskStatusCode::Queued;
                task.status_text = "等待下载".to_string();
                let id = task.id;
                drop(tasks);
                self.notify();
                return id;
            }
        }

        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        self.tasks.borrow_mut().push(DownloadTask {
            id,
            mod_id,
            file_id,
            mod_name,
            file_name,
            image_url,
            progress: 0,
            phase: DownloadTaskPhase::Queued,
            status_code: DownloadTaskStatusCode::Queued,
            status_text: "等待下载".to_string(),
        });
        self.notify();
        id
    }

    pub fn apply_update(
        &self,
        id: u64,
        phase: DownloadTaskPhase,
        progress: u8,
        status_code: DownloadTaskStatusCode,
        status_text: impl Into<String>,
        image_url: Option<String>,
    ) {
        {
            let mut tasks = self.tasks.borrow_mut();
            if let Some(task) = tasks.iter_mut().find(|task| task.id == id) {
                task.phase = phase;
                task.progress = progress.min(100);
                task.status_code = status_code;
                task.status_text = status_text.into();
                if let Some(image_url) = image_url {
                    task.image_url = Some(image_url);
                }
            }
        }
        self.notify();
    }

    pub fn remove_task(&self, id: u64) {
        let mut tasks = self.tasks.borrow_mut();
        let before = tasks.len();
        tasks.retain(|task| task.id != id);
        if tasks.len() != before {
            drop(tasks);
            self.notify();
        }
    }

    fn notify(&self) {
        let listeners = self.listeners.borrow().clone();
        for listener in listeners {
            listener();
        }
    }
}

impl DownloadScheduler {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            inner: Rc::new(DownloadSchedulerInner {
                max_concurrent: Cell::new(max_concurrent.max(1)),
                shutdown_in_progress: Cell::new(false),
                running_handles: RefCell::new(HashMap::new()),
                retry_reserved: RefCell::new(HashSet::new()),
                pending_order: RefCell::new(VecDeque::new()),
                pending_jobs: RefCell::new(HashMap::new()),
                paused_jobs: RefCell::new(HashMap::new()),
                user_actions: RefCell::new(VecDeque::new()),
                retry_counts: RefCell::new(HashMap::new()),
            }),
        }
    }

    pub fn set_max_concurrent(&self, state: &Rc<AppState>, max_concurrent: usize) {
        self.inner.max_concurrent.set(max_concurrent.max(1));
        self.drive(state);
    }

    pub fn insert_fresh(
        &self,
        state: &Rc<AppState>,
        task_id: u64,
        card: ModCard,
        detail: Option<ModDetail>,
        file: ModFile,
        folder_name: String,
    ) {
        let request = DownloadTaskRequest::Fresh(FreshDownloadTask {
            task_id,
            card,
            detail,
            file,
            folder_name,
        });
        self.insert_task(state, request);
    }

    pub fn insert_resume(
        &self,
        state: &Rc<AppState>,
        task_id: u64,
        mod_id: u64,
        file_id: u64,
        file_name: String,
        folder_name: String,
        is_r18: bool,
    ) {
        self.insert_task(
            state,
            DownloadTaskRequest::Resume(ResumeDownloadTask {
                task_id,
                mod_id,
                file_id,
                file_name,
                folder_name,
                is_r18,
            }),
        );
    }

    pub fn store_paused(&self, state: &Rc<AppState>, task_id: u64, request: DownloadTaskRequest) {
        let progress = state
            .downloads
            .snapshot()
            .into_iter()
            .find(|task| task.id == task_id)
            .map(|task| task.progress)
            .unwrap_or(0);
        self.inner.pending_jobs.borrow_mut().remove(&task_id);
        self.inner
            .pending_order
            .borrow_mut()
            .retain(|pending_id| *pending_id != task_id);
        self.persist_paused_request(state, &request);
        self.inner.paused_jobs.borrow_mut().insert(task_id, request);
        state.downloads.apply_task_update(
            task_id,
            DownloadTaskPhase::Paused,
            progress,
            DownloadTaskStatusCode::Paused,
            "已暂停",
            None,
        );
    }

    pub fn start_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        self.enqueue_user_action(UserTaskAction::Start(task_id));
        self.drive(state);
        true
    }

    pub fn pause_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        self.enqueue_user_action(UserTaskAction::Pause(task_id));
        self.drive(state);
        true
    }

    #[allow(dead_code)]
    pub fn remove_task(&self, state: &Rc<AppState>, task_id: u64) -> bool {
        self.enqueue_user_action(UserTaskAction::Remove(task_id));
        self.drive(state);
        true
    }

    #[allow(dead_code)]
    pub fn pause_all(&self, state: &Rc<AppState>) -> usize {
        let mut task_ids = state
            .downloads
            .snapshot()
            .into_iter()
            .filter(|task| {
                matches!(
                    task.phase,
                    DownloadTaskPhase::Queued
                        | DownloadTaskPhase::Downloading
                        | DownloadTaskPhase::Installing
                )
            })
            .map(|task| task.id)
            .collect::<Vec<_>>();
        task_ids.sort_unstable();
        task_ids.dedup();

        for task_id in task_ids.iter().copied() {
            self.enqueue_user_action(UserTaskAction::Pause(task_id));
        }
        self.drive(state);
        task_ids.len()
    }

    pub fn has_running_tasks(&self) -> bool {
        !self.inner.running_handles.borrow().is_empty()
            || !self.inner.retry_reserved.borrow().is_empty()
    }

    pub fn begin_shutdown(&self, state: &Rc<AppState>) -> usize {
        self.inner.shutdown_in_progress.set(true);

        let running = self
            .inner
            .running_handles
            .borrow()
            .iter()
            .map(|(task_id, handle)| (*task_id, handle.clone()))
            .collect::<Vec<_>>();

        for (task_id, handle) in running.iter().cloned() {
            let update = handle.shutdown();
            state.downloads.apply_task_update(
                task_id,
                update.phase,
                update.progress,
                update.status_code,
                update.status_text,
                update.image_path,
            );
        }

        running.len()
    }

    pub fn reset_retry_state_for(&self, task_id: u64) {
        self.clear_retry_state(task_id);
    }

    fn enqueue_user_action(&self, action: UserTaskAction) {
        self.inner.user_actions.borrow_mut().push_back(action);
    }

    fn persist_paused_request(&self, state: &Rc<AppState>, request: &DownloadTaskRequest) {
        let Ok(Some(record)) = state.manager.get_record(request.mod_id()) else {
            return;
        };
        let Some(download) = record.active_download.as_ref() else {
            return;
        };

        let temp_file_path = download.temp_file_path.as_deref().map(Path::new);
        let _ = state.manager.update_download_progress(
            request.folder_name(),
            DownloadCheckpointPhase::Paused,
            DOWNLOAD_STATUS_PAUSED,
            download.downloaded_bytes,
            download.total_bytes,
            temp_file_path,
            download.debug_detail.as_deref(),
        );
    }

    fn process_user_actions(&self, state: &Rc<AppState>) -> bool {
        let mut processed = false;

        loop {
            let Some(action) = self.inner.user_actions.borrow_mut().pop_front() else {
                break;
            };
            processed = true;
            match action {
                UserTaskAction::Start(task_id) => {
                    self.process_start_action(state, task_id);
                }
                UserTaskAction::Pause(task_id) => {
                    self.process_pause_action(state, task_id);
                }
                UserTaskAction::Remove(task_id) => {
                    self.process_remove_action(state, task_id);
                }
            }
        }

        processed
    }

    fn process_start_action(&self, state: &Rc<AppState>, task_id: u64) {
        if self.inner.running_handles.borrow().contains_key(&task_id) {
            return;
        }

        let Some(job) = self.inner.paused_jobs.borrow_mut().remove(&task_id) else {
            return;
        };
        let update = job.queued_update();
        self.inner.pending_jobs.borrow_mut().insert(task_id, job);
        if !self
            .inner
            .pending_order
            .borrow()
            .iter()
            .any(|pending_id| *pending_id == task_id)
        {
            self.inner.pending_order.borrow_mut().push_back(task_id);
        }
        state.downloads.apply_task_update(
            task_id,
            update.phase,
            update.progress,
            update.status_code,
            update.status_text,
            update.image_path,
        );
    }

    fn process_pause_action(&self, state: &Rc<AppState>, task_id: u64) {
        if let Some(handle) = self.inner.running_handles.borrow().get(&task_id).cloned() {
            let update = handle.pause();
            state.downloads.apply_task_update(
                task_id,
                update.phase,
                update.progress,
                update.status_code,
                update.status_text,
                update.image_path,
            );
            return;
        }

        let Some(job) = self.inner.pending_jobs.borrow_mut().remove(&task_id) else {
            return;
        };
        let update = job.paused_update(0);
        self.inner
            .pending_order
            .borrow_mut()
            .retain(|pending_id| *pending_id != task_id);
        self.persist_paused_request(state, &job);
        self.inner.paused_jobs.borrow_mut().insert(task_id, job);
        state.downloads.apply_task_update(
            task_id,
            update.phase,
            update.progress,
            update.status_code,
            update.status_text,
            update.image_path,
        );
    }

    fn process_remove_action(&self, state: &Rc<AppState>, task_id: u64) {
        if let Some(handle) = self.inner.running_handles.borrow().get(&task_id).cloned() {
            let update = handle.remove();
            self.clear_retry_state(task_id);
            state.downloads.apply_task_update(
                task_id,
                update.phase,
                update.progress,
                update.status_code,
                update.status_text,
                update.image_path,
            );
            return;
        }

        let request = self
            .inner
            .pending_jobs
            .borrow_mut()
            .remove(&task_id)
            .or_else(|| self.inner.paused_jobs.borrow_mut().remove(&task_id));
        if let Some(request) = request {
            self.inner
                .pending_order
                .borrow_mut()
                .retain(|pending_id| *pending_id != task_id);
            self.clear_retry_state(task_id);

            let lifecycle = DownloadLifecycleTask::new(
                state.client.clone(),
                state.mod_file_downloader.clone(),
                state.manager.clone(),
                request,
            );
            let _ = lifecycle.delete_task_files();
            state.downloads.remove_task_entry(task_id);
            return;
        }

        let Some(task) = state
            .downloads
            .snapshot()
            .into_iter()
            .find(|task| task.id == task_id)
        else {
            return;
        };

        if !matches!(
            task.phase,
            DownloadTaskPhase::Failed | DownloadTaskPhase::Completed
        ) {
            return;
        }

        if let Ok(Some(record)) = state.manager.get_record(task.mod_id) {
            let _ = state.manager.clear_active_download(&record.folder);
        }
        self.clear_retry_state(task_id);
        state.downloads.remove_task_entry(task_id);
    }

    #[allow(dead_code)]
    pub fn task_status(&self, task_id: u64) -> Option<super::download_task::DownloadTaskUpdate> {
        self.inner
            .running_handles
            .borrow()
            .get(&task_id)
            .map(DownloadTaskExecutionHandle::current_status)
    }

    fn insert_task(&self, state: &Rc<AppState>, request: DownloadTaskRequest) {
        self.insert_task_with_options(state, request, 0, false);
    }

    fn insert_task_with_options(
        &self,
        state: &Rc<AppState>,
        request: DownloadTaskRequest,
        queued_progress: u8,
        prioritize: bool,
    ) {
        let task_id = request.task_id();
        let lifecycle = DownloadLifecycleTask::new(
            state.client.clone(),
            state.mod_file_downloader.clone(),
            state.manager.clone(),
            request.clone(),
        );
        if let Err(err) = lifecycle.prepare_metadata() {
            state.downloads.apply_task_update(
                task_id,
                DownloadTaskPhase::Failed,
                0,
                DownloadTaskStatusCode::Failed,
                err,
                None,
            );
            return;
        }

        self.inner.paused_jobs.borrow_mut().remove(&task_id);
        self.inner
            .pending_jobs
            .borrow_mut()
            .insert(task_id, request);
        self.inner
            .pending_order
            .borrow_mut()
            .retain(|pending_id| *pending_id != task_id);
        if prioritize {
            self.inner.pending_order.borrow_mut().push_front(task_id);
        } else {
            self.inner.pending_order.borrow_mut().push_back(task_id);
        }
        state.downloads.apply_task_update(
            task_id,
            DownloadTaskPhase::Queued,
            queued_progress,
            DownloadTaskStatusCode::Queued,
            "等待下载",
            None,
        );
        self.drive(state);
    }

    fn drive(&self, state: &Rc<AppState>) {
        loop {
            let mut progressed = false;

            if !self.inner.shutdown_in_progress.get() {
                loop {
                    let running_count = self.active_slot_count();
                    if running_count >= self.inner.max_concurrent.get() {
                        break;
                    }

                    let Some(task_id) = self.inner.pending_order.borrow_mut().pop_front() else {
                        break;
                    };
                    let Some(request) = self.inner.pending_jobs.borrow_mut().remove(&task_id)
                    else {
                        progressed = true;
                        continue;
                    };

                    self.spawn_task(state, task_id, request);
                    progressed = true;
                }
            }

            if self.process_user_actions(state) {
                progressed = true;
            }

            if !progressed {
                break;
            }
        }
    }

    fn spawn_task(&self, state: &Rc<AppState>, task_id: u64, request: DownloadTaskRequest) {
        let request_for_retry = request.clone();
        let mod_id = request.mod_id();
        let folder_name = request.folder_name().to_string();
        let lifecycle = DownloadLifecycleTask::new(
            state.client.clone(),
            state.mod_file_downloader.clone(),
            state.manager.clone(),
            request,
        );
        let execution = lifecycle.spawn();
        let handle = execution.handle.clone();
        let receiver = Rc::new(RefCell::new(execution.receiver));

        self.inner
            .running_handles
            .borrow_mut()
            .insert(task_id, handle.clone());

        let scheduler = self.clone();
        let state = state.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(120), move || {
            let mut latest_update = None;

            loop {
                match receiver.borrow_mut().try_recv() {
                    Ok(DownloadTaskEvent::Update(update)) => {
                        if handle.has_pending_control_request() {
                            continue;
                        }
                        latest_update = Some(update);
                    }
                    Ok(DownloadTaskEvent::Finished(update)) => {
                        if let Some(pending_update) = latest_update.take() {
                            state.downloads.apply_task_update(
                                task_id,
                                pending_update.phase,
                                pending_update.progress,
                                pending_update.status_code,
                                pending_update.status_text,
                                pending_update.image_path,
                            );
                        }
                        state.downloads.apply_task_update(
                            task_id,
                            update.phase,
                            update.progress,
                            update.status_code,
                            update.status_text,
                            update.image_path,
                        );
                        if update.status_code == DownloadTaskStatusCode::Paused {
                            scheduler.clear_retry_state(task_id);
                            scheduler.store_paused(&state, task_id, request_for_retry.clone());
                            scheduler.finish_task(&state, task_id);
                            return gtk::glib::ControlFlow::Break;
                        }
                        if scheduler.inner.shutdown_in_progress.get()
                            && update.phase == DownloadTaskPhase::Queued
                            && update.status_code == DownloadTaskStatusCode::Queued
                        {
                            scheduler.clear_retry_state(task_id);
                            scheduler.finish_task(&state, task_id);
                            return gtk::glib::ControlFlow::Break;
                        }
                        if update.status_code == DownloadTaskStatusCode::Removed {
                            scheduler.clear_retry_state(task_id);
                            let lifecycle = DownloadLifecycleTask::new(
                                state.client.clone(),
                                state.mod_file_downloader.clone(),
                                state.manager.clone(),
                                request_for_retry.clone(),
                            );
                            let _ = lifecycle.delete_task_files();
                            state.downloads.remove_task_entry(task_id);
                            scheduler.finish_task(&state, task_id);
                            return gtk::glib::ControlFlow::Break;
                        }
                        scheduler.clear_retry_state(task_id);
                        state.notify_installed_changed();
                        scheduler.finish_task(&state, task_id);
                        let downloads = state.downloads.clone();
                        gtk::glib::timeout_add_local_once(Duration::from_millis(1800), move || {
                            downloads.remove_task_entry(task_id);
                        });
                        return gtk::glib::ControlFlow::Break;
                    }
                    Ok(DownloadTaskEvent::Failed(update)) => {
                        if let Some(pending_update) = latest_update.take() {
                            state.downloads.apply_task_update(
                                task_id,
                                pending_update.phase,
                                pending_update.progress,
                                pending_update.status_code,
                                pending_update.status_text,
                                pending_update.image_path,
                            );
                        }
                        let update_phase = update.phase;
                        let update_progress = update.progress;
                        let update_status_code = update.status_code;
                        let update_status_text = update.status_text.clone();
                        let update_image_path = update.image_path.clone();
                        state.downloads.apply_task_update(
                            task_id,
                            update_phase,
                            update_progress,
                            update_status_code,
                            update_status_text,
                            update_image_path,
                        );
                        let retried = scheduler.handle_failure(
                            &state,
                            task_id,
                            &request_for_retry,
                            mod_id,
                            &folder_name,
                            &update,
                        );
                        if !retried {
                            scheduler.clear_retry_state(task_id);
                        }
                        scheduler.finish_task(&state, task_id);
                        return gtk::glib::ControlFlow::Break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        if let Some(pending_update) = latest_update.take() {
                            state.downloads.apply_task_update(
                                task_id,
                                pending_update.phase,
                                pending_update.progress,
                                pending_update.status_code,
                                pending_update.status_text,
                                pending_update.image_path,
                            );
                        }
                        return gtk::glib::ControlFlow::Continue;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if let Some(pending_update) = latest_update.take() {
                            state.downloads.apply_task_update(
                                task_id,
                                pending_update.phase,
                                pending_update.progress,
                                pending_update.status_code,
                                pending_update.status_text,
                                pending_update.image_path,
                            );
                        }
                        let status = handle.current_status();
                        state.downloads.apply_task_update(
                            task_id,
                            DownloadTaskPhase::Failed,
                            status.progress,
                            DownloadTaskStatusCode::Failed,
                            if status.status_text.is_empty() {
                                "下载线程已断开".to_string()
                            } else {
                                status.status_text
                            },
                            status.image_path,
                        );
                        scheduler.clear_retry_state(task_id);
                        scheduler.finish_task(&state, task_id);
                        return gtk::glib::ControlFlow::Break;
                    }
                }
            }
        });
    }

    fn finish_task(&self, state: &Rc<AppState>, task_id: u64) {
        self.inner.running_handles.borrow_mut().remove(&task_id);
        self.drive(state);
    }

    fn clear_retry_state(&self, task_id: u64) {
        self.inner.retry_counts.borrow_mut().remove(&task_id);
        self.inner.retry_reserved.borrow_mut().remove(&task_id);
    }

    fn active_slot_count(&self) -> usize {
        self.inner.running_handles.borrow().len() + self.inner.retry_reserved.borrow().len()
    }

    fn bump_retry(&self, task_id: u64, kind: RetryKind) -> u8 {
        let mut retry_counts = self.inner.retry_counts.borrow_mut();
        let counters = retry_counts.entry(task_id).or_default();
        let count = match kind {
            RetryKind::UnexpectedEof => {
                counters.unexpected_eof = counters.unexpected_eof.saturating_add(1);
                counters.unexpected_eof
            }
            RetryKind::FileNotFound => {
                counters.file_not_found = counters.file_not_found.saturating_add(1);
                counters.file_not_found
            }
            RetryKind::Network => {
                counters.network = counters.network.saturating_add(1);
                counters.network
            }
            RetryKind::InvalidRange => {
                counters.invalid_range = counters.invalid_range.saturating_add(1);
                counters.invalid_range
            }
        };
        count
    }

    fn schedule_retry(
        &self,
        state: &Rc<AppState>,
        task_id: u64,
        request: DownloadTaskRequest,
        progress: u8,
        message: String,
    ) {
        let generation = {
            let mut retry_counts = self.inner.retry_counts.borrow_mut();
            let counters = retry_counts.entry(task_id).or_default();
            counters.generation = counters.generation.saturating_add(1);
            counters.generation
        };

        state.downloads.apply_task_update(
            task_id,
            DownloadTaskPhase::Queued,
            progress,
            DownloadTaskStatusCode::RetryPending,
            message,
            None,
        );

        let scheduler = self.clone();
        let state = state.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(600), move || {
            let current_generation = scheduler
                .inner
                .retry_counts
                .borrow()
                .get(&task_id)
                .map(|c| c.generation);
            if current_generation != Some(generation) {
                return;
            }
            scheduler.insert_task(&state, request);
        });
    }

    fn schedule_retry_with_slot_reservation(
        &self,
        state: &Rc<AppState>,
        task_id: u64,
        request: DownloadTaskRequest,
        progress: u8,
        message: String,
    ) {
        let generation = {
            let mut retry_counts = self.inner.retry_counts.borrow_mut();
            let counters = retry_counts.entry(task_id).or_default();
            counters.generation = counters.generation.saturating_add(1);
            counters.generation
        };

        self.inner.retry_reserved.borrow_mut().insert(task_id);
        state.downloads.apply_task_update(
            task_id,
            DownloadTaskPhase::Queued,
            progress,
            DownloadTaskStatusCode::RetryPending,
            message,
            None,
        );

        let scheduler = self.clone();
        let state = state.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(600), move || {
            let current_generation = scheduler
                .inner
                .retry_counts
                .borrow()
                .get(&task_id)
                .map(|c| c.generation);
            if current_generation != Some(generation) {
                return;
            }
            scheduler.inner.retry_reserved.borrow_mut().remove(&task_id);
            scheduler.insert_task_with_options(&state, request, progress, true);
        });
    }

    fn restore_download_checkpoint(
        &self,
        state: &Rc<AppState>,
        mod_id: u64,
        folder_name: &str,
        reset_downloaded_bytes: bool,
        delete_temp_file: bool,
    ) {
        let Ok(Some(record)) = state.manager.get_record(mod_id) else {
            return;
        };
        let Some(download) = record.active_download.as_ref() else {
            return;
        };

        let temp_file_path = download.temp_file_path.as_deref().map(Path::new);
        if delete_temp_file {
            if let Some(path) = temp_file_path {
                let _ = std::fs::remove_file(path);
            }
        }

        let downloaded_bytes = if reset_downloaded_bytes {
            0
        } else {
            download.downloaded_bytes
        };
        let _ = state.manager.update_download_progress(
            folder_name,
            DownloadCheckpointPhase::Started,
            DOWNLOAD_STATUS_STARTED,
            downloaded_bytes,
            download.total_bytes,
            temp_file_path,
            None,
        );
    }

    fn handle_failure(
        &self,
        state: &Rc<AppState>,
        task_id: u64,
        request: &DownloadTaskRequest,
        mod_id: u64,
        folder_name: &str,
        update: &DownloadTaskUpdate,
    ) -> bool {
        let Ok(Some(record)) = state.manager.get_record(mod_id) else {
            return false;
        };
        let Some(download) = record.active_download.as_ref() else {
            return false;
        };

        match download.status_code {
            DOWNLOAD_STATUS_FAILED_UNEXPECTED_EOF => {
                if self.bump_retry(task_id, RetryKind::UnexpectedEof) <= 1 {
                    self.restore_download_checkpoint(state, mod_id, folder_name, true, true);
                    self.schedule_retry(
                        state,
                        task_id,
                        request.clone(),
                        0,
                        "检测到文件不完整，正在重新下载".to_string(),
                    );
                    true
                } else {
                    false
                }
            }
            DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND => {
                if self.bump_retry(task_id, RetryKind::FileNotFound) <= 1 {
                    self.restore_download_checkpoint(state, mod_id, folder_name, true, true);
                    self.schedule_retry(
                        state,
                        task_id,
                        request.clone(),
                        0,
                        "文件不存在，正在重新下载".to_string(),
                    );
                    true
                } else {
                    false
                }
            }
            DOWNLOAD_STATUS_FAILED_NETWORK => {
                let attempt = self.bump_retry(task_id, RetryKind::Network);
                if attempt <= 3 {
                    self.restore_download_checkpoint(state, mod_id, folder_name, false, false);
                    self.schedule_retry_with_slot_reservation(
                        state,
                        task_id,
                        request.clone(),
                        update.progress,
                        format!("网络异常，重新连接 {attempt}/3"),
                    );
                    true
                } else {
                    let final_message = format!("网络环境异常，已重试 3 次");
                    let debug_detail = if update.status_text.is_empty() {
                        final_message.clone()
                    } else {
                        format!("{final_message}：{}", update.status_text)
                    };
                    let _ = state.manager.mark_download_failed(
                        folder_name,
                        DOWNLOAD_STATUS_FAILED_NETWORK_ENVIRONMENT,
                        &debug_detail,
                    );
                    state.downloads.apply_task_update(
                        task_id,
                        DownloadTaskPhase::Failed,
                        update.progress,
                        DownloadTaskStatusCode::Failed,
                        final_message,
                        update.image_path.clone(),
                    );
                    false
                }
            }
            DOWNLOAD_STATUS_FAILED_INVALID_RANGE => {
                let attempt = self.bump_retry(task_id, RetryKind::InvalidRange);
                if attempt <= 1 {
                    self.restore_download_checkpoint(state, mod_id, folder_name, true, true);
                    self.schedule_retry(
                        state,
                        task_id,
                        request.clone(),
                        0,
                        "检测到续传范围异常，正在重新下载".to_string(),
                    );
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
