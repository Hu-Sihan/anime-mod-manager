use serde::{de::DeserializeOwned, Deserialize, Serialize};

// ─── GameBanana API response types ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModListPage {
    #[serde(rename = "_aMetadata")]
    pub metadata: PageMetadata,

    #[serde(rename = "_aRecords")]
    pub records: Vec<ModRecord>,

    /// True when the API itself returned fewer records than requested
    /// (= genuinely exhausted). Set by the client after fetching,
    /// NOT by serde.
    #[serde(skip, default)]
    pub api_exhausted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMetadata {
    #[serde(rename = "_nRecordCount")]
    pub total: u64,

    #[serde(rename = "_nPerpage")]
    pub per_page: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRecord {
    #[serde(rename = "_idRow")]
    pub id: u64,

    #[serde(rename = "_sModelName")]
    #[serde(default)]
    pub model_name: String,

    #[serde(rename = "_sName")]
    pub name: String,

    #[serde(rename = "_sProfileUrl")]
    pub profile_url: String,

    #[serde(rename = "_tsDateAdded", default)]
    pub date_added: i64,

    #[serde(rename = "_tsDateModified", default)]
    pub date_modified: i64,

    #[serde(rename = "_bHasFiles")]
    pub has_files: bool,

    #[serde(rename = "_aRootCategory")]
    pub root_category: Option<CategoryInfo>,

    #[serde(rename = "_aSubCategory")]
    pub sub_category: Option<CategoryInfo>,

    #[serde(rename = "_aPreviewMedia")]
    pub preview_media: Option<PreviewMedia>,

    #[serde(rename = "_aSubmitter", default)]
    pub submitter: Option<SubmitterInfo>,

    #[serde(rename = "_nLikeCount", default)]
    pub like_count: u32,

    #[serde(rename = "_nViewCount", default)]
    pub view_count: u32,

    #[serde(rename = "_sVersion")]
    #[serde(default)]
    pub version: String,

    #[serde(rename = "_bIsObsolete")]
    #[serde(default)]
    pub is_obsolete: bool,

    #[serde(rename = "_bHasContentRatings")]
    #[serde(default)]
    pub has_content_ratings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryInfo {
    #[serde(rename = "_sName")]
    pub name: String,

    #[serde(rename = "_sProfileUrl")]
    pub profile_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMedia {
    #[serde(rename = "_aImages")]
    #[serde(default)]
    pub images: Vec<PreviewImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewImage {
    #[serde(rename = "_sType")]
    #[serde(default)]
    pub image_type: String,

    #[serde(rename = "_sBaseUrl")]
    pub base_url: String,

    #[serde(rename = "_sFile")]
    pub file: String,

    #[serde(rename = "_sFile220")]
    #[serde(default)]
    pub file_220: String,

    #[serde(rename = "_sFile100")]
    #[serde(default)]
    pub file_100: String,

    #[serde(rename = "_sFile530")]
    #[serde(default)]
    pub file_530: String,

    #[serde(rename = "_sFile800")]
    #[serde(default)]
    pub file_800: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitterInfo {
    #[serde(rename = "_idRow")]
    pub id: u64,

    #[serde(rename = "_sName")]
    pub name: String,

    #[serde(rename = "_sAvatarUrl")]
    #[serde(default)]
    pub avatar_url: String,
}

// ─── Mod detail ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDetail {
    #[serde(rename = "_idRow", default)]
    pub id: u64,

    #[serde(rename = "_sName", default)]
    pub name: String,

    #[serde(rename = "_sText")]
    #[serde(default)]
    pub description: String,

    #[serde(rename = "_aFiles")]
    #[serde(default)]
    pub files: Vec<ModFile>,

    #[serde(rename = "_aRootCategory")]
    #[serde(default)]
    pub root_category: Option<CategoryInfo>,

    #[serde(rename = "_aCategory")]
    #[serde(default)]
    pub category: Option<CategoryInfo>,

    #[serde(rename = "_aPreviewMedia")]
    #[serde(default)]
    pub preview_media: Option<PreviewMedia>,

    #[serde(rename = "_aSubmitter")]
    #[serde(default)]
    pub submitter: Option<SubmitterInfo>,

    #[serde(rename = "_tsDateModified", default)]
    pub date_modified: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFile {
    #[serde(rename = "_idRow")]
    pub id: u64,

    #[serde(rename = "_sFile")]
    pub filename: String,

    #[serde(rename = "_nFilesize")]
    pub size: u64,

    #[serde(rename = "_tsDateAdded")]
    #[serde(default)]
    pub date_added: i64,

    #[serde(rename = "_nDownloadCount")]
    #[serde(default)]
    pub download_count: u32,

    #[serde(rename = "_sDownloadUrl")]
    pub download_url: String,

    #[serde(rename = "_sMd5Checksum")]
    #[serde(default)]
    pub md5: String,

    #[serde(rename = "_sDescription")]
    #[serde(default)]
    pub description: String,
}

// ─── App-internal types ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModCard {
    pub id: u64,
    pub name: String,
    pub author: String,
    pub category: String,
    pub subcategory: Option<String>,
    pub likes: u32,
    pub views: u32,
    pub date_added: i64,
    #[serde(default)]
    pub date_modified: i64,
    #[serde(default)]
    pub local_cover_path: Option<String>,
    pub thumbnail_url: Option<String>,
    pub cover_url: Option<String>,
    pub is_r18: bool,
    pub has_files: bool,
    pub profile_url: String,
}

impl ModCard {
    /// A blank placeholder card for lazy-loading slots
    pub fn placeholder() -> Self {
        Self {
            id: 0,
            name: String::new(),
            author: String::new(),
            category: String::new(),
            subcategory: None,
            likes: 0,
            views: 0,
            date_added: 0,
            date_modified: 0,
            local_cover_path: None,
            thumbnail_url: None,
            cover_url: None,
            is_r18: false,
            has_files: false,
            profile_url: String::new(),
        }
    }
}

impl From<ModRecord> for ModCard {
    fn from(r: ModRecord) -> Self {
        let thumbnail = r.preview_media.as_ref().and_then(|m| {
            m.images.first().map(|img| {
                if !img.file_530.is_empty() {
                    format!("{}/{}", img.base_url, img.file_530)
                } else {
                    format!("{}/{}", img.base_url, img.file)
                }
            })
        });
        let cover = r.preview_media.as_ref().and_then(|m| {
            m.images
                .first()
                .map(|img| format!("{}/{}", img.base_url, img.file))
        });
        Self {
            id: r.id,
            name: r.name,
            author: r
                .submitter
                .map(|s| s.name)
                .unwrap_or_else(|| "Unknown".into()),
            category: r.root_category.map(|c| c.name).unwrap_or_default(),
            subcategory: r.sub_category.map(|c| c.name),
            likes: r.like_count,
            views: r.view_count,
            date_added: r.date_added,
            date_modified: r.date_modified,
            local_cover_path: None,
            thumbnail_url: thumbnail,
            cover_url: cover,
            is_r18: r.has_content_ratings,
            has_files: r.has_files,
            profile_url: r.profile_url,
        }
    }
}

// ─── Local mod types ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    #[serde(default)]
    pub meta_uuid: String,
    pub name: String,
    pub mod_id: u64,
    pub installed_at: i64,
    pub folder: String,
    pub remote_date_modified: i64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub subcategory: Option<String>,
    #[serde(default)]
    pub is_r18: bool,
    #[serde(default)]
    pub local_cover_path: Option<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub profile_url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default = "default_mod_entry_state")]
    pub state: ModEntryState,
    #[serde(default)]
    pub current_file_name: Option<String>,
    #[serde(default)]
    pub active_download: Option<DownloadCheckpoint>,
    #[serde(default)]
    pub local_detail: Option<LocalModDetail>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MetaTemplateKind {
    Mod,
    Download,
}

pub trait MetaTemplate: Serialize + DeserializeOwned + Clone {
    const KIND: MetaTemplateKind;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModMetaTemplate {
    pub name: String,
    pub mod_id: u64,
    pub installed_at: i64,
    pub folder: String,
    pub remote_date_modified: i64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub subcategory: Option<String>,
    #[serde(default)]
    pub is_r18: bool,
    #[serde(default)]
    pub local_cover_path: Option<String>,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub profile_url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default = "default_mod_entry_state")]
    pub state: ModEntryState,
    #[serde(default)]
    pub current_file_name: Option<String>,
    #[serde(default)]
    pub local_detail: Option<LocalModDetail>,
}

impl MetaTemplate for ModMetaTemplate {
    const KIND: MetaTemplateKind = MetaTemplateKind::Mod;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModEntryState {
    Pending,
    Installed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadCheckpointPhase {
    /// 任务已开始或应当继续执行。
    Started,
    /// 任务被暂停，等待用户恢复。
    Paused,
    /// 任务失败，具体失败原因由 `status_code` 细分。
    Failed,
}

impl DownloadCheckpointPhase {
    pub fn from_status_code(status_code: u16) -> Self {
        if (400..500).contains(&status_code) {
            Self::Failed
        } else if (300..400).contains(&status_code) {
            Self::Paused
        } else {
            Self::Started
        }
    }

    pub fn default_status_code(self) -> u16 {
        match self {
            Self::Started => DOWNLOAD_STATUS_STARTED,
            Self::Paused => DOWNLOAD_STATUS_PAUSED,
            Self::Failed => DOWNLOAD_STATUS_FAILED_UNKNOWN,
        }
    }
}

/// 下载任务已创建并处于开始/继续执行状态。
pub const DOWNLOAD_STATUS_STARTED: u16 = 200;
/// 下载任务被用户暂停。
pub const DOWNLOAD_STATUS_PAUSED: u16 = 300;
/// 未细分原因的通用失败。
pub const DOWNLOAD_STATUS_FAILED_UNKNOWN: u16 = 400;
/// 初始化 meta 或本地目录失败。
pub const DOWNLOAD_STATUS_FAILED_PREPARE: u16 = 410;
/// 拉取远端模组详情失败。
pub const DOWNLOAD_STATUS_FAILED_REMOTE_META: u16 = 420;
/// 远端资源没有访问权限。
pub const DOWNLOAD_STATUS_FAILED_ACCESS_DENIED: u16 = 422;
/// 文件下载阶段失败。
pub const DOWNLOAD_STATUS_FAILED_TRANSFER: u16 = 430;
/// 本地目标路径没有写入权限。
pub const DOWNLOAD_STATUS_FAILED_WRITE_PERMISSION: u16 = 431;
/// 读取流提前结束，通常表示文件截断或内容不完整。
pub const DOWNLOAD_STATUS_FAILED_UNEXPECTED_EOF: u16 = 432;
/// 网络异常、超时、连接重置或断线。
pub const DOWNLOAD_STATUS_FAILED_NETWORK: u16 = 433;
/// 网络环境异常，连续重试仍然失败。
pub const DOWNLOAD_STATUS_FAILED_NETWORK_ENVIRONMENT: u16 = 434;
/// 续传 Range 请求无效，例如服务端返回 416。
pub const DOWNLOAD_STATUS_FAILED_INVALID_RANGE: u16 = 435;
/// 安装、解压或落盘阶段失败。
pub const DOWNLOAD_STATUS_FAILED_INSTALL: u16 = 440;
/// 目标文件不存在。
pub const DOWNLOAD_STATUS_FAILED_FILE_NOT_FOUND: u16 = 441;
/// 文件格式或压缩格式不支持。
pub const DOWNLOAD_STATUS_FAILED_UNSUPPORTED_FORMAT: u16 = 442;
/// 没有读取源文件或压缩包的权限。
pub const DOWNLOAD_STATUS_FAILED_READ_PERMISSION: u16 = 443;

pub fn download_status_is_failed(status_code: u16) -> bool {
    (400..500).contains(&status_code)
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DownloadCheckpointPhaseSerde {
    Started,
    Paused,
    Failed,
    Queued,
    Downloading,
    Installing,
}

impl From<DownloadCheckpointPhaseSerde> for DownloadCheckpointPhase {
    fn from(value: DownloadCheckpointPhaseSerde) -> Self {
        match value {
            DownloadCheckpointPhaseSerde::Started
            | DownloadCheckpointPhaseSerde::Queued
            | DownloadCheckpointPhaseSerde::Downloading
            | DownloadCheckpointPhaseSerde::Installing => Self::Started,
            DownloadCheckpointPhaseSerde::Paused => Self::Paused,
            DownloadCheckpointPhaseSerde::Failed => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadStateSerde {
    #[serde(default)]
    file_id: u64,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    temp_file_path: Option<String>,
    #[serde(default)]
    downloaded_bytes: u64,
    #[serde(default)]
    total_bytes: u64,
    #[serde(default)]
    phase: Option<DownloadCheckpointPhaseSerde>,
    #[serde(default)]
    status_code: Option<u16>,
    #[serde(default, alias = "status_text")]
    debug_detail: Option<String>,
    #[serde(default)]
    updated_at: i64,
}

fn normalize_download_state(
    raw_phase: Option<DownloadCheckpointPhaseSerde>,
    raw_status_code: Option<u16>,
    raw_debug_detail: Option<String>,
) -> (DownloadCheckpointPhase, u16, Option<String>) {
    let status_code = raw_status_code.unwrap_or_else(|| {
        raw_phase
            .map(DownloadCheckpointPhase::from)
            .unwrap_or(DownloadCheckpointPhase::Started)
            .default_status_code()
    });
    let phase = DownloadCheckpointPhase::from_status_code(status_code);
    let debug_detail = raw_debug_detail.and_then(|detail| {
        let trimmed = detail.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    (phase, status_code, debug_detail)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "DownloadStateSerde")]
pub struct DownloadCheckpoint {
    #[serde(default)]
    pub file_id: u64,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub temp_file_path: Option<String>,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub total_bytes: u64,
    pub phase: DownloadCheckpointPhase,
    #[serde(default)]
    pub status_code: u16,
    #[serde(default)]
    pub debug_detail: Option<String>,
    #[serde(default)]
    pub updated_at: i64,
}

impl From<DownloadStateSerde> for DownloadCheckpoint {
    fn from(value: DownloadStateSerde) -> Self {
        let (phase, status_code, debug_detail) =
            normalize_download_state(value.phase, value.status_code, value.debug_detail);
        Self {
            file_id: value.file_id,
            file_name: value.file_name,
            temp_file_path: value.temp_file_path,
            downloaded_bytes: value.downloaded_bytes,
            total_bytes: value.total_bytes,
            phase,
            status_code,
            debug_detail,
            updated_at: value.updated_at,
        }
    }
}

/// 下载模板：只保存“下载任务生命周期”相关的状态。
///
/// 这份模板会和 `ModMetaTemplate` 共用同一个 uuid，
/// 但职责不同：
/// - `ModMetaTemplate` 负责“这个模组是谁、装在哪里、有哪些本地信息”
/// - `DownloadMetaTemplate` 负责“这个模组当前下载到哪一步了”
///
/// 典型示例：
/// ```json
/// {
///   "file_id": 475792,
///   "file_name": "vampire_neuvillette_no_sleeves_dark_hair_.rar",
///   "temp_file_path": "/path/to/Mods/123-vampire-neuvillette/vampire_neuvillette_no_sleeves_dark_hair_.rar",
///   "downloaded_bytes": 10485760,
///   "total_bytes": 52428800,
///   "phase": "started",
///   "status_code": 200,
///   "debug_detail": null,
///   "updated_at": 1718366400
/// }
/// ```
///
/// 上面这个例子表示：
/// - 当前任务已经选定了远端文件 `475792`
/// - 临时文件已经创建在模组目录里
/// - 已下载 10 MiB，总大小 50 MiB
/// - 当前阶段是 `Started`
/// - 状态码 `200` 表示任务处于开始/继续执行状态
/// - 启动软件后可以依靠这份信息恢复任务和 UI 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "DownloadStateSerde")]
pub struct DownloadMetaTemplate {
    /// 当前下载任务绑定的远端文件 id。
    #[serde(default)]
    pub file_id: u64,
    /// 原始压缩包文件名，用于 UI 显示和最终安装识别。
    #[serde(default)]
    pub file_name: String,
    /// 半成品下载文件的路径，用于软件重启后的续传。
    #[serde(default)]
    pub temp_file_path: Option<String>,
    /// 当前已经写入临时文件的字节数。
    #[serde(default)]
    pub downloaded_bytes: u64,
    /// 远端报告的总文件大小。
    #[serde(default)]
    pub total_bytes: u64,
    /// 持久化的粗阶段，只区分开始、暂停、失败三类。
    pub phase: DownloadCheckpointPhase,
    /// 任务状态码。
    ///
    /// 约定：
    /// - `200` 表示开始/执行中
    /// - `300` 表示暂停
    /// - `4xx` 表示失败，不同 code 细分失败原因
    #[serde(default)]
    pub status_code: u16,
    /// 调试详情，只在失败或排障时使用，不面向用户显示。
    #[serde(default)]
    pub debug_detail: Option<String>,
    /// 最近一次更新时间戳，主要用于启动时恢复任务顺序。
    #[serde(default)]
    pub updated_at: i64,
}

impl From<DownloadStateSerde> for DownloadMetaTemplate {
    fn from(value: DownloadStateSerde) -> Self {
        let (phase, status_code, debug_detail) =
            normalize_download_state(value.phase, value.status_code, value.debug_detail);
        Self {
            file_id: value.file_id,
            file_name: value.file_name,
            temp_file_path: value.temp_file_path,
            downloaded_bytes: value.downloaded_bytes,
            total_bytes: value.total_bytes,
            phase,
            status_code,
            debug_detail,
            updated_at: value.updated_at,
        }
    }
}

impl MetaTemplate for DownloadMetaTemplate {
    const KIND: MetaTemplateKind = MetaTemplateKind::Download;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalModDetail {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub preview_urls: Vec<String>,
    #[serde(default)]
    pub files: Vec<LocalModFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModFile {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub date_added: i64,
    #[serde(default)]
    pub download_count: u32,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ModStatus {
    NotInstalled,
    Installed(InstalledMod),
    UpdateAvailable {
        installed: InstalledMod,
        latest_date: i64,
    },
}

fn default_enabled() -> bool {
    true
}

fn default_mod_entry_state() -> ModEntryState {
    ModEntryState::Installed
}

impl InstalledMod {
    pub fn from_templates(
        meta_uuid: String,
        mod_meta: ModMetaTemplate,
        download_meta: Option<DownloadMetaTemplate>,
    ) -> Self {
        Self {
            meta_uuid,
            name: mod_meta.name,
            mod_id: mod_meta.mod_id,
            installed_at: mod_meta.installed_at,
            folder: mod_meta.folder,
            remote_date_modified: mod_meta.remote_date_modified,
            author: mod_meta.author,
            category: mod_meta.category,
            subcategory: mod_meta.subcategory,
            is_r18: mod_meta.is_r18,
            local_cover_path: mod_meta.local_cover_path,
            thumbnail_url: mod_meta.thumbnail_url,
            cover_url: mod_meta.cover_url,
            profile_url: mod_meta.profile_url,
            enabled: mod_meta.enabled,
            update_available: mod_meta.update_available,
            state: mod_meta.state,
            current_file_name: mod_meta.current_file_name,
            active_download: download_meta.map(Into::into),
            local_detail: mod_meta.local_detail,
        }
    }

    pub fn split_templates(&self) -> (ModMetaTemplate, Option<DownloadMetaTemplate>) {
        (
            ModMetaTemplate {
                name: self.name.clone(),
                mod_id: self.mod_id,
                installed_at: self.installed_at,
                folder: self.folder.clone(),
                remote_date_modified: self.remote_date_modified,
                author: self.author.clone(),
                category: self.category.clone(),
                subcategory: self.subcategory.clone(),
                is_r18: self.is_r18,
                local_cover_path: self.local_cover_path.clone(),
                thumbnail_url: self.thumbnail_url.clone(),
                cover_url: self.cover_url.clone(),
                profile_url: self.profile_url.clone(),
                enabled: self.enabled,
                update_available: self.update_available,
                state: self.state,
                current_file_name: self.current_file_name.clone(),
                local_detail: self.local_detail.clone(),
            },
            self.active_download.clone().map(Into::into),
        )
    }

    pub fn to_mod_card(&self) -> ModCard {
        ModCard {
            id: self.mod_id,
            name: self.name.clone(),
            author: if self.author.is_empty() {
                "Unknown".to_string()
            } else {
                self.author.clone()
            },
            category: self.category.clone(),
            subcategory: self.subcategory.clone(),
            likes: 0,
            views: 0,
            date_added: self.installed_at,
            date_modified: self.remote_date_modified,
            local_cover_path: self.local_cover_path.clone(),
            thumbnail_url: self.thumbnail_url.clone(),
            cover_url: self.cover_url.clone(),
            is_r18: self.is_r18,
            has_files: true,
            profile_url: self.profile_url.clone(),
        }
    }

    pub fn is_installed(&self) -> bool {
        self.state == ModEntryState::Installed
    }

    pub fn has_active_download(&self) -> bool {
        self.active_download.as_ref().is_some_and(|download| {
            matches!(
                download.phase,
                DownloadCheckpointPhase::Started | DownloadCheckpointPhase::Paused
            )
        })
    }
}

impl From<DownloadCheckpoint> for DownloadMetaTemplate {
    fn from(value: DownloadCheckpoint) -> Self {
        Self {
            file_id: value.file_id,
            file_name: value.file_name,
            temp_file_path: value.temp_file_path,
            downloaded_bytes: value.downloaded_bytes,
            total_bytes: value.total_bytes,
            phase: value.phase,
            status_code: value.status_code,
            debug_detail: value.debug_detail,
            updated_at: value.updated_at,
        }
    }
}

impl From<DownloadMetaTemplate> for DownloadCheckpoint {
    fn from(value: DownloadMetaTemplate) -> Self {
        Self {
            file_id: value.file_id,
            file_name: value.file_name,
            temp_file_path: value.temp_file_path,
            downloaded_bytes: value.downloaded_bytes,
            total_bytes: value.total_bytes,
            phase: value.phase,
            status_code: value.status_code,
            debug_detail: value.debug_detail,
            updated_at: value.updated_at,
        }
    }
}

impl LocalModDetail {
    pub fn from_remote_detail(detail: &ModDetail) -> Self {
        let preview_urls = detail
            .preview_media
            .as_ref()
            .map(|media| {
                media
                    .images
                    .iter()
                    .filter_map(|image| {
                        let file = if !image.file_530.is_empty() {
                            Some(image.file_530.as_str())
                        } else if !image.file.is_empty() {
                            Some(image.file.as_str())
                        } else if !image.file_220.is_empty() {
                            Some(image.file_220.as_str())
                        } else if !image.file_100.is_empty() {
                            Some(image.file_100.as_str())
                        } else if !image.file_800.is_empty() {
                            Some(image.file_800.as_str())
                        } else {
                            None
                        }?;
                        Some(format!("{}/{}", image.base_url, file))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let files = detail
            .files
            .iter()
            .map(|file| LocalModFile {
                id: file.id,
                filename: file.filename.clone(),
                size: file.size,
                date_added: file.date_added,
                download_count: file.download_count,
                description: file.description.clone(),
            })
            .collect();

        Self {
            description: detail.description.clone(),
            preview_urls,
            files,
        }
    }
}
