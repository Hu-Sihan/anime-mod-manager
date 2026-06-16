use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk;

use anime_mod_manager::{download_image, ModCard, ModDetail, ModFile};

use super::{fixed_size_frame::FixedSizeFrame, AppState, DownloadTask, DownloadTaskPhase};

const GALLERY_PAGE_SIZE: usize = 1;
const SIDEBAR_CLOSE_DELAY_MS: u64 = 220;
const DETAIL_DRAWER_WIDTH: i32 = 390;
const DETAIL_DRAWER_HANDLE_GUTTER: i32 = 51;
const DETAIL_DRAWER_BODY_WIDTH: i32 = DETAIL_DRAWER_WIDTH - 36;
const DETAIL_GALLERY_HEIGHT: i32 = DETAIL_DRAWER_BODY_WIDTH * 9 / 16;
const DETAIL_DRAWER_HANDLE_ICON_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/assets/detail-drawer-handle.svg"
);

#[derive(Clone)]
struct DetailWidgets {
    title_label: gtk::Label,
    name_label: gtk::Label,
    author_label: gtk::Label,
    tags_box: gtk::Box,
    description_label: gtk::Label,
    download_file_label: gtk::Label,
    download_button: gtk::Button,
    download_progress: gtk::DrawingArea,
    progress_fraction: Rc<Cell<f64>>,
    progress_title: gtk::Label,
    progress_file_label: gtk::Label,
    progress_pct: gtk::Label,
    download_stack: gtk::Stack,
    versions_button: gtk::Button,
    versions_box: gtk::Box,
    versions_frame: gtk::Box,
    gallery_tiles: Vec<gtk::Box>,
    gallery_pictures: Vec<gtk::Picture>,
    gallery_empty_label: gtk::Label,
    gallery_prev_button: gtk::Button,
    gallery_next_button: gtk::Button,
    gallery_counter_label: gtk::Label,
}

#[derive(Clone)]
struct DetailDownloadControls {
    download_file_label: gtk::Label,
    download_button: gtk::Button,
    download_progress: gtk::DrawingArea,
    progress_fraction: Rc<Cell<f64>>,
    progress_title: gtk::Label,
    progress_file_label: gtk::Label,
    progress_pct: gtk::Label,
    download_stack: gtk::Stack,
    versions_button: gtk::Button,
}

pub struct ModDetailDrawer {
    state: Rc<AppState>,
    scrim: gtk::Box,
    revealer: gtk::Revealer,
    widgets: DetailWidgets,
    latest_file: Rc<RefCell<Option<ModFile>>>,
    current_card: Rc<RefCell<Option<ModCard>>>,
    current_detail: Rc<RefCell<Option<ModDetail>>>,
    gallery_urls: Rc<RefCell<Vec<String>>>,
    gallery_page: Rc<RefCell<usize>>,
    open_token: Rc<RefCell<u64>>,
    current_download_task_id: Rc<Cell<Option<u64>>>,
    progress_binding_token: Rc<Cell<u64>>,
}

impl ModDetailDrawer {
    pub fn new(state: Rc<AppState>) -> Rc<Self> {
        let scrim = gtk::Box::builder()
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .css_classes(["detail-sidebar-scrim"])
            .build();

        let revealer = gtk::Revealer::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::Fill)
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(SIDEBAR_CLOSE_DELAY_MS as u32)
            .reveal_child(false)
            .build();

        let sidebar_shell = gtk::Overlay::builder()
            .width_request(DETAIL_DRAWER_WIDTH + DETAIL_DRAWER_HANDLE_GUTTER)
            .hexpand(false)
            .vexpand(true)
            .css_classes(["detail-sidebar-shell"])
            .build();
        let sidebar = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .width_request(DETAIL_DRAWER_WIDTH)
            .halign(gtk::Align::End)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["detail-sidebar"])
            .build();

        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_top(18)
            .margin_bottom(10)
            .margin_start(18)
            .margin_end(18)
            .build();
        let title_label = gtk::Label::builder()
            .label("模组详情")
            .halign(gtk::Align::Start)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["detail-sidebar-title"])
            .build();
        header.append(&title_label);
        sidebar.append(&header);

        let content_scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_start(18)
            .margin_end(18)
            .margin_bottom(12)
            .build();
        content_scrolled.set_child(Some(&body));
        sidebar.append(&content_scrolled);

        let gallery_overlay = gtk::Overlay::builder()
            .css_classes(["detail-gallery"])
            .hexpand(true)
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .overflow(gtk::Overflow::Hidden)
            .build();
        let gallery_frame = FixedSizeFrame::new(DETAIL_DRAWER_BODY_WIDTH, DETAIL_GALLERY_HEIGHT);
        gallery_frame.set_halign(gtk::Align::Fill);
        gallery_frame.set_child(Some(&gallery_overlay));
        let gallery_grid = gtk::Grid::builder()
            .column_homogeneous(true)
            .row_homogeneous(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        gallery_overlay.set_child(Some(&gallery_grid));

        let mut gallery_tiles = Vec::with_capacity(GALLERY_PAGE_SIZE);
        let mut gallery_pictures = Vec::with_capacity(GALLERY_PAGE_SIZE);
        for idx in 0..GALLERY_PAGE_SIZE {
            let tile = gtk::Box::builder()
                .css_classes(["detail-gallery-tile"])
                .halign(gtk::Align::Fill)
                .valign(gtk::Align::Fill)
                .hexpand(true)
                .vexpand(true)
                .overflow(gtk::Overflow::Hidden)
                .build();
            let picture = gtk::Picture::builder()
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .hexpand(true)
                .vexpand(true)
                .content_fit(gtk::ContentFit::Contain)
                .can_shrink(true)
                .build();
            tile.append(&picture);
            gallery_grid.attach(&tile, 0, idx as i32, 1, 1);
            gallery_tiles.push(tile);
            gallery_pictures.push(picture);
        }

        let gallery_empty_label = gtk::Label::builder()
            .label("暂无预览图")
            .css_classes(["title-4", "dim-label"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        gallery_overlay.add_overlay(&gallery_empty_label);

        let gallery_prev_button =
            gallery_arrow_button("go-previous-symbolic", gtk::Align::Start, 8);
        let gallery_next_button = gallery_arrow_button("go-next-symbolic", gtk::Align::End, 8);
        let gallery_counter_label = gtk::Label::builder()
            .label("0 / 0")
            .css_classes(["detail-gallery-counter"])
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(8)
            .build();
        gallery_overlay.add_overlay(&gallery_prev_button);
        gallery_overlay.add_overlay(&gallery_next_button);
        gallery_overlay.add_overlay(&gallery_counter_label);
        body.append(&gallery_frame);

        let meta_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        let name_label = gtk::Label::builder()
            .label("选择一个模组")
            .halign(gtk::Align::Start)
            .wrap(true)
            .css_classes(["detail-mod-name"])
            .build();
        meta_box.append(&name_label);

        let author_label = gtk::Label::builder()
            .label("")
            .halign(gtk::Align::Start)
            .css_classes(["detail-mod-author"])
            .build();
        meta_box.append(&author_label);

        let tags_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .halign(gtk::Align::Start)
            .margin_top(8)
            .build();
        meta_box.append(&tags_box);
        body.append(&meta_box);

        let description_label = gtk::Label::builder()
            .label("点击模组卡片后会在这里显示简介。")
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Start)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .css_classes(["detail-description"])
            .build();
        body.append(&description_label);

        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::End)
            .css_classes(["detail-action-panel"])
            .build();

        // Versions list panel (appears above action row, dynamic height capped at 2:1)
        let versions_frame = gtk::Box::builder()
            .width_request(DETAIL_DRAWER_BODY_WIDTH)
            .halign(gtk::Align::Center)
            .visible(false)
            .overflow(gtk::Overflow::Hidden)
            .css_classes(["detail-versions-panel"])
            .build();
        let versions_scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(false)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        let versions_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .build();
        versions_scrolled.set_child(Some(&versions_box));
        versions_frame.append(&versions_scrolled);
        footer.append(&versions_frame);

        let action_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["detail-action-row"])
            .build();
        let download_button = gtk::Button::builder()
            .css_classes(["suggested-action", "detail-download-button"])
            .hexpand(true)
            .sensitive(false)
            .overflow(gtk::Overflow::Hidden)
            .build();
        let download_button_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        download_button_content.append(
            &gtk::Label::builder()
                .label("下载")
                .halign(gtk::Align::Center)
                .justify(gtk::Justification::Center)
                .css_classes(["detail-download-title"])
                .build(),
        );
        let download_file_label = gtk::Label::builder()
            .label("正在加载文件列表...")
            .halign(gtk::Align::Center)
            .justify(gtk::Justification::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(32)
            .css_classes(["detail-download-file"])
            .build();
        download_button_content.append(&download_file_label);
        download_button.set_child(Some(&download_button_content));

        let progress_fraction = Rc::new(Cell::new(0.0));
        let fraction_for_draw = progress_fraction.clone();
        let download_progress = gtk::DrawingArea::builder()
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Center)
            .css_classes(["detail-progress-bar"])
            .build();
        download_progress.set_draw_func(move |_, cr, width, height| {
            draw_detail_progress_bar(cr, width, height, fraction_for_draw.get());
        });
        let progress_title = gtk::Label::builder()
            .halign(gtk::Align::Center)
            .justify(gtk::Justification::Center)
            .css_classes(["detail-progress-title"])
            .build();
        let progress_file_label = gtk::Label::builder()
            .halign(gtk::Align::Center)
            .justify(gtk::Justification::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(32)
            .css_classes(["detail-progress-file"])
            .build();
        let progress_pct = gtk::Label::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .margin_end(14)
            .css_classes(["detail-progress-pct"])
            .build();
        let progress_text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(1)
            .hexpand(true)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .margin_start(18)
            .margin_end(18)
            .build();
        progress_text.append(&progress_title);
        progress_text.append(&progress_file_label);
        let progress_overlay = gtk::Overlay::builder()
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Center)
            .overflow(gtk::Overflow::Hidden)
            .css_classes(["detail-download-progress"])
            .build();
        progress_overlay.set_child(Some(&download_progress));
        progress_overlay.add_overlay(&progress_text);
        progress_overlay.add_overlay(&progress_pct);

        // Stack to toggle between button and progress bar
        let download_stack = gtk::Stack::new();
        download_stack.set_vexpand(false);
        download_stack.set_valign(gtk::Align::Center);
        download_stack.add_named(&download_button, Some("button"));
        download_stack.add_named(&progress_overlay, Some("progress"));
        download_stack.set_visible_child_name("button");

        let versions_button = gtk::Button::builder()
            .css_classes(["detail-version-button"])
            .sensitive(false)
            .build();
        versions_button.set_child(Some(
            &gtk::Image::builder()
                .icon_name("pan-up-symbolic")
                .pixel_size(16)
                .build(),
        ));

        action_row.append(&download_stack);
        action_row.append(&versions_button);
        footer.append(&action_row);
        sidebar.append(&footer);

        let collapse_button = gtk::Button::builder()
            .width_request(DETAIL_DRAWER_HANDLE_GUTTER)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .margin_start(0)
            .css_classes(["detail-sidebar-handle"])
            .build();
        let collapse_icon = gtk::Picture::for_filename(DETAIL_DRAWER_HANDLE_ICON_PATH);
        collapse_icon.set_width_request(40);
        collapse_icon.set_height_request(40);
        collapse_icon.set_halign(gtk::Align::Center);
        collapse_icon.set_valign(gtk::Align::Center);
        collapse_icon.set_can_shrink(true);
        collapse_icon.add_css_class("detail-sidebar-handle-icon");
        collapse_button.set_child(Some(&collapse_icon));

        sidebar_shell.set_child(Some(&sidebar));
        sidebar_shell.add_overlay(&collapse_button);
        revealer.set_child(Some(&sidebar_shell));

        let widgets = DetailWidgets {
            title_label,
            name_label,
            author_label,
            tags_box,
            description_label,
            download_file_label,
            download_button,
            download_progress,
            progress_fraction,
            progress_title,
            progress_file_label,
            progress_pct,
            download_stack,
            versions_button,
            versions_box,
            versions_frame,
            gallery_tiles,
            gallery_pictures,
            gallery_empty_label,
            gallery_prev_button,
            gallery_next_button,
            gallery_counter_label,
        };

        let drawer = Rc::new(Self {
            state,
            scrim,
            revealer,
            widgets,
            latest_file: Rc::new(RefCell::new(None)),
            current_card: Rc::new(RefCell::new(None)),
            current_detail: Rc::new(RefCell::new(None)),
            gallery_urls: Rc::new(RefCell::new(Vec::new())),
            gallery_page: Rc::new(RefCell::new(0)),
            open_token: Rc::new(RefCell::new(0)),
            current_download_task_id: Rc::new(Cell::new(None)),
            progress_binding_token: Rc::new(Cell::new(0)),
        });

        connect_drawer_signals(&drawer, collapse_button, collapse_icon);
        drawer
    }

    pub fn scrim(&self) -> &gtk::Box {
        &self.scrim
    }

    pub fn revealer(&self) -> &gtk::Revealer {
        &self.revealer
    }

    pub fn open(&self, card: ModCard) {
        *self.current_card.borrow_mut() = Some(card.clone());
        *self.current_detail.borrow_mut() = None;
        *self.latest_file.borrow_mut() = None;
        *self.gallery_page.borrow_mut() = 0;
        *self.open_token.borrow_mut() += 1;
        let token = *self.open_token.borrow();
        set_detail_task_binding(
            &self.current_download_task_id,
            &self.progress_binding_token,
            None,
        );

        self.scrim.set_visible(true);
        self.revealer.set_reveal_child(true);
        self.widgets.versions_frame.set_visible(false);
        // Reset button to default state
        self.widgets
            .download_button
            .remove_css_class("detail-installed");
        self.widgets
            .download_button
            .add_css_class("suggested-action");
        set_button_title(&self.widgets.download_button, "下载");
        sync_detail_action_heights(
            &self.widgets.download_button,
            &self.widgets.download_progress,
            &self.widgets.versions_button,
        );
        self.widgets.download_stack.set_visible_child_name("button");
        self.widgets.title_label.set_text(&card.name);
        self.widgets.name_label.set_text(&card.name);
        self.widgets
            .author_label
            .set_text(&format!("by {}", card.author));

        // Check download / install status
        let active_download =
            find_active_download_task(&self.state.downloads.snapshot(), card.id, None);
        let is_installed = is_mod_installed(&self.state, card.id);

        if let Some(ref task) = active_download {
            let binding_revision = set_detail_task_binding(
                &self.current_download_task_id,
                &self.progress_binding_token,
                Some(task.id),
            );
            render_detail_task_state(&self.widgets, task);
            self.widgets.versions_button.set_sensitive(false);
            watch_detail_task(
                detail_download_controls(&self.widgets),
                self.state.clone(),
                self.open_token.clone(),
                token,
                self.current_download_task_id.clone(),
                self.progress_binding_token.clone(),
                binding_revision,
                task.id,
            );
        } else if is_installed {
            set_button_title(&self.widgets.download_button, "已安装");
            self.widgets
                .download_button
                .remove_css_class("suggested-action");
            self.widgets
                .download_button
                .add_css_class("detail-installed");
            self.widgets.download_button.set_sensitive(false);
            self.widgets.versions_button.set_sensitive(false);
        } else {
            self.widgets
                .download_file_label
                .set_text("正在加载文件列表...");
            self.widgets.download_button.set_sensitive(false);
            self.widgets.versions_button.set_sensitive(false);
        }
        render_tags(
            &self.widgets.tags_box,
            &card.category,
            card.subcategory.as_deref(),
            card.is_r18,
        );
        self.widgets.description_label.set_text("正在加载简介...");
        clear_versions_box(&self.widgets.versions_box);
        let gallery_urls = initial_gallery_urls(&card);
        *self.gallery_urls.borrow_mut() = gallery_urls;
        render_gallery(&self.widgets, &self.gallery_urls.borrow(), 0);
        preload_gallery_urls(&self.gallery_urls.borrow());

        let client = self.state.client.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<ModDetail, String>>();
        std::thread::spawn(move || {
            let result = client.get_mod(card.id).map_err(|err| err.to_string());
            let _ = tx.send(result);
        });

        let widgets = self.widgets.clone();
        let state = self.state.clone();
        let latest_file = self.latest_file.clone();
        let current_card = self.current_card.clone();
        let current_detail = self.current_detail.clone();
        let gallery_urls = self.gallery_urls.clone();
        let gallery_page = self.gallery_page.clone();
        let open_token = self.open_token.clone();
        let current_download_task_id = self.current_download_task_id.clone();
        let progress_binding_token = self.progress_binding_token.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(120), move || match rx.try_recv() {
            Ok(Ok(detail)) => {
                if *open_token.borrow() != token {
                    return gtk::glib::ControlFlow::Break;
                }
                let Some(card) = current_card.borrow().clone() else {
                    return gtk::glib::ControlFlow::Break;
                };
                populate_detail(
                    &widgets,
                    state.clone(),
                    card,
                    detail,
                    latest_file.clone(),
                    current_detail.clone(),
                    gallery_urls.clone(),
                    gallery_page.clone(),
                    open_token.clone(),
                    current_download_task_id.clone(),
                    progress_binding_token.clone(),
                    token,
                );
                gtk::glib::ControlFlow::Break
            }
            Ok(Err(_err)) => {
                if *open_token.borrow() == token {
                    *current_detail.borrow_mut() = None;
                    widgets.description_label.set_text("未能加载模组详情。");
                    widgets.download_file_label.set_text("无法读取可下载文件。");
                }
                gtk::glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                if *open_token.borrow() == token {}
                gtk::glib::ControlFlow::Break
            }
        });
    }

    pub fn close(&self) {
        set_detail_task_binding(
            &self.current_download_task_id,
            &self.progress_binding_token,
            None,
        );
        self.widgets.versions_frame.set_visible(false);
        self.revealer.set_reveal_child(false);
        let scrim = self.scrim.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(SIDEBAR_CLOSE_DELAY_MS), move || {
            scrim.set_visible(false);
            gtk::glib::ControlFlow::Break
        });
    }
}

fn connect_drawer_signals(
    drawer: &Rc<ModDetailDrawer>,
    collapse_button: gtk::Button,
    collapse_icon: gtk::Picture,
) {
    let weak = Rc::downgrade(drawer);
    collapse_button.connect_clicked(move |_| {
        if let Some(drawer) = weak.upgrade() {
            drawer.close();
        }
    });

    let animation_source = Rc::new(RefCell::new(None::<gtk::glib::SourceId>));
    let animation_phase = Rc::new(Cell::new(0.0f64));

    let motion = gtk::EventControllerMotion::new();
    {
        let animation_source = animation_source.clone();
        let animation_phase = animation_phase.clone();
        let icon = collapse_icon.clone();
        motion.connect_enter(move |_, _, _| {
            icon.add_css_class("detail-sidebar-handle-icon-active");
            if animation_source.borrow().is_some() {
                return;
            }
            animation_phase.set(0.0);
            let icon_for_tick = icon.clone();
            let animation_source_for_tick = animation_source.clone();
            let animation_phase_for_tick = animation_phase.clone();
            let source = gtk::glib::timeout_add_local(Duration::from_millis(34), move || {
                let phase = animation_phase_for_tick.get() + 0.22;
                animation_phase_for_tick.set(phase);

                let offset = (phase.sin() * 6.0).round() as i32;
                icon_for_tick.set_margin_start(offset.max(0));
                icon_for_tick.set_margin_end((-offset).max(0));
                icon_for_tick.set_opacity(0.58 + ((phase * 1.35).sin() + 1.0) * 0.21);

                if icon_for_tick.has_css_class("detail-sidebar-handle-icon-active") {
                    gtk::glib::ControlFlow::Continue
                } else {
                    icon_for_tick.set_margin_start(0);
                    icon_for_tick.set_margin_end(0);
                    icon_for_tick.set_opacity(1.0);
                    *animation_source_for_tick.borrow_mut() = None;
                    gtk::glib::ControlFlow::Break
                }
            });
            *animation_source.borrow_mut() = Some(source);
        });
    }
    {
        let icon = collapse_icon.clone();
        motion.connect_leave(move |_| {
            icon.remove_css_class("detail-sidebar-handle-icon-active");
            icon.set_margin_start(0);
            icon.set_margin_end(0);
            icon.set_opacity(1.0);
        });
    }
    collapse_button.add_controller(motion);

    let weak = Rc::downgrade(drawer);
    let gesture = gtk::GestureClick::new();
    gesture.connect_pressed(move |_, _, _, _| {
        if let Some(drawer) = weak.upgrade() {
            drawer.close();
        }
    });
    drawer.scrim.add_controller(gesture);

    let weak = Rc::downgrade(drawer);
    drawer
        .widgets
        .gallery_prev_button
        .clone()
        .connect_clicked(move |_| {
            if let Some(drawer) = weak.upgrade() {
                let mut page = drawer.gallery_page.borrow_mut();
                if *page > 0 {
                    *page -= 1;
                }
                render_gallery(&drawer.widgets, &drawer.gallery_urls.borrow(), *page);
            }
        });

    let weak = Rc::downgrade(drawer);
    drawer
        .widgets
        .gallery_next_button
        .clone()
        .connect_clicked(move |_| {
            if let Some(drawer) = weak.upgrade() {
                let total_pages = drawer.gallery_urls.borrow().len().max(1);
                let mut page = drawer.gallery_page.borrow_mut();
                if *page + 1 < total_pages {
                    *page += 1;
                }
                render_gallery(&drawer.widgets, &drawer.gallery_urls.borrow(), *page);
            }
        });

    let weak = Rc::downgrade(drawer);
    drawer.widgets.download_button.connect_clicked(move |_| {
        if let Some(drawer) = weak.upgrade() {
            let card = drawer.current_card.borrow().clone();
            let detail = drawer.current_detail.borrow().clone();
            let file = drawer.latest_file.borrow().clone();
            if let (Some(card), Some(file)) = (card, file) {
                let token = *drawer.open_token.borrow();
                start_download(
                    drawer.state.clone(),
                    card,
                    detail,
                    file,
                    drawer.widgets.download_button.clone(),
                    drawer.widgets.download_progress.clone(),
                    drawer.widgets.progress_fraction.clone(),
                    drawer.widgets.progress_title.clone(),
                    drawer.widgets.progress_file_label.clone(),
                    drawer.widgets.progress_pct.clone(),
                    drawer.widgets.download_stack.clone(),
                    drawer.widgets.versions_button.clone(),
                    drawer.open_token.clone(),
                    drawer.current_download_task_id.clone(),
                    drawer.progress_binding_token.clone(),
                    token,
                );
            }
        }
    });

    let weak = Rc::downgrade(drawer);
    drawer.widgets.versions_button.connect_clicked(move |_| {
        if let Some(drawer) = weak.upgrade() {
            let frame = &drawer.widgets.versions_frame;
            frame.set_visible(!frame.is_visible());
        }
    });
}

fn populate_detail(
    widgets: &DetailWidgets,
    state: Rc<AppState>,
    card: ModCard,
    detail: ModDetail,
    latest_file: Rc<RefCell<Option<ModFile>>>,
    current_detail: Rc<RefCell<Option<ModDetail>>>,
    gallery_urls: Rc<RefCell<Vec<String>>>,
    gallery_page: Rc<RefCell<usize>>,
    open_token: Rc<RefCell<u64>>,
    current_download_task_id: Rc<Cell<Option<u64>>>,
    progress_binding_token: Rc<Cell<u64>>,
    token: u64,
) {
    let title = if detail.name.is_empty() {
        card.name.clone()
    } else {
        detail.name.clone()
    };
    widgets.title_label.set_text(&title);
    widgets.name_label.set_text(&title);

    let author = detail
        .submitter
        .as_ref()
        .map(|submitter| submitter.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| card.author.clone());
    widgets.author_label.set_text(&format!("by {author}"));

    let root_category = detail
        .root_category
        .as_ref()
        .map(|cat| cat.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(card.category.as_str());
    let subcategory = detail
        .category
        .as_ref()
        .map(|cat| cat.name.as_str())
        .or(card.subcategory.as_deref());
    render_tags(&widgets.tags_box, root_category, subcategory, card.is_r18);

    let description = html_to_plain_text(&detail.description);
    if description.trim().is_empty() {
        widgets.description_label.set_text("暂无简介。");
    } else {
        widgets.description_label.set_text(description.trim());
    }

    let mut files = detail.files.clone();
    files.sort_by_key(|file| Reverse(file.date_added));
    *latest_file.borrow_mut() = files.first().cloned();
    *current_detail.borrow_mut() = Some(detail.clone());

    if let Some(file) = files.first() {
        let tasks = state.downloads.snapshot();
        let downloading_task = current_download_task_id
            .get()
            .and_then(|task_id| {
                tasks.iter().find(|task| {
                    task.id == task_id
                        && task.mod_id == card.id
                        && matches!(
                            task.phase,
                            DownloadTaskPhase::Queued
                                | DownloadTaskPhase::Paused
                                | DownloadTaskPhase::Downloading
                                | DownloadTaskPhase::Installing
                        )
                })
            })
            .cloned()
            .or_else(|| find_active_download_task(&tasks, card.id, Some(file.id)));
        let is_installed = is_mod_installed(&state, card.id);
        if let Some(ref task) = downloading_task {
            let binding_revision = if current_download_task_id.get() == Some(task.id) {
                progress_binding_token.get()
            } else {
                set_detail_task_binding(
                    &current_download_task_id,
                    &progress_binding_token,
                    Some(task.id),
                )
            };
            render_detail_task_state(widgets, task);
            widgets.download_file_label.set_text(&file.filename);
            widgets.versions_button.set_sensitive(false);
            watch_detail_task(
                detail_download_controls(widgets),
                state.clone(),
                open_token.clone(),
                token,
                current_download_task_id.clone(),
                progress_binding_token.clone(),
                binding_revision,
                task.id,
            );
        } else if is_installed {
            set_detail_task_binding(&current_download_task_id, &progress_binding_token, None);
            widgets.download_stack.set_visible_child_name("button");
            set_button_title(&widgets.download_button, "已安装");
            widgets.download_button.remove_css_class("suggested-action");
            widgets.download_button.add_css_class("detail-installed");
            widgets.download_button.set_sensitive(false);
            widgets.versions_button.set_sensitive(false);
        } else {
            set_detail_task_binding(&current_download_task_id, &progress_binding_token, None);
            widgets.download_stack.set_visible_child_name("button");
            set_button_title(&widgets.download_button, "下载");
            widgets.download_button.add_css_class("suggested-action");
            widgets.download_button.remove_css_class("detail-installed");
            widgets.download_file_label.set_text(&file.filename);
            widgets.download_button.set_sensitive(true);
            widgets.versions_button.set_sensitive(true);
        }
    } else {
        set_detail_task_binding(&current_download_task_id, &progress_binding_token, None);
        widgets.download_file_label.set_text("暂无可下载文件");
        widgets.download_button.set_sensitive(false);
        widgets.versions_button.set_sensitive(false);
    }
    rebuild_versions_list(
        widgets,
        state,
        card.clone(),
        files,
        latest_file,
        current_detail,
        open_token,
        current_download_task_id,
        progress_binding_token,
        token,
    );

    *gallery_urls.borrow_mut() = detail_gallery_urls(&detail, &card);
    *gallery_page.borrow_mut() = 0;
    render_gallery(widgets, &gallery_urls.borrow(), 0);
    preload_gallery_urls(&gallery_urls.borrow());
}

fn detail_download_controls(widgets: &DetailWidgets) -> DetailDownloadControls {
    DetailDownloadControls {
        download_file_label: widgets.download_file_label.clone(),
        download_button: widgets.download_button.clone(),
        download_progress: widgets.download_progress.clone(),
        progress_fraction: widgets.progress_fraction.clone(),
        progress_title: widgets.progress_title.clone(),
        progress_file_label: widgets.progress_file_label.clone(),
        progress_pct: widgets.progress_pct.clone(),
        download_stack: widgets.download_stack.clone(),
        versions_button: widgets.versions_button.clone(),
    }
}

fn rebuild_versions_list(
    widgets: &DetailWidgets,
    state: Rc<AppState>,
    card: ModCard,
    files: Vec<ModFile>,
    latest_file: Rc<RefCell<Option<ModFile>>>,
    current_detail: Rc<RefCell<Option<ModDetail>>>,
    open_token: Rc<RefCell<u64>>,
    current_download_task_id: Rc<Cell<Option<u64>>>,
    progress_binding_token: Rc<Cell<u64>>,
    token: u64,
) {
    clear_versions_box(&widgets.versions_box);

    let item_count = files.len();

    if files.is_empty() {
        let empty = gtk::Label::builder()
            .label("没有可下载文件")
            .halign(gtk::Align::Start)
            .css_classes(["dim-label"])
            .build();
        widgets.versions_box.append(&empty);
        widgets.versions_button.set_sensitive(false);
        widgets.versions_frame.set_height_request(0);
        widgets
            .versions_frame
            .remove_css_class("detail-versions-panel-overflow");
        return;
    }

    for file in files {
        let button = gtk::Button::builder()
            .css_classes(["detail-file-row"])
            .halign(gtk::Align::Fill)
            .build();

        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();

        let text_col = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(3)
            .hexpand(true)
            .build();

        let title = gtk::Label::builder()
            .label(&file.filename)
            .halign(gtk::Align::Start)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["detail-file-name"])
            .build();
        text_col.append(&title);

        let note_text = html_to_plain_text(&file.description);
        let note = gtk::Label::builder()
            .label(if note_text.trim().is_empty() {
                ""
            } else {
                note_text.trim()
            })
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .visible(!note_text.trim().is_empty())
            .css_classes(["detail-file-note"])
            .build();
        text_col.append(&note);

        row.append(&text_col);

        let meta_col = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .build();
        let time_label = gtk::Label::builder()
            .label(&relative_time(file.date_added))
            .halign(gtk::Align::End)
            .css_classes(["detail-file-time"])
            .build();
        meta_col.append(&time_label);
        meta_col.append(
            &gtk::Image::builder()
                .icon_name("folder-download-symbolic")
                .pixel_size(16)
                .css_classes(["detail-file-icon"])
                .build(),
        );
        row.append(&meta_col);

        button.set_child(Some(&row));

        let frame = widgets.versions_frame.clone();
        let download_file_label = widgets.download_file_label.clone();
        let download_button = widgets.download_button.clone();
        let download_progress = widgets.download_progress.clone();
        let progress_fraction = widgets.progress_fraction.clone();
        let progress_title = widgets.progress_title.clone();
        let progress_file_label = widgets.progress_file_label.clone();
        let progress_pct = widgets.progress_pct.clone();
        let download_stack = widgets.download_stack.clone();
        let versions_button = widgets.versions_button.clone();
        let state = state.clone();
        let card = card.clone();
        let current_detail = current_detail.clone();
        let file_for_click = file.clone();
        let latest_file = latest_file.clone();
        let open_token = open_token.clone();
        let current_download_task_id = current_download_task_id.clone();
        let progress_binding_token = progress_binding_token.clone();
        button.connect_clicked(move |_| {
            frame.set_visible(false);
            *latest_file.borrow_mut() = Some(file_for_click.clone());
            download_file_label.set_text(&file_for_click.filename);
            start_download(
                state.clone(),
                card.clone(),
                current_detail.borrow().clone(),
                file_for_click.clone(),
                download_button.clone(),
                download_progress.clone(),
                progress_fraction.clone(),
                progress_title.clone(),
                progress_file_label.clone(),
                progress_pct.clone(),
                download_stack.clone(),
                versions_button.clone(),
                open_token.clone(),
                current_download_task_id.clone(),
                progress_binding_token.clone(),
                token,
            );
        });

        widgets.versions_box.append(&button);
    }

    let estimated_row_h = 58;
    let margins = 20;
    let needed_h = (item_count * estimated_row_h + margins) as i32;
    let max_h = DETAIL_DRAWER_BODY_WIDTH / 2;
    let h = needed_h.min(max_h);
    widgets.versions_frame.set_height_request(h);
    let overflow = needed_h >= max_h;
    if overflow {
        widgets
            .versions_frame
            .add_css_class("detail-versions-panel-overflow");
    } else {
        widgets
            .versions_frame
            .remove_css_class("detail-versions-panel-overflow");
    }
    if let Some(scrolled) = widgets
        .versions_frame
        .first_child()
        .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
    {
        scrolled.set_vscrollbar_policy(if overflow {
            gtk::PolicyType::Automatic
        } else {
            gtk::PolicyType::Never
        });
    }
}

fn clear_versions_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn render_tags(tags_box: &gtk::Box, category: &str, subcategory: Option<&str>, is_r18: bool) {
    while let Some(child) = tags_box.first_child() {
        tags_box.remove(&child);
    }

    for text in [Some(category), subcategory] {
        let Some(text) = text else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let label = gtk::Label::builder()
            .label(text)
            .css_classes(["detail-tag"])
            .build();
        tags_box.append(&label);
    }

    if is_r18 {
        let label = gtk::Label::builder()
            .label("R-18")
            .css_classes(["detail-tag", "detail-tag-r18"])
            .build();
        tags_box.append(&label);
    }
}

fn initial_gallery_urls(card: &ModCard) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(url) = card.cover_url.as_ref().filter(|url| !url.is_empty()) {
        urls.push(url.clone());
    }
    if let Some(url) = card.thumbnail_url.as_ref().filter(|url| !url.is_empty()) {
        if !urls.iter().any(|item| item == url) {
            urls.push(url.clone());
        }
    }
    urls
}

fn detail_gallery_urls(detail: &ModDetail, card: &ModCard) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(media) = detail.preview_media.as_ref() {
        for image in &media.images {
            let file = if !image.file_530.is_empty() {
                &image.file_530
            } else if !image.file.is_empty() {
                &image.file
            } else if !image.file_220.is_empty() {
                &image.file_220
            } else if !image.file_100.is_empty() {
                &image.file_100
            } else {
                continue;
            };
            urls.push(format!("{}/{}", image.base_url, file));
        }
    }

    if urls.is_empty() {
        urls = initial_gallery_urls(card);
    }
    urls
}

fn render_gallery(widgets: &DetailWidgets, urls: &[String], page: usize) {
    if urls.is_empty() {
        widgets.gallery_empty_label.set_visible(true);
        widgets.gallery_prev_button.set_sensitive(false);
        widgets.gallery_next_button.set_sensitive(false);
        widgets.gallery_counter_label.set_text("0 / 0");
        for tile in &widgets.gallery_tiles {
            tile.set_visible(false);
        }
        return;
    }

    widgets.gallery_empty_label.set_visible(false);
    let total_pages = urls.len();
    let clamped_page = page.min(total_pages.saturating_sub(1));
    let start = clamped_page;

    widgets
        .gallery_counter_label
        .set_text(&format!("{} / {}", clamped_page + 1, total_pages));
    widgets.gallery_prev_button.set_sensitive(clamped_page > 0);
    widgets
        .gallery_next_button
        .set_sensitive(clamped_page + 1 < total_pages);

    for (idx, (tile, picture)) in widgets
        .gallery_tiles
        .iter()
        .zip(widgets.gallery_pictures.iter())
        .enumerate()
    {
        if let Some(url) = urls.get(start + idx) {
            tile.set_visible(true);
            load_remote_picture(picture, url);
        } else {
            tile.set_visible(false);
            picture.set_paintable(Option::<&gtk::gdk::Paintable>::None);
        }
    }
}

fn preload_gallery_urls(urls: &[String]) {
    for url in urls {
        let u = url.clone();
        std::thread::spawn(move || {
            let _ = anime_mod_manager::download_image(&u);
        });
    }
}

fn load_remote_picture(picture: &gtk::Picture, url: &str) {
    picture.set_widget_name(url);
    picture.set_paintable(Option::<&gtk::gdk::Paintable>::None);
    let picture = picture.clone();
    let url_string = url.to_string();
    let url_for_thread = url_string.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let _ = tx.send(download_image(&url_for_thread).unwrap_or_default());
    });

    gtk::glib::timeout_add_local(Duration::from_millis(90), move || match rx.try_recv() {
        Ok(data) if !data.is_empty() => {
            if picture.widget_name() != url_string {
                return gtk::glib::ControlFlow::Break;
            }
            if let Ok(pixbuf) = gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(data)) {
                picture.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
            }
            gtk::glib::ControlFlow::Break
        }
        Ok(_) => gtk::glib::ControlFlow::Break,
        Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => gtk::glib::ControlFlow::Break,
    });
}

fn start_download(
    state: Rc<AppState>,
    card: ModCard,
    detail: Option<ModDetail>,
    file: ModFile,
    download_button: gtk::Button,
    download_progress: gtk::DrawingArea,
    progress_fraction: Rc<Cell<f64>>,
    progress_title: gtk::Label,
    progress_file_label: gtk::Label,
    progress_pct: gtk::Label,
    download_stack: gtk::Stack,
    versions_button: gtk::Button,
    open_token: Rc<RefCell<u64>>,
    current_download_task_id: Rc<Cell<Option<u64>>>,
    progress_binding_token: Rc<Cell<u64>>,
    token: u64,
) {
    sync_detail_action_heights(&download_button, &download_progress, &versions_button);
    download_stack.set_visible_child_name("progress");
    set_detail_progress_widgets(
        &download_progress,
        &progress_fraction,
        &progress_title,
        &progress_file_label,
        &progress_pct,
        DownloadTaskPhase::Queued,
        0,
        &file.filename,
        "等待下载",
    );
    if *open_token.borrow() == token {
        download_button.set_sensitive(false);
        versions_button.set_sensitive(false);
    }
    let task_id = state.downloads.submit_fresh(&state, card, detail, file);
    let binding_revision = set_detail_task_binding(
        &current_download_task_id,
        &progress_binding_token,
        Some(task_id),
    );
    watch_detail_task(
        DetailDownloadControls {
            download_file_label: progress_file_label.clone(),
            download_button: download_button.clone(),
            download_progress: download_progress.clone(),
            progress_fraction: progress_fraction.clone(),
            progress_title: progress_title.clone(),
            progress_file_label: progress_file_label.clone(),
            progress_pct: progress_pct.clone(),
            download_stack: download_stack.clone(),
            versions_button: versions_button.clone(),
        },
        state.clone(),
        open_token.clone(),
        token,
        current_download_task_id.clone(),
        progress_binding_token.clone(),
        binding_revision,
        task_id,
    );
}

fn set_button_title(button: &gtk::Button, title: &str) {
    if let Some(child) = button.child() {
        if let Some(content) = child.downcast_ref::<gtk::Box>() {
            if let Some(first) = content.first_child() {
                if let Some(label) = first.downcast_ref::<gtk::Label>() {
                    label.set_text(title);
                }
            }
        }
    }
}

fn find_active_download_task(
    tasks: &[DownloadTask],
    mod_id: u64,
    preferred_file_id: Option<u64>,
) -> Option<DownloadTask> {
    let is_active = |task: &&DownloadTask| {
        task.mod_id == mod_id
            && matches!(
                task.phase,
                DownloadTaskPhase::Paused
                    | DownloadTaskPhase::Queued
                    | DownloadTaskPhase::Downloading
                    | DownloadTaskPhase::Installing
            )
    };

    if let Some(file_id) = preferred_file_id {
        if let Some(task) = tasks
            .iter()
            .find(|task| is_active(task) && task.file_id == file_id)
            .cloned()
        {
            return Some(task);
        }
    }

    tasks.iter().find(is_active).cloned()
}

fn render_detail_task_state(widgets: &DetailWidgets, task: &DownloadTask) {
    render_detail_task_controls(&detail_download_controls(widgets), task);
}

fn render_detail_task_controls(controls: &DetailDownloadControls, task: &DownloadTask) {
    match task.phase {
        DownloadTaskPhase::Queued => {
            controls.download_stack.set_visible_child_name("progress");
            sync_detail_action_heights(
                &controls.download_button,
                &controls.download_progress,
                &controls.versions_button,
            );
            set_detail_progress_state(
                controls,
                task.phase,
                task.progress,
                &task.file_name,
                &task.status_text,
            );
            controls.download_button.set_sensitive(false);
            controls.versions_button.set_sensitive(false);
        }
        DownloadTaskPhase::Paused => {
            controls.download_stack.set_visible_child_name("button");
            set_button_title(&controls.download_button, "继续下载");
            controls.download_file_label.set_text(&task.file_name);
            controls
                .download_button
                .remove_css_class("detail-installed");
            controls.download_button.add_css_class("suggested-action");
            controls.download_button.set_sensitive(true);
            controls.versions_button.set_sensitive(true);
        }
        DownloadTaskPhase::Downloading | DownloadTaskPhase::Installing => {
            controls.download_stack.set_visible_child_name("progress");
            sync_detail_action_heights(
                &controls.download_button,
                &controls.download_progress,
                &controls.versions_button,
            );
            set_detail_progress_state(
                controls,
                task.phase,
                task.progress,
                &task.file_name,
                &task.status_text,
            );
            controls.download_button.set_sensitive(false);
            controls.versions_button.set_sensitive(false);
        }
        DownloadTaskPhase::Completed => {
            controls.download_stack.set_visible_child_name("button");
            set_button_title(&controls.download_button, "已安装");
            controls.download_file_label.set_text(&task.file_name);
            controls
                .download_button
                .remove_css_class("suggested-action");
            controls.download_button.add_css_class("detail-installed");
            controls.download_button.set_sensitive(false);
            controls.versions_button.set_sensitive(false);
        }
        DownloadTaskPhase::Failed => {
            controls.download_stack.set_visible_child_name("button");
            set_button_title(&controls.download_button, "下载");
            controls.download_file_label.set_text(&task.file_name);
            controls
                .download_button
                .remove_css_class("detail-installed");
            controls.download_button.add_css_class("suggested-action");
            controls.download_button.set_sensitive(true);
            controls.versions_button.set_sensitive(true);
        }
    }
}

fn set_detail_task_binding(
    current_download_task_id: &Rc<Cell<Option<u64>>>,
    progress_binding_token: &Rc<Cell<u64>>,
    task_id: Option<u64>,
) -> u64 {
    let next = progress_binding_token.get().wrapping_add(1);
    progress_binding_token.set(next);
    current_download_task_id.set(task_id);
    next
}

fn is_bound_detail_task(
    open_token: &Rc<RefCell<u64>>,
    expected_open_token: u64,
    current_download_task_id: &Rc<Cell<Option<u64>>>,
    progress_binding_token: &Rc<Cell<u64>>,
    expected_binding_revision: u64,
    task_id: u64,
) -> bool {
    *open_token.borrow() == expected_open_token
        && progress_binding_token.get() == expected_binding_revision
        && current_download_task_id.get() == Some(task_id)
}

fn watch_detail_task(
    controls: DetailDownloadControls,
    state: Rc<AppState>,
    open_token: Rc<RefCell<u64>>,
    expected_open_token: u64,
    current_download_task_id: Rc<Cell<Option<u64>>>,
    progress_binding_token: Rc<Cell<u64>>,
    expected_binding_revision: u64,
    task_id: u64,
) {
    gtk::glib::timeout_add_local(Duration::from_millis(300), move || {
        if !is_bound_detail_task(
            &open_token,
            expected_open_token,
            &current_download_task_id,
            &progress_binding_token,
            expected_binding_revision,
            task_id,
        ) {
            return gtk::glib::ControlFlow::Break;
        }

        let tasks = state.downloads.snapshot();
        if let Some(task) = tasks.iter().find(|task| task.id == task_id) {
            render_detail_task_controls(&controls, task);
            if matches!(
                task.phase,
                DownloadTaskPhase::Completed | DownloadTaskPhase::Failed
            ) {
                return gtk::glib::ControlFlow::Break;
            }
            gtk::glib::ControlFlow::Continue
        } else {
            gtk::glib::ControlFlow::Break
        }
    });
}

fn set_detail_progress_state(
    controls: &DetailDownloadControls,
    phase: DownloadTaskPhase,
    progress: u8,
    file_name: &str,
    status_text: &str,
) {
    sync_detail_action_heights(
        &controls.download_button,
        &controls.download_progress,
        &controls.versions_button,
    );
    set_detail_progress_widgets(
        &controls.download_progress,
        &controls.progress_fraction,
        &controls.progress_title,
        &controls.progress_file_label,
        &controls.progress_pct,
        phase,
        progress,
        file_name,
        status_text,
    );
}

fn set_detail_progress_widgets(
    progress_area: &gtk::DrawingArea,
    progress_fraction: &Rc<Cell<f64>>,
    progress_title: &gtk::Label,
    progress_file_label: &gtk::Label,
    progress_pct: &gtk::Label,
    phase: DownloadTaskPhase,
    progress: u8,
    file_name: &str,
    status_text: &str,
) {
    progress_fraction.set((progress as f64 / 100.0).clamp(0.0, 1.0));
    progress_area.queue_draw();
    progress_title.set_text(detail_progress_title(phase, status_text));
    progress_file_label.set_text(file_name);
    progress_pct.set_text(&format!("{}%", progress));
}

fn sync_detail_action_heights(
    download_button: &gtk::Button,
    download_progress: &gtk::DrawingArea,
    versions_button: &gtk::Button,
) {
    let (_, natural, _, _) = download_button.measure(gtk::Orientation::Vertical, -1);
    let target = natural.max(38);
    download_progress.set_height_request(target);
    versions_button.set_height_request(target);
}

fn detail_progress_title(phase: DownloadTaskPhase, status_text: &str) -> &str {
    match phase {
        DownloadTaskPhase::Queued => "正在等待",
        DownloadTaskPhase::Paused => "已暂停",
        DownloadTaskPhase::Downloading if status_text == "复用已下载文件" => "校验文件",
        DownloadTaskPhase::Downloading => "下载中",
        DownloadTaskPhase::Installing => "正在安装",
        DownloadTaskPhase::Completed => "已完成",
        DownloadTaskPhase::Failed => "下载失败",
    }
}

fn is_mod_installed(state: &Rc<AppState>, mod_id: u64) -> bool {
    state
        .manager
        .get_record(mod_id)
        .ok()
        .flatten()
        .is_some_and(|record| record.is_installed() && !record.has_active_download())
}

fn draw_detail_progress_bar(cr: &gtk::cairo::Context, width: i32, height: i32, fraction: f64) {
    if width <= 0 || height <= 0 {
        return;
    }

    let width = width as f64;
    let height = height as f64;
    let fraction = fraction.clamp(0.0, 1.0);

    cr.set_antialias(gtk::cairo::Antialias::Best);

    rounded_rect_path(cr, 0.0, 0.0, width, height, height / 2.0);
    cr.set_source_rgb(0.84, 0.87, 0.91);
    let _ = cr.fill();

    let fill_width = (width * fraction).clamp(0.0, width);
    if fill_width > 0.0 {
        rounded_rect_path(
            cr,
            0.0,
            0.0,
            fill_width,
            height,
            (height / 2.0).min(fill_width / 2.0),
        );
        cr.set_source_rgb(0.21, 0.52, 0.89);
        let _ = cr.fill();
    }
}

fn rounded_rect_path(
    cr: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let right = x + width;
    let bottom = y + height;

    cr.new_sub_path();
    cr.arc(
        right - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        right - radius,
        bottom - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        bottom - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    cr.close_path();
}

fn relative_time(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "时间未知".to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let delta = (now - timestamp).max(0);

    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;

    if delta >= YEAR {
        format!("{}年前", delta / YEAR)
    } else if delta >= MONTH {
        format!("{}个月前", delta / MONTH)
    } else if delta >= DAY {
        format!("{}天前", delta / DAY)
    } else if delta >= HOUR {
        format!("{}小时前", delta / HOUR)
    } else if delta >= MINUTE {
        format!("{}分钟前", delta / MINUTE)
    } else {
        "刚刚".to_string()
    }
}

fn html_to_plain_text(input: &str) -> String {
    let with_breaks = input
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("<p>", "");

    let mut stripped = String::with_capacity(with_breaks.len());
    let mut inside_tag = false;
    for ch in with_breaks.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => stripped.push(ch),
            _ => {}
        }
    }

    stripped
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn gallery_arrow_button(icon_name: &str, halign: gtk::Align, margin: i32) -> gtk::Button {
    let builder = gtk::Button::builder()
        .child(
            &gtk::Image::builder()
                .icon_name(icon_name)
                .pixel_size(18)
                .build(),
        )
        .css_classes(["detail-gallery-arrow"])
        .halign(halign)
        .valign(gtk::Align::Center);

    match halign {
        gtk::Align::Start => builder.margin_start(margin).build(),
        gtk::Align::End => builder.margin_end(margin).build(),
        _ => builder.build(),
    }
}
