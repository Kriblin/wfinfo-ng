use image::DynamicImage;
use log::{debug, info, warn};

use crate::{
    database::{Database, Item},
    error::Error,
    ocr::{normalize_string, reward_image_to_reward_names},
    theme::Theme,
    ui::overlay::Reward,
};

pub(super) fn run_detection(
    image: DynamicImage,
    database: &Database,
) -> Result<(Vec<Reward>, Theme, Vec<String>), Error> {
    let (text, theme) = reward_image_to_reward_names(image, None)?;
    let raw_text = text.clone();
    let text: Vec<String> = text.iter().map(|text| normalize_string(text)).collect();
    debug!("{:#?}", text);

    let items: Vec<Option<&Item>> = text
        .iter()
        .map(|text| database.find_item(text, None))
        .collect();

    let best = items
        .iter()
        .map(|item| {
            item.map(|item| {
                item.platinum
                    .max(item.ducats as f32 / 10.0 + item.platinum / 100.0)
            })
            .unwrap_or(0.0)
        })
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|best| best.0);

    let mut rewards = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(item) = item {
            let set = try_find_set(item.drop_name.clone(), database);
            let set_info = if let Some(set) = set {
                format!("(Set:{}, Plat: {})", set.name, set.platinum)
            } else {
                String::new()
            };
            info!(
                "{}\n\t{}\t{}\t{}\t{}",
                item.drop_name,
                item.platinum,
                item.ducats as f32 / 10.0,
                if Some(index) == best { "<----" } else { "" },
                set_info
            );
            rewards.push(Reward {
                name: item.drop_name.clone(),
                platinum: item.platinum,
                ducats: item.ducats,
                is_best: Some(index) == best,
                set_info,
            });
        } else {
            warn!("Unknown item\n\tUnknown");
        }
    }

    Ok((rewards, theme, raw_text))
}

fn try_find_set(item_name: String, database: &Database) -> Option<&Item> {
    let words: Vec<&str> = item_name.split_whitespace().take(2).collect();
    if words.len() < 2 {
        return None;
    }

    database.find_item(&(words.join(" ") + " Set"), None)
}
