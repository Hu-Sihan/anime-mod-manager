mod config;
mod i18n;
mod perf;
mod ui;

use adw::prelude::*;
use gtk;

rust_i18n::i18n!("locales");

const APP_ID: &str = "moe.launcher.mod-manager-demo";

use std::cell::RefCell;
thread_local! {
    static LIGHT_TAG_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

pub fn apply_tag_light_theme(light: bool) {
    LIGHT_TAG_PROVIDER.with(|cell| {
        let Some(ref provider) = *cell.borrow() else { return };
        let Some(display) = gtk::gdk::Display::default() else { return };
        if light {
            gtk::style_context_add_provider_for_display(
                &display, provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        } else {
            gtk::style_context_remove_provider_for_display(&display, provider);
        }
    });
}

pub fn init_light_tag_provider(provider: gtk::CssProvider) {
    LIGHT_TAG_PROVIDER.with(|cell| *cell.borrow_mut() = Some(provider));
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    adw::init()?;

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_app| {
        let _perf = perf::ScopeTimer::with_threshold("app.connect_startup.css", 1);
        let css = gtk::CssProvider::new();
        css.load_from_string(include_str!("style.css"));
        let display = &gtk::gdk::Display::default().unwrap();
        gtk::style_context_add_provider_for_display(
            display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        // Store light-mode tag override provider for runtime toggling
        let light_tags = gtk::CssProvider::new();
        light_tags.load_from_string(
            ".tag-installed { background-color: rgba(255,255,255,0.96); color: @success_color; border: 1px solid alpha(@success_color, 0.22); }"
        );
        init_light_tag_provider(light_tags);
    });

    app.connect_activate(|app| {
        let _perf = perf::ScopeTimer::with_threshold("app.connect_activate", 1);
        ui::MainWindow::new(app);
    });

    app.run();

    Ok(())
}
