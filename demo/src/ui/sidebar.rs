use crate::tr;
use std::rc::Rc;

use adw::prelude::*;
use gtk;
use gtk::glib;

use super::{AppState, TabPage};

pub struct Sidebar {
    container: gtk::Box,
    buttons: Vec<gtk::ToggleButton>,
    logo: gtk::Image,
    avatar: gtk::Image,
}

impl Sidebar {
    pub fn new(state: Rc<AppState>, stack: &gtk::Stack) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        let logo = gtk::Image::builder()
            .icon_name("applications-games-symbolic")
            .pixel_size(24)
            .margin_top(4)
            .margin_bottom(8)
            .tooltip_text(&*tr!("app.logo_tooltip"))
            .build();
        container.append(&logo);

        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_margin_start(6);
        sep.set_margin_end(6);
        container.append(&sep);

        let btn_local = icon_toggle("folder-symbolic", &*tr!("sidebar.local"));
        let btn_download = icon_toggle("document-save-symbolic", &*tr!("sidebar.download"));
        let btn_browse = icon_toggle("system-search-symbolic", &*tr!("sidebar.browse"));
        let btn_settings = icon_toggle("emblem-system-symbolic", &*tr!("sidebar.settings"));

        btn_browse.set_active(true);

        let s1 = stack.clone();
        let state1 = state.clone();
        btn_local.connect_toggled(glib::clone!(
            #[weak] btn_download, #[weak] btn_browse, #[weak] btn_settings,
            move |btn| {
                if btn.is_active() {
                    btn_download.set_active(false);
                    btn_browse.set_active(false);
                    btn_settings.set_active(false);
                    s1.set_visible_child_name("local");
                    state1.current_tab.replace(TabPage::Local);
                }
            }
        ));

        let s2 = stack.clone();
        let state2 = state.clone();
        btn_download.connect_toggled(glib::clone!(
            #[weak] btn_local, #[weak] btn_browse, #[weak] btn_settings,
            move |btn| {
                if btn.is_active() {
                    btn_local.set_active(false);
                    btn_browse.set_active(false);
                    btn_settings.set_active(false);
                    s2.set_visible_child_name("download");
                    state2.current_tab.replace(TabPage::Download);
                }
            }
        ));

        let s3 = stack.clone();
        let state3 = state.clone();
        btn_browse.connect_toggled(glib::clone!(
            #[weak] btn_local, #[weak] btn_download, #[weak] btn_settings,
            move |btn| {
                if btn.is_active() {
                    btn_local.set_active(false);
                    btn_download.set_active(false);
                    btn_settings.set_active(false);
                    s3.set_visible_child_name("browse");
                    state3.current_tab.replace(TabPage::Browse);
                }
            }
        ));

        let s4 = stack.clone();
        let state4 = state.clone();
        btn_settings.connect_toggled(glib::clone!(
            #[weak] btn_local, #[weak] btn_download, #[weak] btn_browse,
            move |btn| {
                if btn.is_active() {
                    btn_local.set_active(false);
                    btn_download.set_active(false);
                    btn_browse.set_active(false);
                    s4.set_visible_child_name("settings");
                    state4.current_tab.replace(TabPage::Settings);
                }
            }
        ));

        container.append(&btn_local);
        container.append(&btn_download);
        container.append(&btn_browse);

        let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        spacer.set_vexpand(true);
        container.append(&spacer);

        container.append(&btn_settings);

        let bottom_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        bottom_sep.set_margin_start(6);
        bottom_sep.set_margin_end(6);
        bottom_sep.set_margin_bottom(4);
        container.append(&bottom_sep);

        let avatar = gtk::Image::builder()
            .icon_name("avatar-default-symbolic")
            .pixel_size(24)
            .tooltip_text(&*tr!("sidebar.gb_account"))
            .margin_bottom(6)
            .build();
        container.append(&avatar);

        let buttons = vec![btn_local, btn_download, btn_browse, btn_settings];

        // Store widgets in Rc for language-change subscription
        let sidebar = Self { container, buttons, logo, avatar };
        let logo_w = sidebar.logo.clone();
        let btns = sidebar.buttons.clone();
        let av = sidebar.avatar.clone();
        state.subscribe_language_changed(move || {
            let labels: [&str; 4] = [
                &*tr!("sidebar.local"),
                &*tr!("sidebar.download"),
                &*tr!("sidebar.browse"),
                &*tr!("sidebar.settings"),
            ];
            for (btn, label) in btns.iter().zip(labels.iter()) {
                btn.set_tooltip_text(Some(label));
            }
            logo_w.set_tooltip_text(Some(&*tr!("app.logo_tooltip")));
            av.set_tooltip_text(Some(&*tr!("sidebar.gb_account")));
        });

        sidebar
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }
}

fn icon_toggle(icon_name: &str, tooltip: &str) -> gtk::ToggleButton {
    gtk::ToggleButton::builder()
        .child(
            &gtk::Image::builder()
                .icon_name(icon_name)
                .pixel_size(22)
                .build(),
        )
        .tooltip_text(tooltip)
        .css_classes(["sidebar-tab", "flat"])
        .margin_start(4)
        .margin_end(4)
        .build()
}
