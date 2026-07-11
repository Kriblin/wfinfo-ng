use levenshtein::levenshtein;

use crate::error::{DatabaseError, Result};

use super::{Database, Item};

impl Database {
    pub fn find_item(&self, needle: &str, threshold: Option<usize>) -> Option<&Item> {
        let needle_clean = needle.replace([' ', '\n'], "");
        let needle_is_set = needle_clean.ends_with("Set");
        self.items
            .iter()
            .filter(|item| needle_is_set || !item.name.ends_with("Set"))
            .filter_map(|item| {
                let distance = levenshtein(&item.drop_name.replace(' ', ""), &needle_clean);
                let current_threshold = threshold.unwrap_or(item.drop_name.len() / 3);
                (distance <= current_threshold).then_some((item, distance))
            })
            .min_by_key(|(_, distance)| *distance)
            .map(|(item, _)| item)
    }

    pub fn find_item_exact(&self, needle: &str) -> Result<&Item> {
        self.items
            .iter()
            .find(|item| item.name == needle)
            .ok_or_else(|| DatabaseError::ItemNotFound(needle.to_string()).into())
    }
}
