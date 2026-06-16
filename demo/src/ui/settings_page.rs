use std::cmp::Ordering;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use anyhow::{anyhow, Context, Result};
use gtk;

use crate::config::{AppSettings, GimiRuntimeSettings, UI_LANGUAGE_OPTIONS};

use super::AppState;

#[derive(Clone, Debug, Default)]
struct RuntimeCardState {
    is_installed: bool,
    installed_version: Option<String>,
    latest_checked_version: Option<String>,
    last_check_error: Option<String>,
    checking: bool,
    operation: Option<RuntimeOperation>,
}

#[derive(Clone, Debug)]
struct RuntimeOperation {
    kind: RuntimeOperationKind,
    progress: u8,
    detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeOperationKind {
    Download,
    Update,
}

#[derive(Clone)]
struct RuntimeWidgets {
    name_label: gtk::Label,
    current_version_label: gtk::Label,
    arrow_label: gtk::Label,
    latest_version_label: gtk::Label,
    primary_button: gtk::Button,
    check_button: gtk::Button,
    progress_row: gtk::Box,
    progress_bar: gtk::ProgressBar,
    progress_label: gtk::Label,
}

enum RuntimeMessage {
    CheckFinished(Result<String, String>),
    InstallProgress { progress: u8, detail: String },
    InstallFinished { version: String },
    InstallFailed(String),
}

pub struct SettingsPage {
    pub container: gtk::Box,
    state: Rc<AppState>,
    language_dropdown: gtk::DropDown,
    night_mode_switch: gtk::Switch,
    concurrent_downloads_spin: gtk::SpinButton,
    runtime_state: Rc<std::cell::RefCell<RuntimeCardState>>,
    runtime_widgets: RuntimeWidgets,
}

impl SettingsPage {
    pub fn new(state: Rc<AppState>) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        container.append(
            &gtk::Label::builder()
                .label("设置")
                .css_classes(["title-3"])
                .halign(gtk::Align::Start)
                .margin_start(12)
                .margin_top(12)
                .margin_bottom(4)
                .build(),
        );

        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(4)
            .margin_bottom(4)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(8)
            .build();
        scrolled.set_child(Some(&content_box));
        container.append(&scrolled);

        let runtime_state = Rc::new(std::cell::RefCell::new(RuntimeCardState::default()));
        let (runtime_card, runtime_widgets) = build_runtime_card();
        let core_section = section_shell("Core");
        core_section.append(&runtime_card);
        content_box.append(&core_section);

        let ui_section = section_shell("UI");
        let ui_card = card_box();

        let language_dropdown = gtk::DropDown::from_strings(UI_LANGUAGE_OPTIONS);
        ui_card.append(&build_widget_row(
            "language",
            "界面语言标识",
            &language_dropdown,
        ));
        ui_card.append(&separator());

        let night_mode_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        ui_card.append(&build_widget_row(
            "night_mode",
            "夜间模式开关",
            &night_mode_switch,
        ));
        ui_section.append(&ui_card);
        content_box.append(&ui_section);

        let network_section = section_shell("Network");
        let network_card = card_box();
        let concurrent_downloads_spin = gtk::SpinButton::with_range(1.0, 16.0, 1.0);
        concurrent_downloads_spin.set_digits(0);
        concurrent_downloads_spin.set_numeric(true);
        concurrent_downloads_spin.set_width_chars(3);
        concurrent_downloads_spin.set_valign(gtk::Align::Center);
        network_card.append(&build_widget_row(
            "concurrent_downloads",
            "同时允许进行的下载任务数量",
            &concurrent_downloads_spin,
        ));
        network_section.append(&network_card);
        content_box.append(&network_section);

        let page = Self {
            container,
            state,
            language_dropdown,
            night_mode_switch,
            concurrent_downloads_spin,
            runtime_state,
            runtime_widgets,
        };

        page.sync_from_state();

        page.language_dropdown.connect_selected_notify({
            let state = page.state.clone();
            move |dropdown| {
                {
                    state.settings.borrow_mut().ui.language = selected_language(dropdown);
                }
                state.persist_settings();
            }
        });

        page.night_mode_switch.connect_active_notify({
            let state = page.state.clone();
            move |switch| {
                {
                    state.settings.borrow_mut().ui.night_mode = switch.is_active();
                }
                state.persist_settings();
            }
        });

        page.concurrent_downloads_spin.connect_value_changed({
            let state = page.state.clone();
            move |spin| {
                let value = spin.value_as_int().max(1) as u32;
                {
                    state.settings.borrow_mut().network.concurrent_downloads = value;
                }
                state.downloads.set_max_concurrent(&state, value as usize);
                state.persist_settings();
            }
        });

        page.runtime_widgets.check_button.connect_clicked({
            let state = page.state.clone();
            let runtime_state = page.runtime_state.clone();
            let runtime_widgets = page.runtime_widgets.clone();
            move |_| {
                let runtime = state.settings.borrow().core.gimi_runtime.clone();
                {
                    let mut card = runtime_state.borrow_mut();
                    card.checking = true;
                    card.last_check_error = None;
                }
                render_runtime_widgets(&runtime_widgets, &runtime, &runtime_state.borrow());

                let (tx, rx) = std::sync::mpsc::channel::<RuntimeMessage>();
                let runtime_for_thread = runtime.clone();
                std::thread::spawn(move || {
                    let result = fetch_latest_release_tag(&runtime_for_thread)
                        .map_err(|err| err.to_string());
                    let _ = tx.send(RuntimeMessage::CheckFinished(result));
                });

                let runtime_state_poll = runtime_state.clone();
                let runtime_widgets_poll = runtime_widgets.clone();
                let runtime_poll = runtime.clone();
                gtk::glib::timeout_add_local(Duration::from_millis(150), move || {
                    match rx.try_recv() {
                        Ok(RuntimeMessage::CheckFinished(result)) => {
                            let mut card = runtime_state_poll.borrow_mut();
                            card.checking = false;
                            match result {
                                Ok(tag) => {
                                    card.latest_checked_version = Some(tag.clone());
                                    card.last_check_error = None;
                                }
                                Err(err) => {
                                    card.last_check_error = Some(err.clone());
                                }
                            }
                            render_runtime_widgets(&runtime_widgets_poll, &runtime_poll, &card);
                            gtk::glib::ControlFlow::Break
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            gtk::glib::ControlFlow::Continue
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            let mut card = runtime_state_poll.borrow_mut();
                            card.checking = false;
                            card.last_check_error = Some("后台检查线程已断开".to_string());
                            render_runtime_widgets(&runtime_widgets_poll, &runtime_poll, &card);
                            gtk::glib::ControlFlow::Break
                        }
                        _ => gtk::glib::ControlFlow::Break,
                    }
                });
            }
        });

        page.runtime_widgets.primary_button.connect_clicked({
            let state = page.state.clone();
            let runtime_state = page.runtime_state.clone();
            let runtime_widgets = page.runtime_widgets.clone();
            move |_| {
                let runtime = state.settings.borrow().core.gimi_runtime.clone();
                let (kind, target_tag) = {
                    let card = runtime_state.borrow();
                    if !card.is_installed {
                        (
                            RuntimeOperationKind::Download,
                            preferred_target_tag(&runtime, &card),
                        )
                    } else if let Some(target) = available_update_tag(&runtime, &card) {
                        (RuntimeOperationKind::Update, target)
                    } else {
                        return;
                    }
                };

                {
                    let mut card = runtime_state.borrow_mut();
                    card.operation = Some(RuntimeOperation {
                        kind,
                        progress: 2,
                        detail: "准备任务...".to_string(),
                    });
                    card.last_check_error = None;
                }
                render_runtime_widgets(&runtime_widgets, &runtime, &runtime_state.borrow());

                let (tx, rx) = std::sync::mpsc::channel::<RuntimeMessage>();
                let target_tag_for_thread = target_tag.clone();
                let runtime_for_thread = runtime.clone();
                std::thread::spawn(move || {
                    let progress_tx = tx.clone();
                    let result = install_runtime_package(
                        &runtime_for_thread,
                        &target_tag_for_thread,
                        move |progress, detail| {
                            let _ = progress_tx
                                .send(RuntimeMessage::InstallProgress { progress, detail });
                        },
                    );
                    match result {
                        Ok(()) => {
                            let _ = tx.send(RuntimeMessage::InstallFinished {
                                version: target_tag_for_thread,
                            });
                        }
                        Err(err) => {
                            let _ = tx.send(RuntimeMessage::InstallFailed(err.to_string()));
                        }
                    }
                });

                let runtime_state_poll = runtime_state.clone();
                let runtime_widgets_poll = runtime_widgets.clone();
                let runtime_poll = runtime.clone();
                gtk::glib::timeout_add_local(Duration::from_millis(20), move || {
                    match rx.try_recv() {
                        Ok(RuntimeMessage::InstallProgress { progress, detail }) => {
                            if let Some(operation) =
                                runtime_state_poll.borrow_mut().operation.as_mut()
                            {
                                operation.progress = progress;
                                operation.detail = detail;
                            }
                            render_runtime_widgets(
                                &runtime_widgets_poll,
                                &runtime_poll,
                                &runtime_state_poll.borrow(),
                            );
                            gtk::glib::ControlFlow::Continue
                        }
                        Ok(RuntimeMessage::InstallFinished { version }) => {
                            {
                                let mut card = runtime_state_poll.borrow_mut();
                                card.is_installed = true;
                                card.installed_version = Some(version.clone());
                                card.latest_checked_version = Some(version.clone());
                                card.operation = None;
                                card.last_check_error = None;
                            }
                            render_runtime_widgets(
                                &runtime_widgets_poll,
                                &runtime_poll,
                                &runtime_state_poll.borrow(),
                            );
                            gtk::glib::ControlFlow::Break
                        }
                        Ok(RuntimeMessage::InstallFailed(err)) => {
                            {
                                let mut card = runtime_state_poll.borrow_mut();
                                card.operation = None;
                                card.last_check_error = Some(err.clone());
                                refresh_local_runtime_state(&runtime_poll, &mut card);
                            }
                            render_runtime_widgets(
                                &runtime_widgets_poll,
                                &runtime_poll,
                                &runtime_state_poll.borrow(),
                            );
                            gtk::glib::ControlFlow::Break
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            gtk::glib::ControlFlow::Continue
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            {
                                let mut card = runtime_state_poll.borrow_mut();
                                card.operation = None;
                                card.last_check_error = Some("后台安装线程已断开".to_string());
                                refresh_local_runtime_state(&runtime_poll, &mut card);
                            }
                            render_runtime_widgets(
                                &runtime_widgets_poll,
                                &runtime_poll,
                                &runtime_state_poll.borrow(),
                            );
                            gtk::glib::ControlFlow::Break
                        }
                        _ => gtk::glib::ControlFlow::Break,
                    }
                });
            }
        });

        page
    }

    pub fn sync_from_state(&self) {
        let settings = self.state.settings.borrow().clone();
        self.populate_ui_controls(&settings);
        {
            let mut runtime_card = self.runtime_state.borrow_mut();
            refresh_local_runtime_state(&settings.core.gimi_runtime, &mut runtime_card);
        }
        render_runtime_widgets(
            &self.runtime_widgets,
            &settings.core.gimi_runtime,
            &self.runtime_state.borrow(),
        );
    }

    fn populate_ui_controls(&self, settings: &AppSettings) {
        let selected = UI_LANGUAGE_OPTIONS
            .iter()
            .position(|option| *option == settings.ui.language)
            .unwrap_or(0);
        self.language_dropdown.set_selected(selected as u32);
        self.night_mode_switch.set_active(settings.ui.night_mode);
        self.concurrent_downloads_spin
            .set_value(settings.network.concurrent_downloads.max(1) as f64);
    }
}

fn build_runtime_card() -> (gtk::Box, RuntimeWidgets) {
    let card = card_box();

    let name_label = gtk::Label::builder()
        .label("GIMI")
        .halign(gtk::Align::Start)
        .css_classes(["runtime-name"])
        .build();
    let current_version_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .css_classes(["runtime-current-version"])
        .build();
    let arrow_label = gtk::Label::builder()
        .label("→")
        .halign(gtk::Align::Start)
        .css_classes(["runtime-version-arrow"])
        .visible(false)
        .build();
    let latest_version_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .css_classes(["runtime-latest-version"])
        .visible(false)
        .build();

    let header_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(14)
        .margin_end(14)
        .margin_top(14)
        .margin_bottom(8)
        .build();
    header_row.append(&name_label);

    let version_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    version_row.append(&current_version_label);
    version_row.append(&arrow_label);
    version_row.append(&latest_version_label);
    header_row.append(&version_row);
    card.append(&header_row);

    let progress_bar = gtk::ProgressBar::builder()
        .hexpand(true)
        .css_classes(["runtime-progress-bar"])
        .build();
    let progress_label = gtk::Label::builder()
        .label("0%")
        .css_classes(["runtime-progress-label"])
        .halign(gtk::Align::End)
        .build();
    let progress_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(14)
        .margin_end(14)
        .margin_top(6)
        .margin_bottom(4)
        .visible(false)
        .build();
    progress_row.append(
        &gtk::Label::builder()
            .label("进度")
            .css_classes(["caption-heading"])
            .halign(gtk::Align::Start)
            .build(),
    );
    progress_row.append(&progress_bar);
    progress_row.append(&progress_label);
    card.append(&progress_row);

    let primary_button = gtk::Button::builder()
        .css_classes(["suggested-action"])
        .visible(false)
        .build();
    let check_button = gtk::Button::builder().label("检查更新").build();

    let actions_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .halign(gtk::Align::End)
        .margin_start(14)
        .margin_end(14)
        .margin_top(8)
        .margin_bottom(14)
        .build();
    actions_row.append(&primary_button);
    actions_row.append(&check_button);
    card.append(&actions_row);

    (
        card,
        RuntimeWidgets {
            name_label,
            current_version_label,
            arrow_label,
            latest_version_label,
            primary_button,
            check_button,
            progress_row,
            progress_bar,
            progress_label,
        },
    )
}

fn refresh_local_runtime_state(runtime: &GimiRuntimeSettings, card: &mut RuntimeCardState) {
    card.is_installed = runtime_payload_exists(runtime.importer_directory.as_path());
    card.installed_version = read_installed_version(runtime).ok().flatten();
    if !card.is_installed {
        card.installed_version = None;
    }
}

fn render_runtime_widgets(
    widgets: &RuntimeWidgets,
    runtime: &GimiRuntimeSettings,
    card: &RuntimeCardState,
) {
    if card.is_installed {
        widgets.name_label.remove_css_class("runtime-name-missing");
    } else {
        widgets.name_label.add_css_class("runtime-name-missing");
    }

    widgets.current_version_label.set_text(
        match (card.is_installed, card.installed_version.as_deref()) {
            (false, _) => "未安装",
            (true, Some(version)) => version,
            (true, None) => "未知版本",
        },
    );

    let available_update = available_update_tag(runtime, card);
    if let Some(next_version) = available_update.as_deref() {
        widgets.arrow_label.set_visible(true);
        widgets.latest_version_label.set_visible(true);
        widgets.latest_version_label.set_text(next_version);
    } else {
        widgets.arrow_label.set_visible(false);
        widgets.latest_version_label.set_visible(false);
        widgets.latest_version_label.set_text("");
    }

    if let Some(operation) = &card.operation {
        widgets.progress_row.set_visible(true);
        widgets
            .progress_bar
            .set_fraction((operation.progress as f64 / 100.0).clamp(0.0, 1.0));
        widgets
            .progress_label
            .set_text(&format!("{}%", operation.progress));
        widgets.primary_button.set_label(match operation.kind {
            RuntimeOperationKind::Download => "下载中",
            RuntimeOperationKind::Update => "更新中",
        });
        widgets.primary_button.set_visible(false);
        widgets.check_button.set_visible(false);
        widgets.primary_button.set_sensitive(false);
        widgets.check_button.set_sensitive(false);
        return;
    }

    widgets.progress_row.set_visible(false);
    widgets.progress_bar.set_fraction(0.0);
    widgets.progress_label.set_text("0%");
    widgets.primary_button.set_sensitive(true);
    widgets.check_button.set_sensitive(!card.checking);

    if !card.is_installed {
        widgets.primary_button.set_label("下载");
        widgets.primary_button.set_visible(true);
        widgets.check_button.set_visible(false);
        return;
    }

    widgets.check_button.set_visible(true);
    widgets.check_button.set_label(if card.checking {
        "检查中"
    } else {
        "检查更新"
    });

    if available_update.is_some() {
        widgets.primary_button.set_label("更新");
        widgets.primary_button.set_visible(true);
    } else {
        widgets.primary_button.set_visible(false);
    }
}

fn preferred_target_tag(runtime: &GimiRuntimeSettings, card: &RuntimeCardState) -> String {
    match &card.latest_checked_version {
        Some(remote)
            if compare_version_tags(remote, &runtime.managed_version) == Ordering::Greater =>
        {
            remote.clone()
        }
        _ => runtime.managed_version.clone(),
    }
}

fn available_update_tag(runtime: &GimiRuntimeSettings, card: &RuntimeCardState) -> Option<String> {
    if !card.is_installed {
        return None;
    }

    let target = preferred_target_tag(runtime, card);
    match card.installed_version.as_deref() {
        Some(current) => {
            if compare_version_tags(&target, current) == Ordering::Greater {
                Some(target)
            } else {
                None
            }
        }
        None => Some(target),
    }
}

fn fetch_latest_release_tag(runtime: &GimiRuntimeSettings) -> Result<String> {
    let html = anime_mod_manager::download_bytes(&runtime.releases_url())
        .map_err(|err| anyhow!("无法读取 GitHub release 页面: {err}"))?;
    let body = String::from_utf8(html).context("GitHub release 页面不是有效 UTF-8")?;
    let needle = format!(
        "/{}/{}/releases/tag/",
        runtime.github_repo_owner, runtime.github_repo_name
    );
    let start = body
        .find(&needle)
        .ok_or_else(|| anyhow!("未能在 GitHub release 页面中定位 tag"))?;
    let rest = &body[start + needle.len()..];
    let end = rest
        .find('"')
        .or_else(|| rest.find('?'))
        .ok_or_else(|| anyhow!("GitHub release 页面中的 tag 结构异常"))?;
    let tag = rest[..end].trim().to_string();
    if tag.is_empty() {
        return Err(anyhow!("GitHub release 页面返回了空 tag"));
    }
    Ok(tag)
}

fn install_runtime_package(
    runtime: &GimiRuntimeSettings,
    tag: &str,
    mut progress: impl FnMut(u8, String),
) -> Result<()> {
    progress(4, "正在准备目录...".to_string());
    fs::create_dir_all(&runtime.importer_directory)
        .with_context(|| format!("无法创建目录 {}", runtime.importer_directory.display()))?;
    fs::create_dir_all(runtime.mods_directory())
        .with_context(|| format!("无法创建目录 {}", runtime.mods_directory().display()))?;

    progress(8, "正在连接下载源...".to_string());
    let archive_bytes =
        download_archive_with_progress(&runtime.tag_archive_url(tag), |download_percent| {
            let overall = 12 + (download_percent as u16 * 46 / 100) as u8;
            progress(
                overall.min(58),
                format!("正在下载 {tag} ... {}%", download_percent),
            );
        })?;

    progress(58, "下载完成，正在清理旧 runtime ...".to_string());
    clear_managed_runtime_files(runtime)?;

    progress(66, "开始解压 GIMI 文件...".to_string());
    extract_runtime_archive(runtime, &archive_bytes, |percent, detail| {
        progress(percent, detail);
    })?;

    progress(97, "正在写入版本标记...".to_string());
    fs::write(runtime.version_marker_path(), tag).with_context(|| {
        format!(
            "无法写入版本标记 {}",
            runtime.version_marker_path().display()
        )
    })?;
    fs::create_dir_all(runtime.mods_directory())
        .with_context(|| format!("无法创建目录 {}", runtime.mods_directory().display()))?;

    progress(100, format!("{tag} 安装完成"));
    Ok(())
}

fn download_archive_with_progress(url: &str, mut on_progress: impl FnMut(u8)) -> Result<Vec<u8>> {
    let mut response = minreq::get(url)
        .with_header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .with_timeout(60)
        .send_lazy()
        .map_err(|err| anyhow!("下载请求失败: {err}"))?;

    if response.status_code != 200 {
        return Err(anyhow!("下载请求返回 HTTP {}", response.status_code));
    }

    let total_size = response
        .headers
        .get("content-length")
        .and_then(|value| value.parse::<u64>().ok());
    let mut bytes = Vec::new();
    if let Some(total) = total_size {
        let reserve = usize::try_from(total).unwrap_or(0);
        if reserve > 0 {
            bytes.reserve(reserve);
        }
    }

    let mut buffer = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut last_progress = 0u8;

    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|err| anyhow!("下载过程中读取失败: {err}"))?;
        if read == 0 {
            break;
        }

        bytes.extend_from_slice(&buffer[..read]);
        downloaded += read as u64;

        let progress = if let Some(total) = total_size {
            ((downloaded.saturating_mul(100)) / total.max(1)).min(100) as u8
        } else {
            ((downloaded / (128 * 1024)).min(95)) as u8
        };

        if progress > last_progress {
            last_progress = progress;
            on_progress(progress);
        }
    }

    on_progress(100);
    Ok(bytes)
}

fn clear_managed_runtime_files(runtime: &GimiRuntimeSettings) -> Result<()> {
    for relative in ["Core", "ShaderFixes"] {
        let path = runtime.importer_directory.join(relative);
        if path.exists() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("无法清理旧目录 {}", path.display()))?;
        }
    }

    let ini_path = runtime.importer_directory.join("d3dx.ini");
    if ini_path.exists() {
        fs::remove_file(&ini_path)
            .with_context(|| format!("无法删除旧文件 {}", ini_path.display()))?;
    }

    Ok(())
}

fn extract_runtime_archive(
    runtime: &GimiRuntimeSettings,
    archive_bytes: &[u8],
    mut progress: impl FnMut(u8, String),
) -> Result<()> {
    let cursor = Cursor::new(archive_bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("下载的压缩包不是有效 ZIP")?;

    let mut payload_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("无法读取压缩包条目 #{index}"))?;
        if let Some(relative) = runtime_payload_relative_path(&entry.mangled_name()) {
            if !relative.as_os_str().is_empty() {
                payload_entries.push((index, relative, entry.is_dir()));
            }
        }
    }

    if payload_entries.is_empty() {
        return Err(anyhow!("压缩包里没有找到 GIMI 运行时文件"));
    }

    let total_bytes = payload_entries
        .iter()
        .filter(|(_, _, is_dir)| !*is_dir)
        .try_fold(0u64, |acc, (index, _, is_dir)| {
            if *is_dir {
                return Ok(acc);
            }
            let entry = archive
                .by_index(*index)
                .with_context(|| format!("无法读取压缩包条目 #{index}"))?;
            Ok::<u64, anyhow::Error>(acc.saturating_add(entry.size()))
        })?
        .max(1);
    let mut processed_bytes = 0u64;
    let mut last_percent = 65u8;
    let mut buffer = [0u8; 64 * 1024];

    for (index, relative, is_dir) in payload_entries {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("无法重新读取压缩包条目 #{index}"))?;
        let target_path = runtime.importer_directory.join(&relative);

        if is_dir {
            fs::create_dir_all(&target_path)
                .with_context(|| format!("无法创建目录 {}", target_path.display()))?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("无法创建目录 {}", parent.display()))?;
        }

        let mut output = fs::File::create(&target_path)
            .with_context(|| format!("无法写入文件 {}", target_path.display()))?;

        loop {
            let read = entry
                .read(&mut buffer)
                .with_context(|| format!("无法读取压缩包文件 {}", relative.display()))?;
            if read == 0 {
                break;
            }

            use std::io::Write;
            output
                .write_all(&buffer[..read])
                .with_context(|| format!("无法解压文件 {}", target_path.display()))?;

            processed_bytes = processed_bytes.saturating_add(read as u64);
            let percent = 66 + ((processed_bytes.saturating_mul(29)) / total_bytes) as u8;
            let percent = percent.min(95);
            if percent > last_percent {
                last_percent = percent;
                progress(percent, format!("正在解压 {}", relative.display()));
            }
        }
    }

    Ok(())
}

fn runtime_payload_relative_path(entry_path: &Path) -> Option<PathBuf> {
    let components: Vec<_> = entry_path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_os_string()),
            _ => None,
        })
        .collect();

    let gimi_index = components.iter().position(|value| value == "GIMI")?;

    let mut relative = PathBuf::new();
    for component in components.into_iter().skip(gimi_index + 1) {
        relative.push(component);
    }

    Some(relative)
}

fn read_installed_version(runtime: &GimiRuntimeSettings) -> Result<Option<String>> {
    let version_path = runtime.version_marker_path();
    if !version_path.is_file() {
        return Ok(None);
    }

    let version = fs::read_to_string(&version_path)
        .with_context(|| format!("无法读取版本标记 {}", version_path.display()))?
        .trim()
        .to_string();

    if version.is_empty() {
        Ok(None)
    } else {
        Ok(Some(version))
    }
}

fn runtime_payload_exists(importer_directory: &Path) -> bool {
    importer_directory.join("d3dx.ini").is_file()
        && importer_directory.join("Core").is_dir()
        && importer_directory.join("ShaderFixes").is_dir()
}

fn compare_version_tags(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());

    for index in 0..max_len {
        let l = *left_parts.get(index).unwrap_or(&0);
        let r = *right_parts.get(index).unwrap_or(&0);
        match l.cmp(&r) {
            Ordering::Equal => continue,
            other => return other,
        }
    }

    left.cmp(right)
}

fn version_parts(version: &str) -> Vec<u32> {
    let trimmed = version.trim_start_matches(|ch: char| !ch.is_ascii_digit());
    let mut parts = Vec::new();
    let mut current = String::new();

    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            if let Ok(value) = current.parse::<u32>() {
                parts.push(value);
            }
            current.clear();
        }
    }

    if !current.is_empty() {
        if let Ok(value) = current.parse::<u32>() {
            parts.push(value);
        }
    }

    parts
}

fn selected_language(dropdown: &gtk::DropDown) -> String {
    UI_LANGUAGE_OPTIONS
        .get(dropdown.selected() as usize)
        .copied()
        .unwrap_or("zh-CN")
        .to_string()
}

fn section_shell(title: &str) -> gtk::Box {
    let section = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();

    section.append(
        &gtk::Label::builder()
            .label(title)
            .css_classes(["heading"])
            .halign(gtk::Align::Start)
            .build(),
    );
    section
}

fn card_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["card"])
        .build()
}

fn build_widget_row(title: &str, description: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(14)
        .margin_end(14)
        .margin_top(10)
        .margin_bottom(10)
        .build();

    let text_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .hexpand(true)
        .build();
    text_box.append(
        &gtk::Label::builder()
            .label(title)
            .halign(gtk::Align::Start)
            .css_classes(["caption-heading"])
            .build(),
    );
    text_box.append(
        &gtk::Label::builder()
            .label(description)
            .halign(gtk::Align::Start)
            .css_classes(["dim-label"])
            .wrap(true)
            .build(),
    );

    row.append(&text_box);
    row.append(widget);
    row
}

fn separator() -> gtk::Separator {
    gtk::Separator::new(gtk::Orientation::Horizontal)
}
