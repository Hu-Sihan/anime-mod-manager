use crate::tr;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use adw::prelude::*;
use gtk;
use gtk::gdk::prelude::GdkCairoContextExt;

use super::download_task::DownloadTaskStatusCode;
use super::{AppState, DownloadTask, DownloadTaskPhase};

const DOWNLOAD_CARD_WIDTH: i32 = 860;
const DOWNLOAD_CARD_HEIGHT: i32 = 56;

pub struct DownloadPage {
    pub container: gtk::Box,
    content: gtk::Box,
    empty_label: gtk::Label,
    state: Rc<AppState>,
    cards: RefCell<HashMap<u64, DownloadCardView>>,
}

impl DownloadPage {
    pub fn new(state: Rc<AppState>) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        let title = gtk::Label::builder()
            .label(tr!("download.title"))
            .css_classes(["title-3"])
            .halign(gtk::Align::Start)
            .hexpand(true)
            .margin_start(12)
            .margin_top(12)
            .margin_bottom(8)
            .build();
        container.append(&title);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Start)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(12)
            .build();
        scrolled.set_propagate_natural_height(false);
        let empty_label = gtk::Label::builder()
            .label(tr!("download.empty"))
            .css_classes(["caption", "dim-label"])
            .halign(gtk::Align::Start)
            .build();
        scrolled.set_child(Some(&content));
        container.append(&scrolled);

        let this = Self {
            container,
            content,
            empty_label,
            state,
            cards: RefCell::new(HashMap::new()),
        };
        this.refresh();
        this
    }

    pub fn refresh(&self) {
        let tasks = self.state.downloads.snapshot();
        let active_ids: HashSet<u64> = tasks.iter().map(|task| task.id).collect();
        let mut cards = self.cards.borrow_mut();
        cards.retain(|task_id, _| active_ids.contains(task_id));

        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }

        if tasks.is_empty() {
            self.content.append(&self.empty_label);
            return;
        }

        for task in tasks.iter().rev() {
            let card = cards
                .entry(task.id)
                .or_insert_with(|| DownloadCardView::new(self.state.clone()));
            card.update(task);
            self.content.append(card.widget());
        }
    }

    pub fn ensure_preview_loaded(&self) {
        self.refresh();
    }
}

#[derive(Clone)]
struct DownloadCardImages {
    _grayscale_bytes: gtk::glib::Bytes,
    _color_bytes: gtk::glib::Bytes,
    grayscale: gdk_pixbuf::Pixbuf,
    color: gdk_pixbuf::Pixbuf,
}

struct PreparedDownloadCardImages {
    width: i32,
    height: i32,
    rowstride: i32,
    grayscale_pixels: Vec<u8>,
    color_pixels: Vec<u8>,
}

struct DownloadCardView {
    root: gtk::Box,
    background: gtk::DrawingArea,
    wave: gtk::DrawingArea,
    name: gtk::Label,
    version: gtk::Label,
    status_stack: gtk::Stack,
    percent: gtk::Label,
    paused_icon: gtk::Image,
    task_id: Rc<Cell<u64>>,
    progress: Rc<Cell<u8>>,
    phase: Rc<Cell<DownloadTaskPhase>>,
    images: Rc<RefCell<Option<DownloadCardImages>>>,
    image_source: Rc<RefCell<Option<String>>>,
}

impl DownloadCardView {
    fn new(state: Rc<AppState>) -> Self {
        let shell = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["download-queue-shell"])
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Start)
            .build();

        let card = gtk::Overlay::builder()
            .hexpand(true)
            .vexpand(false)
            .height_request(DOWNLOAD_CARD_HEIGHT)
            .valign(gtk::Align::Start)
            .overflow(gtk::Overflow::Hidden)
            .css_classes(["download-queue-card"])
            .build();
        card.set_size_request(DOWNLOAD_CARD_WIDTH, DOWNLOAD_CARD_HEIGHT);
        card.set_can_target(true);

        let progress = Rc::new(Cell::new(0u8));
        let phase = Rc::new(Cell::new(DownloadTaskPhase::Queued));
        let task_id = Rc::new(Cell::new(0u64));
        let images = Rc::new(RefCell::new(None::<DownloadCardImages>));
        let image_source = Rc::new(RefCell::new(None::<String>));

        let images_for_draw = images.clone();
        let progress_for_draw = progress.clone();
        let background = gtk::DrawingArea::builder()
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["download-queue-picture"])
            .build();
        background.set_can_target(false);
        background.set_draw_func(move |_, cr, width, height| {
            let images_ref = images_for_draw.borrow();
            let Some(images) = images_ref.as_ref() else {
                return;
            };

            draw_cover_pixbuf(cr, &images.grayscale, width, height);

            let progress_x = (width as f64) * (progress_for_draw.get() as f64 / 100.0);
            let _ = cr.save();
            cr.rectangle(0.0, 0.0, progress_x, height as f64);
            cr.clip();
            draw_cover_pixbuf(cr, &images.color, width, height);
            let _ = cr.restore();
        });
        card.set_child(Some(&background));

        let mask = gtk::Box::builder()
            .css_classes(["download-queue-mask"])
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .build();
        mask.set_can_target(false);
        card.add_overlay(&mask);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .margin_start(18)
            .margin_end(18)
            .margin_top(7)
            .margin_bottom(7)
            .build();
        content.set_can_target(false);

        let left = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();

        let name = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .css_classes(["download-queue-name"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        left.append(&name);

        let version = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .css_classes(["download-queue-version"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        left.append(&version);
        content.append(&left);

        let status_stack = gtk::Stack::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::Center)
            .width_request(112)
            .build();
        status_stack.set_can_target(false);
        let percent = gtk::Label::builder()
            .css_classes(["download-queue-percent"])
            .valign(gtk::Align::Center)
            .build();
        percent.set_can_target(false);
        let paused_icon = gtk::Image::builder()
            .icon_name("media-playback-pause-symbolic")
            .pixel_size(34)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .css_classes(["download-queue-status-icon"])
            .build();
        paused_icon.set_can_target(false);
        status_stack.add_named(&percent, Some("text"));
        status_stack.add_named(&paused_icon, Some("paused"));
        status_stack.set_visible_child_name("text");
        content.append(&status_stack);
        card.add_overlay(&content);

        let wave = build_progress_wave(progress.clone(), phase.clone());
        card.add_overlay(&wave);

        let click = gtk::GestureClick::builder().button(0).build();
        let click_state = state.clone();
        let click_task_id = task_id.clone();
        click.connect_pressed(move |_, _, _, _| {
            let current_task_id = click_task_id.get();
            if current_task_id == 0 {
                return;
            }
            let current_phase = click_state
                .downloads
                .snapshot()
                .into_iter()
                .find(|task| task.id == current_task_id)
                .map(|task| task.phase);
            match current_phase {
                Some(DownloadTaskPhase::Paused) => {
                    click_state
                        .downloads
                        .start_task(&click_state, current_task_id);
                }
                Some(DownloadTaskPhase::Queued)
                | Some(DownloadTaskPhase::Downloading)
                | Some(DownloadTaskPhase::Installing) => {
                    click_state
                        .downloads
                        .pause_task(&click_state, current_task_id);
                }
                Some(DownloadTaskPhase::Failed) => {
                    click_state
                        .downloads
                        .restart_failed_task(&click_state, current_task_id);
                }
                Some(DownloadTaskPhase::Completed) | None => {}
            }
        });
        card.add_controller(click);

        shell.append(&card);

        Self {
            root: shell,
            background,
            wave,
            name,
            version,
            status_stack,
            percent,
            paused_icon,
            task_id,
            progress,
            phase,
            images,
            image_source,
        }
    }

    fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn update(&self, task: &DownloadTask) {
        self.task_id.set(task.id);
        self.name.set_text(&task.mod_name);
        let detail_text = if task.status_text.is_empty() {
            task.file_name.clone()
        } else {
            format!("{} · {}", task.file_name, task.status_text).to_string()
        };
        self.version.set_text(&detail_text);
        self.percent
            .set_text(&percent_text(task.phase, task.status_code, task.progress));
        if matches!(task.phase, DownloadTaskPhase::Paused)
            && !matches!(task.status_code, DownloadTaskStatusCode::Removed)
        {
            self.status_stack.set_visible_child_name("paused");
            self.paused_icon.set_tooltip_text(Some(&*tr!("download.paused_hint")));
        } else {
            self.status_stack.set_visible_child_name("text");
            self.paused_icon.set_tooltip_text(None);
        }

        if self.progress.get() != task.progress {
            self.progress.set(task.progress);
            self.background.queue_draw();
            self.wave.queue_draw();
        }
        if self.phase.get() != task.phase {
            self.phase.set(task.phase);
            self.wave.queue_draw();
        }

        if *self.image_source.borrow() != task.image_url {
            *self.image_source.borrow_mut() = task.image_url.clone();
            *self.images.borrow_mut() = None;
            self.background.queue_draw();
            if let Some(source) = task.image_url.as_deref() {
                bind_download_card_images(
                    source,
                    &self.background,
                    self.images.clone(),
                    self.image_source.clone(),
                );
            }
        }
    }
}

fn build_progress_wave(
    progress: Rc<Cell<u8>>,
    phase: Rc<Cell<DownloadTaskPhase>>,
) -> gtk::DrawingArea {
    let wave = gtk::DrawingArea::builder()
        .halign(gtk::Align::Fill)
        .valign(gtk::Align::Fill)
        .hexpand(true)
        .vexpand(true)
        .css_classes(["download-queue-wave"])
        .build();
    wave.set_can_target(false);

    let started_at = Instant::now();
    let phase_for_draw = phase.clone();
    wave.set_draw_func(move |_, cr, width, height| {
        let wave_phase = match phase_for_draw.get() {
            DownloadTaskPhase::Downloading | DownloadTaskPhase::Installing => {
                started_at.elapsed().as_secs_f64() * 4.8
            }
            DownloadTaskPhase::Paused => 0.0,
            _ => return,
        };
        let x = ((width as f64) * (progress.get() as f64 / 100.0)).clamp(12.0, width as f64 - 12.0);
        draw_wave_stroke(cr, x, height as f64, 9.0, 18.0, wave_phase, 14.0, 0.10);
        draw_wave_stroke(cr, x, height as f64, 7.0, 18.0, wave_phase, 8.0, 0.18);
        draw_wave_stroke(cr, x, height as f64, 5.5, 18.0, wave_phase, 4.0, 0.34);
        draw_wave_stroke(cr, x, height as f64, 4.0, 18.0, wave_phase, 2.2, 0.95);
    });
    let phase_for_tick = phase.clone();
    wave.add_tick_callback(move |area, _| {
        if matches!(
            phase_for_tick.get(),
            DownloadTaskPhase::Downloading | DownloadTaskPhase::Installing
        ) {
            area.queue_draw();
        }
        gtk::glib::ControlFlow::Continue
    });

    wave
}

fn draw_wave_stroke(
    cr: &gtk::cairo::Context,
    center_x: f64,
    height: f64,
    amplitude: f64,
    wavelength: f64,
    phase: f64,
    line_width: f64,
    alpha: f64,
) {
    cr.new_path();
    let mut y = 0.0;
    while y <= height + 1.0 {
        let x = center_x + amplitude * ((y / wavelength) + phase).sin();
        if y == 0.0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
        y += 3.0;
    }
    cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
    cr.set_line_width(line_width);
    let _ = cr.stroke();
}

fn bind_download_card_images(
    source: &str,
    background: &gtk::DrawingArea,
    images: Rc<RefCell<Option<DownloadCardImages>>>,
    current_source: Rc<RefCell<Option<String>>>,
) {
    let source = source.to_string();
    let source_for_worker = source.clone();

    let background = background.clone();
    let images_for_timeout = images.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Option<PreparedDownloadCardImages>>();
    std::thread::spawn(move || {
        let data = if Path::new(&source_for_worker).exists() {
            std::fs::read(&source_for_worker).ok()
        } else {
            anime_mod_manager::download_image(&source_for_worker).ok()
        };
        let prepared = data.and_then(prepare_download_card_images_from_bytes);
        let _ = tx.send(prepared);
    });
    let rx = Rc::new(RefCell::new(rx));
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        match rx.borrow_mut().try_recv() {
            Ok(Some(prepared)) => {
                if current_source.borrow().as_deref() != Some(source.as_str()) {
                    return gtk::glib::ControlFlow::Break;
                }
                apply_prepared_download_card_images(prepared, &background, &images_for_timeout);
                gtk::glib::ControlFlow::Break
            }
            Ok(None) => gtk::glib::ControlFlow::Break,
            Err(_) => gtk::glib::ControlFlow::Continue,
        }
    });
}

fn percent_text(
    phase: DownloadTaskPhase,
    status_code: DownloadTaskStatusCode,
    progress: u8,
) -> String {
    match phase {
        DownloadTaskPhase::Queued => "排队".to_string(),
        DownloadTaskPhase::Paused => match status_code {
            DownloadTaskStatusCode::Removed => "移除".to_string(),
            _ => "暂停".to_string(),
        },
        DownloadTaskPhase::Downloading | DownloadTaskPhase::Installing => {
            format!("{}%", progress).to_string()
        }
        DownloadTaskPhase::Completed => "完成".to_string(),
        DownloadTaskPhase::Failed => "失败".to_string(),
    }
}

fn apply_prepared_download_card_images(
    prepared: PreparedDownloadCardImages,
    background: &gtk::DrawingArea,
    images: &Rc<RefCell<Option<DownloadCardImages>>>,
) {
    let grayscale_bytes = gtk::glib::Bytes::from_owned(prepared.grayscale_pixels);
    let color_bytes = gtk::glib::Bytes::from_owned(prepared.color_pixels);
    let grayscale = gdk_pixbuf::Pixbuf::from_bytes(
        &grayscale_bytes,
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        prepared.width,
        prepared.height,
        prepared.rowstride,
    );
    let color = gdk_pixbuf::Pixbuf::from_bytes(
        &color_bytes,
        gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        prepared.width,
        prepared.height,
        prepared.rowstride,
    );
    *images.borrow_mut() = Some(DownloadCardImages {
        _grayscale_bytes: grayscale_bytes,
        _color_bytes: color_bytes,
        grayscale,
        color,
    });
    background.queue_draw();
}

fn prepare_download_card_images_from_bytes(data: Vec<u8>) -> Option<PreparedDownloadCardImages> {
    let image = image::load_from_memory(&data).ok()?;
    let prepared = resize_to_cover_center(
        image.to_rgba8(),
        DOWNLOAD_CARD_WIDTH.max(1) as u32,
        DOWNLOAD_CARD_HEIGHT.max(1) as u32,
    );
    let (width, height) = prepared.dimensions();
    let rowstride = width as i32 * 4;

    let mut color_pixels = prepared.into_raw();
    let mut grayscale_pixels = color_pixels.clone();
    grayscale_rgba_in_place(&mut grayscale_pixels);
    box_blur_rgba_in_place(&mut grayscale_pixels, width as usize, height as usize, 12);
    box_blur_rgba_in_place(&mut color_pixels, width as usize, height as usize, 4);

    Some(PreparedDownloadCardImages {
        width: width as i32,
        height: height as i32,
        rowstride,
        grayscale_pixels,
        color_pixels,
    })
}

fn draw_cover_pixbuf(
    cr: &gtk::cairo::Context,
    pixbuf: &gdk_pixbuf::Pixbuf,
    width: i32,
    height: i32,
) {
    if width <= 0 || height <= 0 || pixbuf.width() <= 0 || pixbuf.height() <= 0 {
        return;
    }

    let target_w = width as f64;
    let target_h = height as f64;
    let source_w = pixbuf.width() as f64;
    let source_h = pixbuf.height() as f64;
    let scale = (target_w / source_w).max(target_h / source_h);
    let draw_w = source_w * scale;
    let draw_h = source_h * scale;
    let offset_x = (target_w - draw_w) * 0.5;
    let offset_y = (target_h - draw_h) * 0.5;

    let _ = cr.save();
    cr.translate(offset_x, offset_y);
    cr.scale(scale, scale);
    cr.set_source_pixbuf(pixbuf, 0.0, 0.0);
    let _ = cr.paint();
    let _ = cr.restore();
}

fn resize_to_cover_center(
    rgba: image::RgbaImage,
    target_width: u32,
    target_height: u32,
) -> image::RgbaImage {
    let source_width = rgba.width().max(1) as f64;
    let source_height = rgba.height().max(1) as f64;
    let scale = (target_width as f64 / source_width).max(target_height as f64 / source_height);
    let scaled_width = ((source_width * scale).round() as u32).max(target_width);
    let scaled_height = ((source_height * scale).round() as u32).max(target_height);
    let resized = image::imageops::resize(
        &rgba,
        scaled_width,
        scaled_height,
        image::imageops::FilterType::Triangle,
    );

    let crop_x = (scaled_width.saturating_sub(target_width)) / 2;
    let crop_y = (scaled_height.saturating_sub(target_height)) / 2;
    image::imageops::crop_imm(&resized, crop_x, crop_y, target_width, target_height).to_image()
}

fn grayscale_rgba_in_place(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let gray =
            ((pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8) as u8;
        pixel[0] = gray;
        pixel[1] = gray;
        pixel[2] = gray;
    }
}

fn box_blur_rgba_in_place(pixels: &mut [u8], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 || height == 0 {
        return;
    }

    let channels = 4usize;
    let mut temp = vec![0u8; pixels.len()];
    let mut prefix = vec![0u32; width.max(height) + 1];

    for y in 0..height {
        let row_start = y * width * channels;
        for channel in 0..channels {
            prefix[0] = 0;
            for x in 0..width {
                prefix[x + 1] = prefix[x] + pixels[row_start + x * channels + channel] as u32;
            }
            for x in 0..width {
                let left = x.saturating_sub(radius);
                let right = (x + radius).min(width - 1);
                let sum = prefix[right + 1] - prefix[left];
                let count = (right + 1 - left) as u32;
                temp[row_start + x * channels + channel] = (sum / count) as u8;
            }
        }
    }

    for x in 0..width {
        for channel in 0..channels {
            prefix[0] = 0;
            for y in 0..height {
                prefix[y + 1] = prefix[y] + temp[(y * width + x) * channels + channel] as u32;
            }
            for y in 0..height {
                let top = y.saturating_sub(radius);
                let bottom = (y + radius).min(height - 1);
                let sum = prefix[bottom + 1] - prefix[top];
                let count = (bottom + 1 - top) as u32;
                pixels[(y * width + x) * channels + channel] = (sum / count) as u8;
            }
        }
    }
}
