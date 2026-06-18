use crate::tr;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use adw::prelude::*;
use gtk;

use super::catalog_filter::{apply_compact_dropdown_factory, NameQuery};
use super::mod_detail_window::ModDetailDrawer;
use super::{AppState, ModCardWidget};
use crate::perf;
use anime_mod_manager::{cache::MetadataCache, filter_data::FilterData, GameBananaClient, ModCard};

const COLS: usize = 4;
const ROWS: usize = 4;
const SLOTS: usize = ROWS * COLS;
const PAGE_STEP: usize = 16;

struct VRow {
    cards: [Option<ModCard>; COLS],
    ui_done: bool,
}

pub struct BrowsePage {
    pub container: gtk::Box,
    content_box: gtk::Box,
    _state_label: gtk::Label,
    cache: Arc<RefCell<Option<MetadataCache>>>,
    vrows: Rc<RefCell<VecDeque<VRow>>>,
    offset: Rc<RefCell<usize>>,
    age_filter: Rc<RefCell<u32>>,
    cat_filter: Rc<RefCell<usize>>,
    sub_filter: Rc<RefCell<usize>>,
    downloaded_filter: Rc<RefCell<u32>>,
    filter_data: Rc<RefCell<Option<FilterData>>>,
    search_query: Rc<RefCell<String>>,
    installed_ids: Rc<RefCell<HashSet<u64>>>,
    state: Rc<AppState>,
    detail_drawer: Rc<ModDetailDrawer>,
    cancel_token: Rc<RefCell<Arc<AtomicBool>>>,
    _keep_alive: Rc<RefCell<Option<gtk::glib::SourceId>>>,
}

impl BrowsePage {
    pub fn new(state: Rc<AppState>) -> Self {
        let _perf = perf::ScopeTimer::with_threshold("BrowsePage::new.inner", 1);
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        let overlay = gtk::Overlay::builder().hexpand(true).vexpand(true).build();
        let page_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        overlay.set_child(Some(&page_content));
        container.append(&overlay);
        let detail_drawer = ModDetailDrawer::new(state.clone());
        overlay.add_overlay(detail_drawer.scrim());
        overlay.add_overlay(detail_drawer.revealer());

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text(tr!("browse.search_placeholder"))
            .css_classes(["search-bar"])
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .build();
        page_content.append(&search_entry);
        let filter_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .css_classes(["filter-row"])
            .margin_start(12)
            .margin_end(12)
            .margin_top(4)
            .build();
        filter_row.append(&gtk::Label::new(Some(&*tr!("browse.filter_category"))));
        let cat_combo =
            gtk::DropDown::from_strings(&[&*tr!("browse.tag_all"), "皮肤", "角色", "武器", "UI", "工具", "其他"]);
        cat_combo.set_selected(0);
        filter_row.append(&cat_combo);
        filter_row.append(&gtk::Label::new(Some(&*tr!("browse.filter_subcategory"))));
        let sub_combo = gtk::DropDown::from_strings(&[&*tr!("browse.tag_all"), ""]);
        sub_combo.set_selected(0);
        filter_row.append(&sub_combo);
        filter_row.append(&gtk::Label::new(Some(&*tr!("browse.filter_age"))));
        let age_combo = gtk::DropDown::from_strings(&[&*tr!("browse.age_mixed"), &*tr!("browse.age_sfw"), &*tr!("browse.age_nsfw")]);
        age_combo.set_selected(0);
        filter_row.append(&age_combo);
        filter_row.append(&gtk::Label::new(Some(&*tr!("browse.filter_downloaded"))));
        let downloaded_combo = gtk::DropDown::from_strings(&[&*tr!("browse.tag_all"), &*tr!("browse.downloaded_yes"), &*tr!("browse.downloaded_no")]);
        downloaded_combo.set_selected(0);
        filter_row.append(&downloaded_combo);
        apply_compact_dropdown_factory(&cat_combo);
        apply_compact_dropdown_factory(&sub_combo);
        apply_compact_dropdown_factory(&age_combo);
        apply_compact_dropdown_factory(&downloaded_combo);
        page_content.append(&filter_row);
        let state_label = gtk::Label::new(None);
        state_label.set_margin_start(12);
        state_label.set_margin_top(2);
        page_content.append(&state_label);
        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .build();
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(2)
            .margin_bottom(12)
            .build();
        for _ in 0..ROWS {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .hexpand(true)
                .build();
            for _ in 0..COLS {
                row.append(ModCardWidget::new(&ModCard::placeholder()).widget());
            }
            content_box.append(&row);
        }
        scrolled.set_child(Some(&content_box));
        let sync_overlay = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .visible(true)
            .build();
        sync_overlay.append(
            &gtk::Spinner::builder()
                .width_request(48)
                .height_request(48)
                .spinning(true)
                .build(),
        );
        let sync_label = gtk::Label::new(Some(&*tr!("browse.sync_checking")));
        sync_label.set_margin_top(12);
        sync_overlay.append(&sync_label);
        let main_stack = gtk::Stack::new();
        main_stack.add_named(&scrolled, Some("grid"));
        main_stack.add_named(&sync_overlay, Some("sync"));
        main_stack.set_visible_child_name("sync");
        page_content.append(&main_stack);
        let btn_prev = gtk::Button::from_icon_name("go-previous-symbolic");
        let page_label = gtk::Label::new(Some("0/0"));
        let btn_next = gtk::Button::from_icon_name("go-next-symbolic");
        let pagination = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::Center)
            .margin_top(8)
            .margin_bottom(4)
            .build();
        pagination.append(&btn_prev);
        pagination.append(&page_label);
        pagination.append(&btn_next);
        page_content.append(&pagination);

        let vrows = Rc::new(RefCell::new(VecDeque::new()));
        let cache = Arc::new(RefCell::new(None::<MetadataCache>));
        let offset = Rc::new(RefCell::new(0usize));
        let age_filter = Rc::new(RefCell::new(0u32));
        let cat_filter = Rc::new(RefCell::new(0usize));
        let sub_filter = Rc::new(RefCell::new(0usize));
        let downloaded_filter = Rc::new(RefCell::new(0u32));
        let search_query = Rc::new(RefCell::new(String::new()));
        let installed_ids = Rc::new(RefCell::new(load_installed_ids(&state)));
        let keep = Rc::new(RefCell::new(None::<gtk::glib::SourceId>));
        let sub_map: Rc<RefCell<Vec<Vec<String>>>> =
            Rc::new(RefCell::new(vec![vec![tr!("browse.tag_all")]]));
        let filter_data: Rc<RefCell<Option<FilterData>>> = {
            let cache_dir = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("cache");
            Rc::new(RefCell::new(FilterData::load(cache_dir.join("filters.json"))))
        };
        let cancel_token: Rc<RefCell<Arc<AtomicBool>>> =
            Rc::new(RefCell::new(Arc::new(AtomicBool::new(false))));
        let this = Self {
            container,
            content_box,
            _state_label: state_label,
            cache: cache.clone(),
            vrows: vrows.clone(),
            offset: offset.clone(),
            age_filter: age_filter.clone(),
            cat_filter: cat_filter.clone(),
            sub_filter: sub_filter.clone(),
            downloaded_filter: downloaded_filter.clone(),
            filter_data: filter_data.clone(),
            search_query: search_query.clone(),
            installed_ids: installed_ids.clone(),
            state: state.clone(),
            detail_drawer: detail_drawer.clone(),
            cancel_token: cancel_token.clone(),
            _keep_alive: keep.clone(),
        };

        // reload helper
        let mk_reload = {
            let state_for_reload = state.clone();
            let drawer_for_reload = detail_drawer.clone();
            let vr = vrows.clone();
            let cc = cache.clone();
            let po = offset.clone();
            let af = age_filter.clone();
            let cf = cat_filter.clone();
            let sf = sub_filter.clone();
            let df = downloaded_filter.clone();
            let sq = search_query.clone();
            let ii = installed_ids.clone();
            let fd = filter_data.clone();
            let cb2 = this.content_box.clone();
            let ct = this.cancel_token.clone();
            let bp = btn_prev.clone();
            let bn = btn_next.clone();
            let pl = page_label.clone();
            let sw = scrolled.clone();
            move || {
                *po.borrow_mut() = 0;
                let cg = cc.borrow();
                let fg = fd.borrow();
                if let (Some(cache), Some(fd)) = (cg.as_ref(), fg.as_ref()) {
                    let ci = *cf.borrow();
                    let si = *sf.borrow();
                    let cat = fd.categories.get(ci).cloned().unwrap_or_default();
                    let subs = fd
                        .subcategories
                        .get(&cat)
                        .cloned()
                        .unwrap_or_else(|| vec![tr!("browse.tag_all")]);
                    let query = sq.borrow().clone();
                    let filtered = filter_cards(
                        cache,
                        *af.borrow(),
                        ci,
                        si,
                        *df.borrow(),
                        &fd.categories,
                        &subs,
                        &query,
                        &ii.borrow(),
                    );
                    rel_vrows(&vr, &filtered, 0);
                    commit_ui(
                        &vr,
                        &cb2,
                        &ii.borrow(),
                        &state_for_reload,
                        &drawer_for_reload,
                        &ct,
                    );
                    upd_nav(
                        &bp,
                        &bn,
                        &pl,
                        cache,
                        0,
                        ci,
                        si,
                        *af.borrow(),
                        *df.borrow(),
                        fd,
                        &query,
                        &ii.borrow(),
                    );
                    sw.vadjustment().set_value(0.0);
                    preload(&filtered, 0, ct.borrow().clone());
                }
            }
        };

        // Filter UI
        {
            let sq = search_query.clone();
            let r = mk_reload.clone();
            search_entry.connect_search_changed(move |entry| {
                *sq.borrow_mut() = entry.text().to_string();
                r();
            });
        }
        {
            let af = age_filter.clone();
            let r = mk_reload.clone();
            age_combo.connect_selected_notify(move |c| {
                *af.borrow_mut() = c.selected();
                r();
            });
        }
        {
            let df = downloaded_filter.clone();
            let r = mk_reload.clone();
            downloaded_combo.connect_selected_notify(move |c| {
                *df.borrow_mut() = c.selected();
                r();
            });
        }
        {
            let cf = cat_filter.clone();
            let sf = sub_filter.clone();
            let sm = sub_map.clone();
            let sc = sub_combo.clone();
            let r = mk_reload.clone();
            cat_combo.connect_selected_notify(move |c| {
                *cf.borrow_mut() = c.selected() as usize;
                *sf.borrow_mut() = 0;
                let map = sm.borrow();
                let idx = c.selected() as usize;
                let subs = if idx < map.len() { &map[idx] } else { &map[0] };
                let mut items: Vec<&str> = subs.iter().map(|s| s.as_str()).collect();
                if items.len() == 1 {
                    items.push("");
                }
                sc.set_model(Some(&gtk::StringList::new(&items)));
                sc.set_selected(0);
                r();
            });
        }
        {
            let sf = sub_filter.clone();
            let r = mk_reload.clone();
            sub_combo.connect_selected_notify(move |c| {
                let idx = c.selected() as usize;
                let is_empty = c
                    .model()
                    .and_then(|m| m.downcast::<gtk::StringList>().ok())
                    .and_then(|m| m.string(idx as u32))
                    .map_or(true, |s| s.is_empty());
                if is_empty {
                    c.set_selected(0);
                    return;
                }
                *sf.borrow_mut() = idx;
                r();
            });
        }

        // Sync
        {
            let state_for_sync = state.clone();
            let drawer_for_sync = detail_drawer.clone();
            let slb = sync_label.clone();
            let cc = cache.clone();
            let vr = vrows.clone();
            let cb = this.content_box.clone();
            let po = offset.clone();
            let af2 = age_filter.clone();
            let cf2 = cat_filter.clone();
            let sf2 = sub_filter.clone();
            let df2 = downloaded_filter.clone();
            let sq2 = search_query.clone();
            let ii2 = installed_ids.clone();
            let fd2 = filter_data.clone();
            let sm2 = sub_map.clone();
            let stk = main_stack.clone();
            let bp = btn_prev.clone();
            let bn = btn_next.clone();
            let pl = page_label.clone();
            let cc2 = cat_combo.clone();
            let ct = this.cancel_token.clone();
            let cache_dir = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("cache");
            let fdir = cache_dir.clone();
            let (tx_p, rx_p) = std::sync::mpsc::channel::<String>();
            let (tx_c, rx_c) = std::sync::mpsc::channel::<MetadataCache>();
            let cdn_for_sync = state_for_sync.get_cdn_client();
            std::thread::spawn(move || {
                let _ = tx_p.send(tr!("browse.sync_checking_short"));
                let mut ex = MetadataCache::load(&cache_dir);
                let cdn = cdn_for_sync;

                // Try CDN first
                if let Some(ref cdn) = cdn {
                    let stale = ex.as_ref()
                        .map_or(true, |c| c.is_stale_via_cdn(cdn));
                    if !stale {
                        let c = ex.take().unwrap();
                        let _ = tx_p.send(tr!("browse.sync_cached", c.len().to_string()));
                        let _ = tx_c.send(c);
                        return;
                    }

                    let _ = tx_p.send(tr!("browse.sync_from_cdn"));
                    match MetadataCache::sync_from_cdn(cdn, &cache_dir, ex.as_ref()) {
                        Ok(c) => {
                            let _ = tx_p.send(tr!("browse.sync_cdn_done"));
                            let _ = tx_c.send(c);
                            return;
                        }
                        Err(e) => {
                            let _ = tx_p.send(tr!("browse.sync_cdn_fail", e).to_string());
                            // Keep error visible briefly so user can see the reason
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                        }
                    }
                }

                // Fall back to direct GameBanana
                let client = GameBananaClient::new(8552);
                let stale = ex.as_ref()
                    .map_or(true, |c| c.is_stale(&client).map_or(true, |s| !s));
                if ex.as_ref().map_or(false, |c| !stale) {
                    let c = ex.take().unwrap();
                    let _ = tx_p.send(tr!("browse.sync_cached", c.len().to_string()));
                    let _ = tx_c.send(c);
                    return;
                }

                let _ = tx_p.send(tr!("browse.sync_from_gb"));
                let tx = tx_p.clone();
                let cl = GameBananaClient::new(8552);
                let cd = cache_dir.clone();
                let tx3 = tx_p.clone();
                match MetadataCache::sync(
                    &cl, &cd, false, ex.as_ref(),
                    &|d, t| { let _ = tx.send(format!("{} / {}", d, t).to_string()); },
                    Some(&|info: String| { let _ = tx3.send(info); }),
                ) {
                    Ok(c) => {
                        let _ = tx_p.send(tr!("browse.sync_done"));
                        let _ = tx_c.send(c);
                    }
                    Err(_) => {
                        let _ = tx_p.send(tr!("browse.sync_fail"));
                    }
                }
            });
            let sid =
                gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                    if let Ok(msg) = rx_p.try_recv() {
                        slb.set_text(&msg);
                    }
                    if let Ok(c) = rx_c.try_recv() {
                        *cc.borrow_mut() = Some(c);
                        if let Some(ref cache) = *cc.borrow() {
                            let start = perf::now();
                            let fd = FilterData::build_from_cards(&cache.cards);
                            perf::log_elapsed_with_threshold(
                                "FilterData::build_from_cards(browse)",
                                start,
                                1,
                            );
                            fd.save(fdir.join("filters.json"));
                            let start = perf::now();
                            let mut map = vec![vec![tr!("browse.tag_all")]];
                            for (i, cat) in fd.categories.iter().enumerate() {
                                let subs = fd
                                    .subcategories
                                    .get(cat)
                                    .cloned()
                                    .unwrap_or_else(|| vec![tr!("browse.tag_all")]);
                                while map.len() <= i {
                                    map.push(vec![tr!("browse.tag_all")]);
                                }
                                map[i] = subs;
                            }
                            *sm2.borrow_mut() = map;
                            perf::log_elapsed_with_threshold(
                                "BrowsePage::build_subcategory_map",
                                start,
                                1,
                            );
                            let cats: Vec<String> = fd.categories.clone();
                            *fd2.borrow_mut() = Some(fd);
                            let start = perf::now();
                            let cat_refs: Vec<&str> = cats.iter().map(|s| s.as_str()).collect();
                            cc2.set_model(Some(&gtk::StringList::new(&cat_refs)));
                            cc2.set_selected(0);
                            perf::log_elapsed_with_threshold(
                                "BrowsePage::set_category_dropdown",
                                start,
                                1,
                            );
                        }
                        let cg = cc.borrow();
                        let fg = fd2.borrow();
                        if let (Some(cache), Some(fd)) = (cg.as_ref(), fg.as_ref()) {
                            let ci = *cf2.borrow();
                            let si = *sf2.borrow();
                            let cat = fd.categories.get(ci).cloned().unwrap_or_default();
                            let subs = fd
                                .subcategories
                                .get(&cat)
                                .cloned()
                                .unwrap_or_else(|| vec![tr!("browse.tag_all")]);
                            let query = sq2.borrow().clone();
                            let filtered = filter_cards(
                                cache,
                                *af2.borrow(),
                                ci,
                                si,
                                *df2.borrow(),
                                &fd.categories,
                                &subs,
                                &query,
                                &ii2.borrow(),
                            );
                            let start = perf::now();
                            rel_vrows(&vr, &filtered, *po.borrow());
                            perf::log_elapsed_with_threshold(
                                "BrowsePage::rel_vrows(startup)",
                                start,
                                1,
                            );
                            let start = perf::now();
                            commit_ui(
                                &vr,
                                &cb,
                                &ii2.borrow(),
                                &state_for_sync,
                                &drawer_for_sync,
                                &ct,
                            );
                            perf::log_elapsed_with_threshold(
                                "BrowsePage::commit_ui(startup)",
                                start,
                                1,
                            );
                            let start = perf::now();
                            upd_nav(
                                &bp,
                                &bn,
                                &pl,
                                cache,
                                *po.borrow(),
                                ci,
                                si,
                                *af2.borrow(),
                                *df2.borrow(),
                                fd,
                                &query,
                                &ii2.borrow(),
                            );
                            perf::log_elapsed_with_threshold(
                                "BrowsePage::upd_nav(startup)",
                                start,
                                1,
                            );
                            preload(&filtered, *po.borrow(), ct.borrow().clone());
                        }
                        stk.set_visible_child_name("grid");
                        return gtk::glib::ControlFlow::Break;
                    }
                    gtk::glib::ControlFlow::Continue
                });
            *keep.borrow_mut() = Some(sid);
        }

        // Pagination
        {
            let state_for_pagination = state.clone();
            let drawer_for_pagination = detail_drawer.clone();
            let cb = this.content_box.clone();
            let vr = vrows.clone();
            let c = cache.clone();
            let po = offset.clone();
            let af = age_filter.clone();
            let cf = cat_filter.clone();
            let sf = sub_filter.clone();
            let df = downloaded_filter.clone();
            let sq = search_query.clone();
            let ii = installed_ids.clone();
            let fd = filter_data.clone();
            let bp = btn_prev.clone();
            let bn = btn_next.clone();
            let pl = page_label.clone();
            let ct = this.cancel_token.clone();
            let sw = scrolled.clone();
            let go = move |delta: isize| {
                let new_off = (*po.borrow() as isize + delta).max(0) as usize;
                *po.borrow_mut() = new_off;
                let cg = c.borrow();
                let fg = fd.borrow();
                if let (Some(cache), Some(fd)) = (cg.as_ref(), fg.as_ref()) {
                    let ci = *cf.borrow();
                    let si = *sf.borrow();
                    let cat = fd.categories.get(ci).cloned().unwrap_or_default();
                    let subs = fd
                        .subcategories
                        .get(&cat)
                        .cloned()
                        .unwrap_or_else(|| vec![tr!("browse.tag_all")]);
                    let query = sq.borrow().clone();
                    let filtered = filter_cards(
                        cache,
                        *af.borrow(),
                        ci,
                        si,
                        *df.borrow(),
                        &fd.categories,
                        &subs,
                        &query,
                        &ii.borrow(),
                    );
                    rel_vrows(&vr, &filtered, new_off);
                    commit_ui(
                        &vr,
                        &cb,
                        &ii.borrow(),
                        &state_for_pagination,
                        &drawer_for_pagination,
                        &ct,
                    );
                    upd_nav(
                        &bp,
                        &bn,
                        &pl,
                        cache,
                        new_off,
                        ci,
                        si,
                        *af.borrow(),
                        *df.borrow(),
                        fd,
                        &query,
                        &ii.borrow(),
                    );
                    sw.vadjustment().set_value(0.0);
                    preload(&filtered, new_off, ct.borrow().clone());
                }
            };
            let g1 = go.clone();
            btn_prev.connect_clicked(move |_| g1(-(PAGE_STEP as isize)));
            let g2 = go.clone();
            btn_next.connect_clicked(move |_| g2(PAGE_STEP as isize));
        }
        this
    }

    pub fn refresh_installed_state(&self) {
        *self.installed_ids.borrow_mut() = load_installed_ids(&self.state);
        let cg = self.cache.borrow();
        let fg = self.filter_data.borrow();
        if let (Some(cache), Some(fd)) = (cg.as_ref(), fg.as_ref()) {
            let ci = *self.cat_filter.borrow();
            let si = *self.sub_filter.borrow();
            let cat = fd.categories.get(ci).cloned().unwrap_or_default();
            let subs = fd
                .subcategories
                .get(&cat)
                .cloned()
                .unwrap_or_else(|| vec![tr!("browse.tag_all")]);
            let query = self.search_query.borrow().clone();
            let filtered = filter_cards(
                cache,
                *self.age_filter.borrow(),
                ci,
                si,
                *self.downloaded_filter.borrow(),
                &fd.categories,
                &subs,
                &query,
                &self.installed_ids.borrow(),
            );
            rel_vrows(&self.vrows, &filtered, *self.offset.borrow());
            commit_ui(
                &self.vrows,
                &self.content_box,
                &self.installed_ids.borrow(),
                &self.state,
                &self.detail_drawer,
                &self.cancel_token,
            );
        }
    }
}

fn rel_vrows(vr: &RefCell<VecDeque<VRow>>, filtered: &[&ModCard], offset: usize) {
    let mut v = vr.borrow_mut();
    v.clear();
    let cards_on_page = (filtered.len().saturating_sub(offset)).min(SLOTS);
    let needed_rows = (cards_on_page + COLS - 1) / COLS;
    for i in 0..needed_rows {
        let mut row = VRow {
            cards: Default::default(),
            ui_done: false,
        };
        for col in 0..COLS {
            let idx = offset + i * COLS + col;
            if idx < filtered.len() {
                row.cards[col] = Some(filtered[idx].clone());
            }
        }
        v.push_back(row);
    }
}

fn count_filt(
    cache: &MetadataCache,
    age: u32,
    cat: usize,
    sub: usize,
    downloaded: u32,
    cats: &[String],
    subs: &[String],
    search: &str,
    installed_ids: &HashSet<u64>,
) -> usize {
    let cat_name = if cat > 0 && cat < cats.len() {
        cats[cat].as_str()
    } else {
        ""
    };
    let sub_name = if sub > 0 && sub < subs.len() {
        subs[sub].as_str()
    } else {
        ""
    };
    let query = NameQuery::parse(search);
    cache
        .cards
        .iter()
        .filter(|c| {
            card_matches_filters(
                c,
                age,
                downloaded,
                cat_name,
                sub_name,
                &query,
                installed_ids,
            )
        })
        .count()
}

fn commit_ui(
    vr: &RefCell<VecDeque<VRow>>,
    cb: &gtk::Box,
    installed_ids: &HashSet<u64>,
    state: &Rc<AppState>,
    drawer: &Rc<ModDetailDrawer>,
    cancel_token: &Rc<RefCell<Arc<AtomicBool>>>,
) {
    let _perf = perf::ScopeTimer::with_threshold("BrowsePage::commit_ui", 4);
    cancel_token.borrow().store(true, Ordering::Relaxed);
    let new_token = Arc::new(AtomicBool::new(false));
    *cancel_token.borrow_mut() = new_token.clone();
    let mut v = vr.borrow_mut();
    for (pos, row) in v.iter_mut().enumerate() {
        let ui_row = get_row(cb, pos);
        ui_row.set_visible(true);
        if !row.ui_done {
            while let Some(c) = ui_row.first_child() {
                ui_row.remove(&c);
            }
            for card in &row.cards {
                match card {
                    Some(c) => {
                        let w = build_browse_card(
                            c,
                            installed_ids.contains(&c.id),
                            state.clone(),
                            drawer.clone(),
                            new_token.clone(),
                        );
                        ui_row.append(&w);
                    }
                    None => {
                        let spacer = gtk::Box::builder()
                            .width_request(210)
                            .height_request(140)
                            .hexpand(true)
                            .visible(true)
                            .build();
                        ui_row.append(&spacer);
                    }
                }
            }
            row.ui_done = true;
        }
    }
    for pos in v.len()..ROWS {
        get_row(cb, pos).set_visible(false);
    }
}

fn upd_nav(
    prev: &gtk::Button,
    next: &gtk::Button,
    label: &gtk::Label,
    cache: &MetadataCache,
    offset: usize,
    cat: usize,
    sub: usize,
    age: u32,
    downloaded: u32,
    fd: &FilterData,
    search: &str,
    installed_ids: &HashSet<u64>,
) {
    let cats = &fd.categories;
    let cat_name = if cat > 0 && cat < cats.len() {
        cats[cat].as_str()
    } else {
        ""
    };
    let subs = fd.subcategories.get(cat_name).cloned().unwrap_or_default();
    let total = count_filt(
        cache,
        age,
        cat,
        sub,
        downloaded,
        cats,
        &subs,
        search,
        installed_ids,
    );
    let total_pages = if total > 0 {
        (total + PAGE_STEP - 1) / PAGE_STEP
    } else {
        1
    };
    let max_off = total.saturating_sub(1) / PAGE_STEP * PAGE_STEP;
    let page = offset / PAGE_STEP + 1;
    prev.set_sensitive(offset > 0);
    next.set_sensitive(offset < max_off);
    label.set_text(&format!("{}/{}", page, total_pages).to_string());
}

fn build_browse_card(
    card: &ModCard,
    downloaded: bool,
    _state: Rc<AppState>,
    drawer: Rc<ModDetailDrawer>,
    cancel: Arc<AtomicBool>,
) -> gtk::Overlay {
    let _perf =
        perf::ScopeTimer::with_threshold(format!("BrowsePage::build_browse_card {}", card.id).to_string(), 2);
    let widget = ModCardWidget::new_with_cancel(card, cancel);
    widget.activate();

    let shell = gtk::Overlay::builder().hexpand(true).build();
    shell.set_child(Some(widget.widget()));

    let gesture = gtk::GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    let card_for_detail = card.clone();
    gesture.connect_released(move |_, _, _, _| {
        drawer.open(card_for_detail.clone());
    });
    shell.add_controller(gesture);

    if downloaded {
        let tag = gtk::Label::builder()
            .label(tr!("browse.downloaded_badge"))
            .css_classes(["tag-installed"])
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .margin_start(10)
            .margin_top(10)
            .build();
        tag.set_can_target(false);
        shell.add_overlay(&tag);
    }

    shell
}

fn card_matches_filters(
    card: &ModCard,
    age: u32,
    downloaded: u32,
    cat_name: &str,
    sub_name: &str,
    query: &NameQuery,
    installed_ids: &HashSet<u64>,
) -> bool {
    if age == 1 && card.is_r18 {
        return false;
    }
    if age == 2 && !card.is_r18 {
        return false;
    }
    let is_downloaded = installed_ids.contains(&card.id);
    if downloaded == 1 && !is_downloaded {
        return false;
    }
    if downloaded == 2 && is_downloaded {
        return false;
    }
    if !cat_name.is_empty() && card.category != cat_name {
        return false;
    }
    if !sub_name.is_empty() && card.subcategory.as_deref() != Some(sub_name) {
        return false;
    }
    query.matches(&card.name)
}

fn load_installed_ids(state: &AppState) -> HashSet<u64> {
    state
        .manager
        .list_installed()
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.mod_id)
        .collect()
}

fn filter_cards<'a>(
    cache: &'a MetadataCache,
    age: u32,
    cat: usize,
    sub: usize,
    downloaded: u32,
    cats: &[String],
    subs: &[String],
    search: &str,
    installed_ids: &HashSet<u64>,
) -> Vec<&'a ModCard> {
    let cat_name = if cat > 0 && cat < cats.len() {
        cats[cat].as_str()
    } else {
        ""
    };
    let sub_name = if sub > 0 && sub < subs.len() {
        subs[sub].as_str()
    } else {
        ""
    };
    let query = NameQuery::parse(search);
    cache
        .cards
        .iter()
        .filter(|c| {
            card_matches_filters(
                c,
                age,
                downloaded,
                cat_name,
                sub_name,
                &query,
                installed_ids,
            )
        })
        .collect()
}

fn preload(filtered: &[&ModCard], offset: usize, cancel: Arc<AtomicBool>) {
    let max_off = filtered.len().saturating_sub(SLOTS);
    for adj in [offset.saturating_sub(PAGE_STEP), offset + PAGE_STEP] {
        if adj == offset || adj > max_off {
            continue;
        }
        for i in 0..SLOTS {
            if let Some(card) = filtered.get(adj + i) {
                let url = card
                    .thumbnail_url
                    .clone()
                    .or_else(|| card.cover_url.clone());
                if let Some(u) = url {
                    let c = cancel.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        if c.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = anime_mod_manager::download_image(&u);
                    });
                }
            }
        }
    }
}

fn get_row(content: &gtk::Box, idx: usize) -> gtk::Box {
    let (mut i, mut child) = (0, content.first_child());
    while let Some(c) = child {
        if i == idx {
            return c.downcast().unwrap();
        }
        child = c.next_sibling();
        i += 1;
    }
    panic!("row {idx} not found");
}
