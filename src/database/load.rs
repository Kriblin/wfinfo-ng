use std::{collections::HashMap, fs::read_to_string, path::Path};

use log::warn;
use serde_json::Value;

use crate::{
    error::{DatabaseError, Result},
    models::{
        item::{DucatItem, EquipmentItem, EquipmentType, FilteredItems, Relic},
        price::PriceItem,
    },
};

use super::{Database, Item};

impl Database {
    pub fn load_from_file(
        prices: Option<&Path>,
        filtered_items: Option<&Path>,
    ) -> Result<Database> {
        let prices_path = prices.unwrap_or_else(|| Path::new("test-data/prices.json"));
        let price_table = Self::load_prices(prices_path)?;

        let filtered_items_path =
            filtered_items.unwrap_or_else(|| Path::new("test-data/filtered_items.json"));
        let filtered_items = Self::load_filtered_items(filtered_items_path)?;

        let mut items = Self::process_items(
            filtered_items.eqmt,
            filtered_items.ignored_items,
            &price_table,
        );

        if let Some(item) = items.iter_mut().find(|item| item.name == "Forma Blueprint") {
            item.platinum = 35.0 / 3.0;
        };

        Ok(Database {
            items,
            relics: filtered_items.relics,
        })
    }

    fn load_prices(prices_path: &Path) -> Result<HashMap<String, f32>> {
        let text = read_to_string(prices_path).map_err(|e| {
            DatabaseError::FileNotFound(prices_path.to_path_buf(), Some(e.to_string()))
        })?;

        let price_list: Vec<PriceItem> = serde_json::from_str(&text).map_err(|e| {
            DatabaseError::InvalidFormat(format!("Failed to parse prices JSON: {}", e))
        })?;

        Ok(price_list
            .into_iter()
            .map(|item| (item.name, item.custom_avg))
            .collect())
    }

    fn load_filtered_items(filtered_items_path: &Path) -> Result<FilteredItems> {
        let text = read_to_string(filtered_items_path).map_err(|e| {
            DatabaseError::FileNotFound(filtered_items_path.to_path_buf(), Some(e.to_string()))
        })?;

        let mut json = serde_json::from_str(&text).map_err(|e| {
            DatabaseError::InvalidFormat(format!("Failed to parse filtered items JSON: {}", e))
        })?;

        remove_empty_relics_from_json(&mut json)?;

        serde_json::from_value(json).map_err(|e| {
            DatabaseError::InvalidFormat(format!("Failed to convert JSON to FilteredItems: {}", e))
                .into()
        })
    }

    fn process_items(
        eqmt: HashMap<String, EquipmentItem>,
        ignored_items: HashMap<String, DucatItem>,
        price_table: &HashMap<String, f32>,
    ) -> Vec<Item> {
        eqmt.into_iter()
            .flat_map(|(equipment_name, equipment_item)| {
                let item_type = equipment_item.item_type;
                let set_name = format!("{} Set", equipment_name);
                let set_platinum = price_table.get(&set_name).copied();
                if set_platinum.is_none() {
                    warn!("Failed to find price for item: {}", set_name);
                }
                let set_item = set_platinum.map(|platinum| Item {
                    name: set_name.clone(),
                    drop_name: set_name,
                    platinum,
                    ducats: 0,
                });
                equipment_item
                    .parts
                    .into_iter()
                    .filter_map(move |(name, ducat_item)| {
                        let item_is_part = name.ends_with("Systems")
                            || name.ends_with("Neuroptics")
                            || name.ends_with("Chassis")
                            || name.ends_with("Harness")
                            || name.ends_with("Wings");

                        let drop_name = match item_type {
                            EquipmentType::Warframes | EquipmentType::Archwing
                                if item_is_part && !name.ends_with("Blueprint") =>
                            {
                                format!("{} Blueprint", name)
                            }
                            _ => name.to_owned(),
                        };

                        let platinum = price_table
                            .get(&name)
                            .or_else(|| price_table.get(&format!("{} Blueprint", name)))
                            .copied()
                            .or_else(|| {
                                warn!("Failed to find price for item: {}", name);
                                None
                            })?;

                        Some(Item {
                            name,
                            drop_name,
                            platinum,
                            ducats: ducat_item.ducats,
                        })
                    })
                    .chain(set_item)
            })
            .chain(ignored_items.into_keys().map(|name| Item {
                name: name.to_owned(),
                drop_name: name.to_owned(),
                platinum: 0.0,
                ducats: 0,
            }))
            .collect()
    }
}

fn remove_empty_relics_from_json(value: &mut Value) -> Result<()> {
    let relics = value
        .get_mut("relics")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| DatabaseError::InvalidFormat("relics field is not an object".to_string()))?;

    for kind in relics.values_mut() {
        if let Some(kind) = kind.as_object_mut() {
            kind.retain(|_, relic| serde_json::from_value::<Relic>(relic.clone()).is_ok());
        }
    }
    Ok(())
}
