use crate::tr;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use adw::prelude::*;
use gtk;

use anime_mod_manager::{cache::MetadataCache, filter_data::FilterData, InstalledMod, ModCard};

use super::catalog_filter::{apply_compact_dropdown_factory, NameQuery};
use super::{AppState, ModCardWidget};

pub struct LocalPage {
    pub container: gtk::Box,
    flow_box: gtk::FlowBox,
    result_stack: gtk::Stack,
    state_label: gtk::Label,
    cat_combo: gtk::DropDown,
    sub_combo: gtk::DropDown,
    search_query: Rc<RefCell<String>>,
    age_filter: Rc<RefCell<u32>>,
    cat_filter: Rc<RefCell<usize>>,
    sub_filter: Rc<RefCell<usize>>,
    enabled_filter: Rc<RefCell<u32>>,
    sub_map: Rc<RefCell<Vec<Vec<String>>>>,
    filter_data: Rc<RefCell<FilterData>>,
    installed_mods: Rc<RefCell<Vec<InstalledMod>>>,
    selection: Rc<RefCell<HashSet<String>>>,
    selection_bar: gtk::Box,
    selection_count_label: gtk::Label,
    state: Rc<AppState>,
}

impl LocalPage {
    pub fn new(state: Rc<AppState>) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text(tr!("local.search_placeholder"))
            .css_classes(["search-bar"])
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .build();
        container.append(&search_entry);

        let filter_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["filter-row"])
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .build();

        filter_row.append(&gtk::Label::new(Some(&*tr!("local.filter_category"))));
        let cat_combo = gtk::DropDown::from_strings(&[&*tr!("local.tag_all")]);
        filter_row.append(&cat_combo);

        filter_row.append(&gtk::Label::new(Some(&*tr!("local.filter_subcategory"))));
        let sub_combo = gtk::DropDown::from_strings(&[&*tr!("local.tag_all")]);
        filter_row.append(&sub_combo);

        filter_row.append(&gtk::Label::new(Some(&*tr!("local.filter_age"))));
        let age_combo = gtk::DropDown::from_strings(&[&*tr!("local.age_mixed"), &*tr!("local.age_sfw"), &*tr!("local.age_nsfw")]);
        age_combo.set_selected(0);
        filter_row.append(&age_combo);

        filter_row.append(&gtk::Label::new(Some(&*tr!("local.filter_status"))));
        let enabled_combo = gtk::DropDown::from_strings(&[&*tr!("local.tag_all"), &*tr!("local.status_enabled"), &*tr!("local.status_disabled")]);
        enabled_combo.set_selected(0);
        filter_row.append(&enabled_combo);

        apply_compact_dropdown_factory(&cat_combo);
        apply_compact_dropdown_factory(&sub_combo);
        apply_compact_dropdown_factory(&age_combo);
        apply_compact_dropdown_factory(&enabled_combo);
        container.append(&filter_row);

        let state_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .margin_start(12)
            .margin_top(2)
            .build();
        container.append(&state_label);

        let flow_box = gtk::FlowBox::builder()
            .max_children_per_line(4)
            .min_children_per_line(4)
            .column_spacing(12)
            .row_spacing(12)
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(86)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .build();
        scrolled.set_child(Some(&flow_box));

        let empty_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .vexpand(true)
            .spacing(12)
            .css_classes(["local-empty"])
            .build();
        empty_box.append(
            &gtk::Image::builder()
                .icon_name("folder-symbolic")
                .pixel_size(64)
                .build(),
        );
        empty_box.append(
            &gtk::Label::builder()
                .label(tr!("local.empty"))
                .css_classes(["title-4", "dim-label"])
                .build(),
        );

        let result_stack = gtk::Stack::new();
        result_stack.add_named(&scrolled, Some("grid"));
        result_stack.add_named(&empty_box, Some("empty"));
        result_stack.set_visible_child_name("empty");

        let overlay = gtk::Overlay::builder().hexpand(true).vexpand(true).build();
        overlay.set_child(Some(&result_stack));

        let selection_bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::End)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .css_classes(["card", "selection-action-bar"])
            .visible(false)
            .build();
        selection_bar.set_can_target(true);

        let selection_count_label = gtk::Label::builder()
            .label(tr!("local.selected_count", 0))
            .css_classes(["selection-count-label"])
            .build();
        selection_bar.append(&selection_count_label);

        let delete_button = gtk::Button::builder()
            .label(tr!("local.delete"))
            .css_classes(["destructive-action"])
            .build();
        selection_bar.append(&delete_button);

        let enable_button = gtk::Button::builder().label(tr!("local.enable")).build();
        selection_bar.append(&enable_button);

        let disable_button = gtk::Button::builder().label(tr!("local.disable")).build();
        selection_bar.append(&disable_button);

        overlay.add_overlay(&selection_bar);
        container.append(&overlay);

        let search_query = Rc::new(RefCell::new(String::new()));
        let age_filter = Rc::new(RefCell::new(0u32));
        let cat_filter = Rc::new(RefCell::new(0usize));
        let sub_filter = Rc::new(RefCell::new(0usize));
        let enabled_filter = Rc::new(RefCell::new(0u32));
        let sub_map = Rc::new(RefCell::new(vec![vec![tr!("local.tag_all")]]));
        let filter_data = Rc::new(RefCell::new(FilterData::build_from_cards(&[])));
        let installed_mods = Rc::new(RefCell::new(Vec::<InstalledMod>::new()));
        let selection = Rc::new(RefCell::new(HashSet::<String>::new()));

        let page = Self {
            container,
            flow_box,
            result_stack,
            state_label,
            cat_combo,
            sub_combo,
            search_query,
            age_filter,
            cat_filter,
            sub_filter,
            enabled_filter,
            sub_map,
            filter_data,
            installed_mods,
            selection,
            selection_bar,
            selection_count_label,
            state,
        };

        {
            let query = page.search_query.clone();
            let page_ref = page.clone_handles();
            search_entry.connect_search_changed(move |entry| {
                *query.borrow_mut() = entry.text().to_string();
                render_local_page(&page_ref);
            });
        }

        {
            let age = page.age_filter.clone();
            let page_ref = page.clone_handles();
            age_combo.connect_selected_notify(move |dropdown| {
                *age.borrow_mut() = dropdown.selected();
                render_local_page(&page_ref);
            });
        }

        {
            let enabled = page.enabled_filter.clone();
            let page_ref = page.clone_handles();
            enabled_combo.connect_selected_notify(move |dropdown| {
                *enabled.borrow_mut() = dropdown.selected();
                render_local_page(&page_ref);
            });
        }

        {
            let cat_filter = page.cat_filter.clone();
            let sub_filter = page.sub_filter.clone();
            let sub_map = page.sub_map.clone();
            let sub_combo = page.sub_combo.clone();
            let page_ref = page.clone_handles();
            page.cat_combo.connect_selected_notify(move |dropdown| {
                *cat_filter.borrow_mut() = dropdown.selected() as usize;
                *sub_filter.borrow_mut() = 0;
                let sub_items = sub_map
                    .borrow()
                    .get(dropdown.selected() as usize)
                    .cloned()
                    .unwrap_or_else(|| vec![tr!("local.tag_all")]);
                let refs: Vec<&str> = sub_items.iter().map(|value| value.as_str()).collect();
                sub_combo.set_model(Some(&gtk::StringList::new(&refs)));
                sub_combo.set_selected(0);
                render_local_page(&page_ref);
            });
        }

        {
            let sub_filter = page.sub_filter.clone();
            let page_ref = page.clone_handles();
            page.sub_combo.connect_selected_notify(move |dropdown| {
                *sub_filter.borrow_mut() = dropdown.selected() as usize;
                render_local_page(&page_ref);
            });
        }

        {
            let page_ref = page.clone_handles();
            let manager = page.state.manager.clone();
            let selection = page.selection.clone();
            delete_button.connect_clicked(move |_| {
                let folders: Vec<String> = selection.borrow().iter().cloned().collect();
                for folder in folders {
                    let _ = manager.uninstall(&folder);
                }
                selection.borrow_mut().clear();
                refresh_local_page(&page_ref);
            });
        }

        {
            let page_ref = page.clone_handles();
            let manager = page.state.manager.clone();
            let selection = page.selection.clone();
            enable_button.connect_clicked(move |_| {
                let folders: Vec<String> = selection.borrow().iter().cloned().collect();
                for folder in folders {
                    let _ = manager.enable_mod(&folder);
                }
                selection.borrow_mut().clear();
                refresh_local_page(&page_ref);
            });
        }

        {
            let page_ref = page.clone_handles();
            let manager = page.state.manager.clone();
            let selection = page.selection.clone();
            disable_button.connect_clicked(move |_| {
                let folders: Vec<String> = selection.borrow().iter().cloned().collect();
                for folder in folders {
                    let _ = manager.disable_mod(&folder);
                }
                selection.borrow_mut().clear();
                refresh_local_page(&page_ref);
            });
        }

        page
    }

    pub fn refresh(&self) {
        let handles = self.clone_handles();
        refresh_local_page(&handles);
    }

    fn clone_handles(&self) -> LocalPageHandles {
        LocalPageHandles {
            flow_box: self.flow_box.clone(),
            result_stack: self.result_stack.clone(),
            state_label: self.state_label.clone(),
            cat_combo: self.cat_combo.clone(),
            sub_combo: self.sub_combo.clone(),
            search_query: self.search_query.clone(),
            age_filter: self.age_filter.clone(),
            cat_filter: self.cat_filter.clone(),
            sub_filter: self.sub_filter.clone(),
            enabled_filter: self.enabled_filter.clone(),
            sub_map: self.sub_map.clone(),
            filter_data: self.filter_data.clone(),
            installed_mods: self.installed_mods.clone(),
            selection: self.selection.clone(),
            selection_bar: self.selection_bar.clone(),
            selection_count_label: self.selection_count_label.clone(),
            state: self.state.clone(),
        }
    }
}

#[derive(Clone)]
struct LocalPageHandles {
    flow_box: gtk::FlowBox,
    result_stack: gtk::Stack,
    state_label: gtk::Label,
    cat_combo: gtk::DropDown,
    sub_combo: gtk::DropDown,
    search_query: Rc<RefCell<String>>,
    age_filter: Rc<RefCell<u32>>,
    cat_filter: Rc<RefCell<usize>>,
    sub_filter: Rc<RefCell<usize>>,
    enabled_filter: Rc<RefCell<u32>>,
    sub_map: Rc<RefCell<Vec<Vec<String>>>>,
    filter_data: Rc<RefCell<FilterData>>,
    installed_mods: Rc<RefCell<Vec<InstalledMod>>>,
    selection: Rc<RefCell<HashSet<String>>>,
    selection_bar: gtk::Box,
    selection_count_label: gtk::Label,
    state: Rc<AppState>,
}

fn refresh_local_page(page: &LocalPageHandles) {
    let mut mods = page.state.manager.list_installed().unwrap_or_default();
    mods.sort_by(|left, right| right.installed_at.cmp(&left.installed_at));
    hydrate_local_mods_from_cache(&mut mods);
    let folders: HashSet<String> = mods.iter().map(|item| item.folder.clone()).collect();
    page.selection
        .borrow_mut()
        .retain(|folder| folders.contains(folder));
    *page.installed_mods.borrow_mut() = mods;

    let cards: Vec<ModCard> = page
        .installed_mods
        .borrow()
        .iter()
        .map(InstalledMod::to_mod_card)
        .collect();
    let filter_data = FilterData::build_from_cards(&cards);
    let mut sub_map = vec![vec![tr!("local.tag_all")]];
    for (index, category) in filter_data.categories.iter().enumerate() {
        let subs = filter_data
            .subcategories
            .get(category)
            .cloned()
            .unwrap_or_else(|| vec![tr!("local.tag_all")]);
        while sub_map.len() <= index {
            sub_map.push(vec![tr!("local.tag_all")]);
        }
        sub_map[index] = subs;
    }
    *page.filter_data.borrow_mut() = filter_data.clone();
    *page.sub_map.borrow_mut() = sub_map;

    let categories: Vec<&str> = filter_data
        .categories
        .iter()
        .map(|value| value.as_str())
        .collect();
    page.cat_combo
        .set_model(Some(&gtk::StringList::new(&categories)));
    let mut current_cat = *page.cat_filter.borrow();
    if current_cat >= filter_data.categories.len() {
        current_cat = 0;
        *page.cat_filter.borrow_mut() = 0;
    }
    page.cat_combo.set_selected(current_cat as u32);

    sync_local_subcategories(page);
    render_local_page(page);
}

fn hydrate_local_mods_from_cache(mods: &mut [InstalledMod]) {
    let cache_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("cache");
    let Some(cache) = MetadataCache::load(&cache_dir) else {
        return;
    };

    for item in mods {
        let needs_fill = item.author.is_empty()
            || item.category.is_empty()
            || item.profile_url.is_empty()
            || item.thumbnail_url.is_none()
            || item.cover_url.is_none();
        if !needs_fill {
            continue;
        }

        let Some(card) = cache.cards.iter().find(|card| card.id == item.mod_id) else {
            continue;
        };
        if item.author.is_empty() {
            item.author = card.author.clone();
        }
        if item.category.is_empty() {
            item.category = card.category.clone();
        }
        if item.subcategory.is_none() {
            item.subcategory = card.subcategory.clone();
        }
        if item.profile_url.is_empty() {
            item.profile_url = card.profile_url.clone();
        }
        if item.thumbnail_url.is_none() {
            item.thumbnail_url = card.thumbnail_url.clone();
        }
        if item.cover_url.is_none() {
            item.cover_url = card.cover_url.clone();
        }
        item.is_r18 |= card.is_r18;
    }
}

fn sync_local_subcategories(page: &LocalPageHandles) {
    let selected_cat = *page.cat_filter.borrow();
    let sub_items = page
        .sub_map
        .borrow()
        .get(selected_cat)
        .cloned()
        .unwrap_or_else(|| vec![tr!("local.tag_all")]);
    let refs: Vec<&str> = sub_items.iter().map(|value| value.as_str()).collect();
    page.sub_combo.set_model(Some(&gtk::StringList::new(&refs)));

    let mut current_sub = *page.sub_filter.borrow();
    if current_sub >= sub_items.len() {
        current_sub = 0;
        *page.sub_filter.borrow_mut() = 0;
    }
    page.sub_combo.set_selected(current_sub as u32);
}

fn render_local_page(page: &LocalPageHandles) {
    while let Some(child) = page.flow_box.first_child() {
        page.flow_box.remove(&child);
    }

    let query = NameQuery::parse(&page.search_query.borrow());
    let filter_data = page.filter_data.borrow().clone();
    let categories = filter_data.categories;
    let selected_cat = *page.cat_filter.borrow();
    let selected_sub = *page.sub_filter.borrow();
    let cat_name = if selected_cat > 0 && selected_cat < categories.len() {
        categories[selected_cat].clone()
    } else {
        String::new()
    };
    let sub_items = page
        .sub_map
        .borrow()
        .get(selected_cat)
        .cloned()
        .unwrap_or_else(|| vec![tr!("local.tag_all")]);
    let sub_name = if selected_sub > 0 && selected_sub < sub_items.len() {
        sub_items[selected_sub].clone()
    } else {
        String::new()
    };

    let filtered: Vec<InstalledMod> = page
        .installed_mods
        .borrow()
        .iter()
        .filter(|item| {
            local_mod_matches(
                item,
                *page.age_filter.borrow(),
                *page.enabled_filter.borrow(),
                &cat_name,
                &sub_name,
                &query,
            )
        })
        .cloned()
        .collect();

    page.state_label.set_text(&*tr!(
        "local.summary",
        page.installed_mods.borrow().len(),
        filtered.len()
    ));
    update_selection_bar(
        &page.selection_bar,
        &page.selection_count_label,
        &page.selection.borrow(),
    );

    if filtered.is_empty() {
        page.result_stack.set_visible_child_name("empty");
        return;
    }

    page.result_stack.set_visible_child_name("grid");

    for item in filtered {
        let child = build_local_card_child(page, item);
        page.flow_box.insert(&child, -1);
    }
}

fn build_local_card_child(page: &LocalPageHandles, item: InstalledMod) -> gtk::FlowBoxChild {
    let mut mod_card = item.to_mod_card();
    mod_card.local_cover_path = resolve_local_cover_path(page, &item);
    let card = ModCardWidget::new_with_options(&mod_card, !item.enabled, None);
    card.activate();

    let shell = gtk::Overlay::builder()
        .hexpand(true)
        .vexpand(false)
        .valign(gtk::Align::Start)
        .css_classes(["local-card-shell"])
        .build();
    shell.set_child(Some(card.widget()));

    let badge_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .margin_start(10)
        .margin_top(10)
        .build();
    badge_row.set_can_target(false);
    if !item.enabled {
        let disabled_tag = gtk::Label::new(Some(&*tr!("local.disabled_tag")));
        disabled_tag.set_css_classes(&["tag-disabled"]);
        badge_row.append(&disabled_tag);
    } else {
        let enabled_tag = gtk::Label::new(Some(&*tr!("local.enabled_tag")));
        enabled_tag.set_css_classes(&["tag-installed"]);
        badge_row.append(&enabled_tag);
    }
    if item.update_available {
        let update_tag = gtk::Label::new(Some(&*tr!("local.update_tag")));
        update_tag.set_css_classes(&["tag-update"]);
        badge_row.append(&update_tag);
    }
    shell.add_overlay(&badge_row);

    let selection_overlay = gtk::Box::builder()
        .css_classes(["local-card-selection"])
        .halign(gtk::Align::Fill)
        .valign(gtk::Align::Fill)
        .visible(page.selection.borrow().contains(&item.folder))
        .build();
    selection_overlay.set_can_target(false);
    shell.add_overlay(&selection_overlay);

    let check_badge = gtk::Image::builder()
        .icon_name("object-select-symbolic")
        .pixel_size(18)
        .css_classes(["local-card-check"])
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .margin_end(12)
        .margin_top(12)
        .visible(page.selection.borrow().contains(&item.folder))
        .build();
    check_badge.set_can_target(false);
    shell.add_overlay(&check_badge);

    {
        let folder = item.folder.clone();
        let selection = page.selection.clone();
        let bar = page.selection_bar.clone();
        let count = page.selection_count_label.clone();
        let selection_overlay = selection_overlay.clone();
        let check_badge = check_badge.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
        gesture.connect_pressed(move |_, _, _, _| {
            let now_selected = {
                let mut selection = selection.borrow_mut();
                if selection.contains(&folder) {
                    selection.remove(&folder);
                    false
                } else {
                    selection.insert(folder.clone());
                    true
                }
            };
            selection_overlay.set_visible(now_selected);
            check_badge.set_visible(now_selected);
            update_selection_bar(&bar, &count, &selection.borrow());
        });
        shell.add_controller(gesture);
    }

    let child = gtk::FlowBoxChild::new();
    child.set_valign(gtk::Align::Start);
    child.set_child(Some(&shell));
    child
}

fn resolve_local_cover_path(page: &LocalPageHandles, item: &InstalledMod) -> Option<String> {
    let relative = item.local_cover_path.as_ref()?;
    let root = page.state.manager.entry_dir(&item.folder, item.enabled);
    let path = root.join(relative);
    if path.exists() {
        Some(path.to_string_lossy().to_string())
    } else {
        None
    }
}

fn local_mod_matches(
    item: &InstalledMod,
    age_filter: u32,
    enabled_filter: u32,
    cat_name: &str,
    sub_name: &str,
    query: &NameQuery,
) -> bool {
    if age_filter == 1 && item.is_r18 {
        return false;
    }
    if age_filter == 2 && !item.is_r18 {
        return false;
    }
    if enabled_filter == 1 && !item.enabled {
        return false;
    }
    if enabled_filter == 2 && item.enabled {
        return false;
    }
    if !cat_name.is_empty() && item.category != cat_name {
        return false;
    }
    if !sub_name.is_empty() && item.subcategory.as_deref() != Some(sub_name) {
        return false;
    }
    query.matches(&item.name)
}

fn update_selection_bar(bar: &gtk::Box, count_label: &gtk::Label, selection: &HashSet<String>) {
    let count = selection.len();
    bar.set_visible(count > 0);
    count_label.set_text(&*tr!("local.selected_count", count));
}
