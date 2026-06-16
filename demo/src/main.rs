mod config;
mod perf;
mod ui;

use adw::prelude::*;
use gtk;

const APP_ID: &str = "moe.launcher.mod-manager-demo";

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
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    app.connect_activate(|app| {
        let _perf = perf::ScopeTimer::with_threshold("app.connect_activate", 1);
        ui::MainWindow::new(app);
    });

    app.run();

    Ok(())
}
