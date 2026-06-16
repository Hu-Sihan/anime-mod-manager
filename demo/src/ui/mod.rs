mod browse_page;
mod card_frame;
mod catalog_filter;
mod download_module;
mod download_page;
mod download_scheduler;
mod download_task;
mod fixed_size_frame;
mod local_page;
mod mod_card;
mod mod_detail_window;
mod settings_page;
mod sidebar;

pub use browse_page::BrowsePage;
pub(crate) use download_module::DownloadModule;
pub use download_page::DownloadPage;
pub(crate) use download_scheduler::{DownloadTask, DownloadTaskPhase};
pub use local_page::LocalPage;
pub use mod_card::ModCardWidget;
pub use settings_page::SettingsPage;
pub use sidebar::Sidebar;

use std::cell::{Cell, RefCell};
use std::fs;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use adw::prelude::*;
use gtk;

use crate::config::{app_runtime_dir, AppSettings, GimiRuntimeSettings};
use crate::perf;
use anime_mod_manager::{game_ids, GameBananaClient, MetaManager, ModFileDownloader, ModManager};

// ─── Shared app state ────────────────────────────────────────

type UiListener = Rc<dyn Fn()>;

pub struct AppState {
    pub client: GameBananaClient,
    pub mod_file_downloader: Arc<ModFileDownloader>,
    pub meta_manager: MetaManager,
    pub manager: ModManager,
    pub downloads: DownloadModule,
    pub settings: RefCell<AppSettings>,
    pub current_tab: RefCell<TabPage>,
    installed_listeners: RefCell<Vec<UiListener>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabPage {
    Local,
    Download,
    Browse,
    Settings,
}

impl AppState {
    pub fn subscribe_installed_changed(&self, listener: impl Fn() + 'static) {
        self.installed_listeners
            .borrow_mut()
            .push(Rc::new(listener));
    }

    pub fn notify_installed_changed(&self) {
        let listeners = self.installed_listeners.borrow().clone();
        for listener in listeners {
            listener();
        }
    }

    pub fn persist_settings(&self) {
        let _ = self.settings.borrow().save();
    }
}

// ─── Main Window ─────────────────────────────────────────────

pub struct MainWindow {
    pub _window: adw::ApplicationWindow,
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Rc<Self> {
        let _perf = perf::ScopeTimer::with_threshold("MainWindow::new", 1);
        let content_stack = gtk::Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();

        let app_settings = AppSettings::load_or_default();
        migrate_legacy_gimi_layout(&app_settings.core.gimi_runtime).ok();
        let mods_dir = app_settings.core.gimi_runtime.mods_directory();
        let meta_manager = MetaManager::new(ModManager::meta_roots_for(&mods_dir));
        let manager = ModManager::new(mods_dir, meta_manager.clone());

        let state = Rc::new(AppState {
            client: GameBananaClient::new(game_ids::GENSHIN_IMPACT),
            mod_file_downloader: Arc::new(ModFileDownloader::new()),
            meta_manager,
            manager,
            downloads: DownloadModule::new(app_settings.network.concurrent_downloads_usize()),
            settings: RefCell::new(app_settings),
            current_tab: RefCell::new(TabPage::Browse),
            installed_listeners: RefCell::new(Vec::new()),
        });

        state.manager.init().ok();
        state.meta_manager.scan().ok();
        state.manager.initialize_from_meta().ok();
        state.downloads.initialize_from_meta(&state);

        // Build pages
        let start = perf::now();
        let browse = Rc::new(BrowsePage::new(state.clone()));
        perf::log_elapsed_with_threshold("BrowsePage::new", start, 1);

        let start = perf::now();
        let download = Rc::new(DownloadPage::new(state.clone()));
        perf::log_elapsed_with_threshold("DownloadPage::new", start, 1);

        let start = perf::now();
        let local = Rc::new(LocalPage::new(state.clone()));
        perf::log_elapsed_with_threshold("LocalPage::new", start, 1);

        let start = perf::now();
        let settings = Rc::new(SettingsPage::new(state.clone()));
        perf::log_elapsed_with_threshold("SettingsPage::new", start, 1);

        let browse_weak = Rc::downgrade(&browse);
        state.subscribe_installed_changed(move || {
            if let Some(browse) = browse_weak.upgrade() {
                browse.refresh_installed_state();
            }
        });

        let local_weak = Rc::downgrade(&local);
        state.subscribe_installed_changed(move || {
            if let Some(local) = local_weak.upgrade() {
                local.refresh();
            }
        });

        let download_weak = Rc::downgrade(&download);
        state.downloads.subscribe(move || {
            if let Some(download) = download_weak.upgrade() {
                download.refresh();
            }
        });

        content_stack.add_titled(&browse.container, Some("browse"), "浏览");
        content_stack.add_titled(&download.container, Some("download"), "下载");
        content_stack.add_titled(&local.container, Some("local"), "本地");
        content_stack.add_titled(&settings.container, Some("settings"), "设置");
        content_stack.set_visible_child_name("browse");

        let previous_child = Rc::new(RefCell::new(String::from("browse")));
        let previous_child_for_notify = previous_child.clone();
        let browse_for_notify = browse.clone();
        let download_for_notify = download.clone();
        let local_for_notify = local.clone();
        let settings_for_notify = settings.clone();
        content_stack.connect_visible_child_name_notify(move |stack| {
            let current = stack
                .visible_child_name()
                .map(|name| name.to_string())
                .unwrap_or_default();
            let previous = previous_child_for_notify.replace(current.clone());

            if previous != "browse" && current == "browse" {
                browse_for_notify.refresh_installed_state();
            }
            if previous != "download" && current == "download" {
                download_for_notify.ensure_preview_loaded();
            }
            if previous != "local" && current == "local" {
                local_for_notify.refresh();
            }
            if previous != "settings" && current == "settings" {
                settings_for_notify.sync_from_state();
            }
        });

        // Sidebar
        let sidebar = Sidebar::new(state.clone(), &content_stack);
        let sidebar_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(60)
            .css_classes(["sidebar"])
            .build();
        sidebar_box.append(sidebar.widget());

        // Header bar
        let minimize_btn = window_control_button("pan-down-symbolic", "最小化", "minimize-button");
        let close_btn = window_control_button("window-close-symbolic", "关闭", "close-button");
        let header_controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["window-controls"])
            .valign(gtk::Align::Center)
            .build();
        header_controls.append(&minimize_btn);
        header_controls.append(&close_btn);

        let header = adw::HeaderBar::builder()
            .title_widget(&gtk::Label::new(Some("Anime Mod Manager")))
            .show_end_title_buttons(false)
            .css_classes(["flat", "compact-header"])
            .build();
        header.set_height_request(36);
        header.pack_end(&header_controls);

        // Layout
        let content_area = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        content_area.append(&header);
        content_area.append(&content_stack);

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build();
        main_box.append(&sidebar_box);
        main_box.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        main_box.append(&content_area);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Anime Mod Manager — Demo")
            .default_width(990)
            .default_height(600)
            .resizable(false)
            .build();

        let minimize_window = window.clone();
        minimize_btn.connect_clicked(move |_| {
            minimize_window.minimize();
        });

        let is_closing = Rc::new(Cell::new(false));
        let close_app = app.clone();
        let close_state = state.clone();
        let close_flag = is_closing.clone();
        close_btn.connect_clicked(move |_| {
            request_app_shutdown(&close_app, &close_state, &close_flag);
        });

        let close_app = app.clone();
        let close_state = state.clone();
        let close_flag = is_closing.clone();
        window.connect_close_request(move |_| {
            if close_flag.get() {
                return gtk::glib::Propagation::Proceed;
            }
            request_app_shutdown(&close_app, &close_state, &close_flag);
            gtk::glib::Propagation::Stop
        });

        window.set_content(Some(&main_box));
        window.present();

        Rc::new(Self { _window: window })
    }
}

fn request_app_shutdown(app: &adw::Application, state: &Rc<AppState>, is_closing: &Rc<Cell<bool>>) {
    const FORCE_QUIT_AFTER_MS: u64 = 600;

    if is_closing.replace(true) {
        return;
    }

    state.downloads.begin_shutdown(state);
    if !state.downloads.has_running_tasks() {
        app.quit();
        return;
    }

    let app = app.clone();
    let state = state.clone();
    let force_app = app.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(FORCE_QUIT_AFTER_MS), move || {
        force_app.quit();
    });

    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        if state.downloads.has_running_tasks() {
            gtk::glib::ControlFlow::Continue
        } else {
            app.quit();
            gtk::glib::ControlFlow::Break
        }
    });
}

fn window_control_button(icon_name: &str, tooltip: &str, extra_class: &str) -> gtk::Button {
    gtk::Button::builder()
        .child(
            &gtk::Image::builder()
                .icon_name(icon_name)
                .pixel_size(14)
                .build(),
        )
        .tooltip_text(tooltip)
        .css_classes(["window-control-button", extra_class])
        .valign(gtk::Align::Center)
        .build()
}

fn migrate_legacy_gimi_layout(runtime: &GimiRuntimeSettings) -> std::io::Result<()> {
    let legacy_root = app_runtime_dir();
    let target_root = runtime.importer_directory.as_path();
    if legacy_root == target_root {
        return Ok(());
    }

    let managed_entries = [
        "Core",
        "ShaderFixes",
        "Mods",
        "d3dx.ini",
        ".anime-mod-manager-version",
    ];

    let legacy_has_runtime = managed_entries
        .iter()
        .any(|entry| legacy_root.join(entry).exists());
    if !legacy_has_runtime {
        return Ok(());
    }

    let target_has_runtime = managed_entries
        .iter()
        .any(|entry| target_root.join(entry).exists());
    if target_has_runtime {
        return Ok(());
    }

    fs::create_dir_all(target_root)?;
    for entry in managed_entries {
        let source = legacy_root.join(entry);
        if !source.exists() {
            continue;
        }

        let destination = target_root.join(entry);
        fs::rename(source, destination)?;
    }

    Ok(())
}
