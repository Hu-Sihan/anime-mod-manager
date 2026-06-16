use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Pre-computed filter options from local cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterData {
    /// Category names
    pub categories: Vec<String>,
    /// category_name → subcategory_names
    pub subcategories: HashMap<String, Vec<String>>,
}

impl FilterData {
    /// Build FilterData by scanning all cards in a MetadataCache slice
    pub fn build_from_cards(cards: &[crate::models::ModCard]) -> Self {
        let mut categories = vec!["全部".to_string()];
        let mut sub_map: HashMap<String, Vec<String>> = HashMap::new();
        sub_map.insert("全部".to_string(), vec!["全部".to_string()]);

        for card in cards {
            let cat = card.category.clone();
            if cat.is_empty() {
                continue;
            }
            if !categories.contains(&cat) {
                categories.push(cat.clone());
            }
            let subs = sub_map
                .entry(cat.clone())
                .or_insert_with(|| vec!["全部".to_string()]);
            if let Some(ref sub) = card.subcategory {
                if !sub.is_empty() && !subs.contains(sub) {
                    subs.push(sub.clone());
                }
            }
        }

        for subs in sub_map.values_mut() {
            subs.sort();
            // Ensure "全部" is always first
            if let Some(pos) = subs.iter().position(|s| s == "全部") {
                if pos != 0 {
                    subs.remove(pos);
                    subs.insert(0, "全部".to_string());
                }
            }
        }

        Self {
            categories,
            subcategories: sub_map,
        }
    }

    /// Save to a JSON file
    pub fn save(&self, path: impl AsRef<Path>) {
        if let Ok(json) = serde_json::to_string(self) {
            let _ = fs::write(path.as_ref(), json);
        }
    }

    /// Load from a JSON file
    pub fn load(path: impl AsRef<Path>) -> Option<Self> {
        let data = fs::read_to_string(path.as_ref()).ok()?;
        serde_json::from_str(&data).ok()
    }
}
