use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub core: CoreSettings,
    pub ui: UiSettings,
    pub network: NetworkSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreSettings {
    pub gimi_runtime: GimiRuntimeSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GimiRuntimeSettings {
    pub importer_directory: PathBuf,
    pub managed_version: String,
    pub github_repo_owner: String,
    pub github_repo_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub language: String,
    pub night_mode: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    pub concurrent_downloads: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cdn_base_url: Option<String>,
}

pub const UI_LANGUAGE_OPTIONS: &[&str] = &["zh-CN", "en-US", "ja-JP"];

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            core: CoreSettings::default(),
            ui: UiSettings::default(),
            network: NetworkSettings::default(),
        }
    }
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self {
            gimi_runtime: GimiRuntimeSettings::default(),
        }
    }
}

impl Default for GimiRuntimeSettings {
    fn default() -> Self {
        Self {
            importer_directory: app_runtime_dir().join("gimi"),
            managed_version: "v8.7.8".to_string(),
            github_repo_owner: "SilentNightSound".to_string(),
            github_repo_name: "GIMI-Package".to_string(),
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            night_mode: false,
        }
    }
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            concurrent_downloads: 3,
            cdn_base_url: None,
        }
    }
}

impl AppSettings {
    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        let mut loaded = fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str::<Self>(&data).ok())
            .unwrap_or_default();
        loaded.network.concurrent_downloads = loaded.network.concurrent_downloads.max(1);
        let _ = loaded.save();
        loaded
    }

    pub fn save(&self) -> io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(path, data)
    }

    pub fn config_path() -> PathBuf {
        app_runtime_dir().join("config.json")
    }
}

impl NetworkSettings {
    pub fn concurrent_downloads_usize(&self) -> usize {
        self.concurrent_downloads.max(1) as usize
    }
}

impl GimiRuntimeSettings {
    pub fn mods_directory(&self) -> PathBuf {
        self.importer_directory.join("Mods")
    }

    pub fn version_marker_path(&self) -> PathBuf {
        self.importer_directory.join(".anime-mod-manager-version")
    }

    pub fn releases_url(&self) -> String {
        format!(
            "https://github.com/{}/{}/releases",
            self.github_repo_owner, self.github_repo_name
        )
    }

    pub fn tag_archive_url(&self, tag: &str) -> String {
        format!(
            "https://github.com/{}/{}/archive/refs/tags/{}.zip",
            self.github_repo_owner, self.github_repo_name, tag
        )
    }
}

pub fn app_runtime_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}
