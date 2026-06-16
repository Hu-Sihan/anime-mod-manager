use gtk;
use gtk::prelude::*;

fn fix_dropdown_popup(dropdown: &gtk::DropDown) {
    let dd = dropdown.clone();
    dropdown.connect_activate(move |_| {
        let dd = dd.clone();
        // Use idle_add to run after the popover is fully constructed
        gtk::glib::idle_add_local(move || {
            let mut queue: Vec<gtk::Widget> = Vec::new();
            if let Some(first) = dd.first_child() {
                queue.push(first);
            }
            while let Some(w) = queue.pop() {
                if let Some(lv) = w.downcast_ref::<gtk::ListView>() {
                    lv.set_enable_rubberband(false);
                }
                if let Some(sw) = w.downcast_ref::<gtk::ScrolledWindow>() {
                    sw.set_min_content_height(0);
                    sw.set_max_content_height(800);
                }
                let mut child = w.first_child();
                while let Some(c) = child {
                    queue.push(c.clone());
                    child = c.next_sibling();
                }
            }
            gtk::glib::ControlFlow::Break
        });
    });
}

pub fn apply_compact_dropdown_factory(dropdown: &gtk::DropDown) {
    dropdown.set_enable_search(false);
    fix_dropdown_popup(dropdown);

    let button_factory = gtk::SignalListItemFactory::new();
    button_factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["compact-dropdown-current"])
            .build();
        item.set_child(Some(&label));
    });
    button_factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(label) = item
            .child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
        else {
            return;
        };
        let text = item
            .item()
            .and_then(|obj| obj.downcast::<gtk::StringObject>().ok())
            .map(|obj| obj.string().to_string())
            .unwrap_or_default();
        label.set_text(&text);
    });
    dropdown.set_factory(Some(&button_factory));

    let list_factory = gtk::SignalListItemFactory::new();
    list_factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(2)
            .halign(gtk::Align::Fill)
            .width_request(75)
            .css_classes(["compact-dropdown-item"])
            .visible(false)
            .build();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["compact-dropdown-item-label"])
            .build();
        label.set_width_chars(6);
        label.set_max_width_chars(6);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let check = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .pixel_size(9)
            .visible(false)
            .css_classes(["compact-dropdown-item-check"])
            .build();
        row.append(&label);
        row.append(&check);
        item.set_child(Some(&row));
        item.connect_selected_notify(|list_item| {
            let Some(row) = list_item
                .child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
            else {
                return;
            };
            let Some(check) = row
                .last_child()
                .and_then(|child| child.downcast::<gtk::Image>().ok())
            else {
                return;
            };
            check.set_visible(list_item.is_selected());
        });
    });
    list_factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = item
            .child()
            .and_then(|child| child.downcast::<gtk::Box>().ok())
        else {
            return;
        };
        let Some(label) = row
            .first_child()
            .and_then(|child| child.downcast::<gtk::Label>().ok())
        else {
            return;
        };
        let Some(check) = row
            .last_child()
            .and_then(|child| child.downcast::<gtk::Image>().ok())
        else {
            return;
        };
        let text = item
            .item()
            .and_then(|obj| obj.downcast::<gtk::StringObject>().ok())
            .map(|obj| obj.string().to_string())
            .unwrap_or_default();
        let has_text = !text.is_empty();
        row.set_visible(has_text);
        label.set_text(&text);
        check.set_visible(item.is_selected());
    });
    dropdown.set_list_factory(Some(&list_factory));
}

#[derive(Clone, Copy)]
enum BoolOp {
    And,
    Or,
}

#[derive(Clone)]
enum GapOp {
    Exact(usize, String),
    Any(String),
}

#[derive(Clone)]
struct NameMatcher {
    negated: bool,
    head: String,
    links: Vec<GapOp>,
}

#[derive(Default)]
pub struct NameQuery {
    matchers: Vec<NameMatcher>,
    ops: Vec<BoolOp>,
}

impl NameQuery {
    pub fn parse(input: &str) -> Self {
        let chars: Vec<char> = input.trim().chars().collect();
        if chars.is_empty() {
            return Self::default();
        }

        let mut cursor = 0usize;
        let mut matchers = Vec::new();
        let mut ops = Vec::new();

        while cursor < chars.len() {
            skip_spaces(&chars, &mut cursor);
            if cursor >= chars.len() {
                break;
            }

            let Some(matcher) = parse_name_matcher(&chars, &mut cursor) else {
                cursor += 1;
                continue;
            };
            matchers.push(matcher);

            let had_space = skip_spaces(&chars, &mut cursor);
            if cursor >= chars.len() {
                break;
            }

            match chars[cursor] {
                '+' => {
                    ops.push(BoolOp::And);
                    cursor += 1;
                }
                '|' => {
                    ops.push(BoolOp::Or);
                    cursor += 1;
                }
                _ if had_space && can_start_matcher(chars[cursor]) => {
                    ops.push(BoolOp::And);
                }
                _ => {}
            }
        }

        while ops.len() >= matchers.len() {
            ops.pop();
        }

        Self { matchers, ops }
    }

    pub fn matches(&self, name: &str) -> bool {
        if self.matchers.is_empty() {
            return true;
        }

        let haystack: Vec<char> = name.to_lowercase().chars().collect();
        let mut matched = self.matchers[0].matches(&haystack);

        for (op, matcher) in self.ops.iter().zip(self.matchers.iter().skip(1)) {
            let rhs = matcher.matches(&haystack);
            matched = match op {
                BoolOp::And => matched && rhs,
                BoolOp::Or => matched || rhs,
            };
        }

        matched
    }
}

impl NameMatcher {
    fn matches(&self, haystack: &[char]) -> bool {
        let head: Vec<char> = self.head.chars().collect();
        if head.is_empty() {
            return false;
        }

        let matched = !match_chain(haystack, &head, &self.links).is_empty();
        if self.negated {
            !matched
        } else {
            matched
        }
    }
}

fn parse_name_matcher(chars: &[char], cursor: &mut usize) -> Option<NameMatcher> {
    skip_spaces(chars, cursor);
    if *cursor >= chars.len() {
        return None;
    }

    let mut negated = false;
    if chars[*cursor] == '-' {
        negated = true;
        *cursor += 1;
        skip_spaces(chars, cursor);
    }

    let head = parse_name_word(chars, cursor)?;
    let mut links = Vec::new();

    loop {
        let checkpoint = *cursor;
        skip_spaces(chars, cursor);
        if *cursor >= chars.len() {
            break;
        }

        if chars[*cursor] == '_' {
            let mut gap = 0usize;
            while *cursor < chars.len() && chars[*cursor] == '_' {
                gap += 1;
                *cursor += 1;
            }
            skip_spaces(chars, cursor);
            if let Some(next) = parse_name_word(chars, cursor) {
                links.push(GapOp::Exact(gap, next));
                continue;
            }
            *cursor = checkpoint;
            break;
        }

        if chars[*cursor] == '*' {
            *cursor += 1;
            skip_spaces(chars, cursor);
            if let Some(next) = parse_name_word(chars, cursor) {
                links.push(GapOp::Any(next));
                continue;
            }
            *cursor = checkpoint;
            break;
        }

        *cursor = checkpoint;
        break;
    }

    Some(NameMatcher {
        negated,
        head,
        links,
    })
}

fn parse_name_word(chars: &[char], cursor: &mut usize) -> Option<String> {
    let start = *cursor;
    while *cursor < chars.len() && is_name_word_char(chars[*cursor]) {
        *cursor += 1;
    }

    if *cursor == start {
        None
    } else {
        Some(
            chars[start..*cursor]
                .iter()
                .collect::<String>()
                .to_lowercase(),
        )
    }
}

fn is_name_word_char(ch: char) -> bool {
    !ch.is_whitespace() && !matches!(ch, '+' | '|' | '-' | '_' | '*')
}

fn can_start_matcher(ch: char) -> bool {
    !ch.is_whitespace() && !matches!(ch, '+' | '|' | '_' | '*')
}

fn skip_spaces(chars: &[char], cursor: &mut usize) -> bool {
    let start = *cursor;
    while *cursor < chars.len() && chars[*cursor].is_whitespace() {
        *cursor += 1;
    }
    *cursor > start
}

fn match_chain(haystack: &[char], head: &[char], links: &[GapOp]) -> Vec<(usize, usize)> {
    let mut spans = find_occurrences(haystack, head);
    for link in links {
        let needle = match link {
            GapOp::Exact(_, value) | GapOp::Any(value) => value.chars().collect::<Vec<_>>(),
        };
        let next_spans = find_occurrences(haystack, &needle);
        let mut combined = Vec::new();

        for (start, end) in spans.iter().copied() {
            for (next_start, next_end) in next_spans.iter().copied() {
                let ok = match link {
                    GapOp::Exact(gap, _) => next_start == end + *gap,
                    GapOp::Any(_) => next_start >= end,
                };
                if ok {
                    combined.push((start, next_end));
                }
            }
        }

        if combined.is_empty() {
            return combined;
        }
        spans = combined;
    }

    spans
}

fn find_occurrences(haystack: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for start in 0..=haystack.len() - needle.len() {
        if haystack[start..start + needle.len()] == *needle {
            matches.push((start, start + needle.len()));
        }
    }
    matches
}
