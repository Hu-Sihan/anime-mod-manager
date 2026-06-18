use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use adw::prelude::*;
use gtk;

use super::card_frame::CardFrame;
use crate::perf;
use anime_mod_manager::ModCard;

const MAX_CARD_TEXTURE_WIDTH: u32 = 480;
const MAX_CARD_TEXTURE_HEIGHT: u32 = 320;
const IMAGE_DECODE_WORKER_CAP: usize = 8;
const CARD_IMAGE_FETCH_POLL_MS: u64 = 16;

pub struct ModCardWidget {
    container: gtk::Box,
    active_frame: CardFrame,
    waiting_placeholder: gtk::Box,
}

struct DecodedImage {
    width: i32,
    height: i32,
    rowstride: i32,
    pixels: Vec<u8>,
}

struct DecodeJob {
    bytes: Vec<u8>,
    grayscale_cover: bool,
    tx: std::sync::mpsc::Sender<Option<DecodedImage>>,
}

static IMAGE_DECODE_WORKERS: LazyLock<std::sync::mpsc::Sender<DecodeJob>> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::channel::<DecodeJob>();
    let rx = Arc::new(Mutex::new(rx));
    let worker_count = std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).max(1))
        .unwrap_or(2)
        .min(IMAGE_DECODE_WORKER_CAP);

    for _ in 0..worker_count {
        let rx = rx.clone();
        std::thread::spawn(move || loop {
            let job = {
                let guard = match rx.lock() {
                    Ok(guard) => guard,
                    Err(_) => break,
                };
                match guard.recv() {
                    Ok(job) => job,
                    Err(_) => break,
                }
            };
            let _ = job
                .tx
                .send(decode_image_bytes(job.bytes, job.grayscale_cover));
        });
    }

    tx
});

impl ModCardWidget {
    pub fn new(card: &ModCard) -> Self {
        Self::new_with_options(card, false, None)
    }

    pub fn new_with_cancel(card: &ModCard, cancel: Arc<AtomicBool>) -> Self {
        Self::new_with_options(card, false, Some(cancel))
    }

    pub fn new_with_options(
        card: &ModCard,
        grayscale_cover: bool,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Self {
        // Active: CardFrame (3:2 ratio) wrapping Overlay with picture + gradient text
        let active = gtk::Overlay::builder()
            .css_classes(["mod-card"])
            .overflow(gtk::Overflow::Hidden)
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Start)
            .build();

        let picture = gtk::Picture::builder()
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .content_fit(gtk::ContentFit::Cover)
            .can_shrink(true)
            .build();
        active.set_child(Some(&picture));

        // Gradient background box
        let ov_bg = gtk::Box::builder()
            .css_classes(["card-overlay"])
            .halign(gtk::Align::Fill)
            .valign(gtk::Align::Fill)
            .build();
        active.add_overlay(&ov_bg);

        // Text
        let txt = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::End)
            .vexpand(true)
            .margin_start(10)
            .margin_end(10)
            .margin_bottom(8)
            .build();
        let name = gtk::Label::new(Some(&card.name));
        name.set_css_classes(&["card-title"]);
        name.set_halign(gtk::Align::Start);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(18);
        txt.append(&name);
        let author = gtk::Label::new(Some(&format!("by {}", card.author).to_string()));
        author.set_css_classes(&["card-author"]);
        author.set_halign(gtk::Align::Start);
        txt.append(&author);
        let tags = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["card-tags"])
            .halign(gtk::Align::Start)
            .build();
        if let Some(ref s) = card.subcategory {
            if !s.is_empty() {
                let t = gtk::Label::new(Some(s));
                t.set_css_classes(&["tag-character"]);
                tags.append(&t);
            }
        }
        if card.is_r18 {
            let t = gtk::Label::new(Some("R-18"));
            t.set_css_classes(&["tag-r18"]);
            tags.append(&t);
        }
        txt.append(&tags);
        ov_bg.append(&txt);

        // Wrap active Overlay in CardFrame to enforce 3:2 aspect ratio
        let card_frame = CardFrame::new(1.5);
        card_frame.set_vexpand(false);
        card_frame.set_valign(gtk::Align::Start);
        card_frame.set_child(Some(&active));

        // Waiting placeholder
        let wait = gtk::Box::builder()
            .css_classes(["mod-card", "placeholder"])
            .visible(true)
            .vexpand(false)
            .valign(gtk::Align::Start)
            .build();
        let sp = gtk::DrawingArea::new();
        sp.set_content_width(210);
        sp.set_content_height(140);
        wait.append(&sp);

        // Root
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(false)
            .valign(gtk::Align::Start)
            .build();
        root.append(&card_frame);
        root.append(&wait);

        // Download cover image (cached via download_image)
        let local_cover_path = card.local_cover_path.clone();
        let img_url = card
            .thumbnail_url
            .clone()
            .or_else(|| card.cover_url.clone());
        if let Some(path) = local_cover_path
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.exists())
        {
            let pic = picture.clone();
            let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
            std::thread::spawn(move || {
                let _ = tx.send(std::fs::read(path).unwrap_or_default());
            });
            let rx = Rc::new(RefCell::new(rx));
            gtk::glib::timeout_add_local(
                std::time::Duration::from_millis(CARD_IMAGE_FETCH_POLL_MS),
                move || match rx.borrow_mut().try_recv() {
                    Ok(data) if !data.is_empty() => {
                        apply_image_bytes_async(&pic, data, grayscale_cover);
                        gtk::glib::ControlFlow::Break
                    }
                    Ok(_) => gtk::glib::ControlFlow::Break,
                    Err(_) => gtk::glib::ControlFlow::Continue,
                },
            );
        } else if let Some(url) = img_url {
            let pic = picture.clone();
            if let Some(cached) = anime_mod_manager::img_cache::IMG_CACHE.get(&url) {
                apply_image_bytes_async(&pic, cached, grayscale_cover);
            } else {
                let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
                let u = url.clone();
                let cancel = cancel.clone();
                std::thread::spawn(move || {
                    // Small delay so rapid page flips can cancel before download starts
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    if cancel.as_ref().map_or(false, |c| c.load(Ordering::Relaxed)) {
                        let _ = tx.send(Vec::new());
                        return;
                    }
                    let data = anime_mod_manager::download_image(&u).unwrap_or_default();
                    if cancel.as_ref().map_or(false, |c| c.load(Ordering::Relaxed)) {
                        let _ = tx.send(Vec::new());
                        return;
                    }
                    let _ = tx.send(data);
                });
                let rx = Rc::new(RefCell::new(rx));
                gtk::glib::timeout_add_local(
                    std::time::Duration::from_millis(CARD_IMAGE_FETCH_POLL_MS),
                    move || match rx.borrow_mut().try_recv() {
                        Ok(data) if !data.is_empty() => {
                            apply_image_bytes_async(&pic, data, grayscale_cover);
                            gtk::glib::ControlFlow::Break
                        }
                        Ok(_) => gtk::glib::ControlFlow::Break,
                        Err(_) => gtk::glib::ControlFlow::Continue,
                    },
                );
            }
        }

        Self {
            container: root,
            active_frame: card_frame,
            waiting_placeholder: wait,
        }
    }

    pub fn activate(&self) {
        self.waiting_placeholder.set_visible(false);
        self.active_frame.set_visible(true);
    }
    #[allow(dead_code)]
    pub fn deactivate(&self) {
        self.active_frame.set_visible(false);
        self.waiting_placeholder.set_visible(true);
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }
}

fn apply_image_bytes_async(picture: &gtk::Picture, bytes: Vec<u8>, grayscale_cover: bool) {
    let picture = picture.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Option<DecodedImage>>();
    let _ = IMAGE_DECODE_WORKERS.send(DecodeJob {
        bytes,
        grayscale_cover,
        tx,
    });
    let rx = Rc::new(RefCell::new(rx));
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        match rx.borrow_mut().try_recv() {
            Ok(Some(decoded)) => {
                set_picture_from_decoded(&picture, decoded);
                gtk::glib::ControlFlow::Break
            }
            Ok(None) => gtk::glib::ControlFlow::Break,
            Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => gtk::glib::ControlFlow::Break,
        }
    });
}

fn set_picture_from_decoded(picture: &gtk::Picture, decoded: DecodedImage) {
    let label = format!(
        "ModCard::set_picture_from_decoded {}x{}",
        decoded.width, decoded.height
    );
    let _perf = perf::ScopeTimer::with_threshold(label, 4);
    let bytes = gtk::glib::Bytes::from_owned(decoded.pixels);
    let texture = gtk::gdk::MemoryTexture::new(
        decoded.width,
        decoded.height,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        decoded.rowstride as usize,
    );
    picture.set_paintable(Some(&texture));
}

fn decode_image_bytes(bytes: Vec<u8>, grayscale_cover: bool) -> Option<DecodedImage> {
    let _perf = perf::ScopeTimer::with_threshold("ModCard::decode_image_bytes", 6);
    let image = image::load_from_memory(&bytes).ok()?;
    let rgba = downscale_for_card(image.to_rgba8());
    let (width, height) = rgba.dimensions();
    let mut pixels = rgba.into_raw();
    if grayscale_cover {
        grayscale_rgba_in_place(&mut pixels);
    }
    Some(DecodedImage {
        width: width as i32,
        height: height as i32,
        rowstride: width as i32 * 4,
        pixels,
    })
}

fn downscale_for_card(rgba: image::RgbaImage) -> image::RgbaImage {
    let (width, height) = rgba.dimensions();
    let scale = (width as f64 / MAX_CARD_TEXTURE_WIDTH as f64)
        .max(height as f64 / MAX_CARD_TEXTURE_HEIGHT as f64);
    if scale <= 1.0 {
        return rgba;
    }

    let target_width = ((width as f64 / scale).round() as u32).max(1);
    let target_height = ((height as f64 / scale).round() as u32).max(1);
    image::imageops::resize(
        &rgba,
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    )
}

fn grayscale_rgba_in_place(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let gray =
            ((chunk[0] as u32 * 54 + chunk[1] as u32 * 183 + chunk[2] as u32 * 19) / 256) as u8;
        chunk[0] = gray;
        chunk[1] = gray;
        chunk[2] = gray;
    }
}
