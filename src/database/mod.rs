use std::{collections::HashMap, fs::read_to_string, path::Path};

use levenshtein::levenshtein;
use log::{error, warn};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::{DatabaseError, Result},
    statistics::{self, Bucket},
    models::{
        item::{DucatItem, EquipmentItem, EquipmentType, FilteredItems, Refinement, Relic, Relics},
        price::PriceItem,
    },
};

#[derive(Clone, Debug, Deserialize)]
pub struct Database {
    items: Vec<Item>,
    pub relics: Relics,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Item {
    pub name: String,
    pub drop_name: String,
    pub platinum: f32,
    pub ducats: usize,
}

impl Database {
    pub fn load_from_file(prices: Option<&Path>, filtered_items: Option<&Path>) -> Result<Database> {
        let prices_path = prices.unwrap_or_else(|| Path::new("prices.json"));
        let price_table = Self::load_prices(prices_path)?;

        let filtered_items_path = filtered_items.unwrap_or_else(|| Path::new("filtered_items.json"));
        let filtered_items = Self::load_filtered_items(filtered_items_path)?;

        let mut items = Self::process_items(
            filtered_items.eqmt,
            filtered_items.ignored_items,
            &price_table,
        );

        if let Some(item) = items.iter_mut().find(|item| item.name == "Forma Blueprint") {
            item.platinum = 35.0 / 3.0;
        };

        let relics = filtered_items.relics;

        Ok(Database { items, relics })
    }

    fn load_prices(prices_path: &Path) -> Result<HashMap<String, f32>> {
        let text = read_to_string(prices_path)
            .map_err(|e| DatabaseError::FileNotFound(prices_path.to_path_buf(), Some(e.to_string())))?;

        let price_list: Vec<PriceItem> = serde_json::from_str(&text)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to parse prices JSON: {}", e)))?;

        Ok(price_list
            .into_iter()
            .map(|item| (item.name, item.custom_avg))
            .collect())
    }

    fn load_filtered_items(filtered_items_path: &Path) -> Result<FilteredItems> {
        let text = read_to_string(filtered_items_path)
            .map_err(|e| DatabaseError::FileNotFound(filtered_items_path.to_path_buf(), Some(e.to_string())))?;

        let mut json = serde_json::from_str(&text)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to parse filtered items JSON: {}", e)))?;

        remove_empty_relics_from_json(&mut json)?;

        serde_json::from_value(json)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to convert JSON to FilteredItems: {}", e)).into())
    }

    fn process_items(
        eqmt: HashMap<String, EquipmentItem>,
        ignored_items: HashMap<String, DucatItem>,
        price_table: &HashMap<String, f32>,
    ) -> Vec<Item> {
        eqmt.into_iter()
            .flat_map(|(_name, equipment_item)| {
                let item_type = equipment_item.item_type;
                equipment_item.parts.into_iter().filter_map(move |(name, ducat_item)| {
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
            })
            .chain(ignored_items.into_iter().map(|(name, _)| Item {
                name: name.to_owned(),
                drop_name: name.to_owned(),
                platinum: 0.0,
                ducats: 0,
            }))
            .collect()
    }

    pub fn find_item(&self, needle: &str, threshold: Option<usize>) -> Option<&Item> {
        let needle_clean = needle.replace([' ', '\n'], "");
        self.items
            .iter()
            .filter(|item| !item.name.ends_with("Set"))
            .filter_map(|item| {
                let dist = levenshtein(&item.drop_name.replace(' ', ""), &needle_clean);
                let current_threshold = threshold.unwrap_or(item.drop_name.len() / 3);
                if dist <= current_threshold {
                    Some((item, dist))
                } else {
                    None
                }
            })
            .min_by_key(|(_, dist)| *dist)
            .map(|(item, _)| item)
    }

    pub fn find_item_exact(&self, needle: &str) -> Result<&Item> {
        self.items.iter().find(|item| item.name == needle)
            .ok_or_else(|| DatabaseError::ItemNotFound(needle.to_string()).into())
    }

    fn relic_to_bucket(&self, relic: &Relic, refinement: Refinement) -> Result<Bucket> {
        let items = relic
            .rewards()
            .into_iter()
            .map(|(name, reward_type)| {
                let item = self.find_item_exact(name)?;
                Ok(statistics::Item {
                    value: item.platinum,
                    probability: refinement.chance(reward_type),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Bucket::new(items))
    }

    pub fn single_relic_value(&self, relic: &Relic, refinement: Refinement) -> f32 {
        relic
            .rewards()
            .into_iter()
            .map(|(name, reward_type)| {
                let plat = match self.find_item_exact(name) {
                    Ok(item) => item.platinum,
                    Err(e) => {
                        warn!("Failed to find item {name}: {e}");
                        0.0
                    }
                };
                plat * refinement.chance(reward_type)
            })
            .sum()
    }

    pub fn shared_relic_value(
        &self,
        relic: &Relic,
        refinement: Refinement,
        number_of_relics: u32,
    ) -> f32 {
        match self.relic_to_bucket(relic, refinement) {
            Ok(bucket) => bucket.expectation_of_best_of_n(number_of_relics),
            Err(e) => {
                error!("Error calculating relic value: {e}");
                0.0 // Return a default value in case of error
            }
        }
    }

    pub fn shared_relic_value_bruteforce(
        &self,
        relic: &Relic,
        refinement: Refinement,
        _number_of_relics: u32,
    ) -> f32 {
        let rewards: Vec<_> = relic
            .rewards()
            .into_iter()
            .map(|(name, reward_type)| {
                let plat = match self.find_item_exact(name) {
                    Ok(item) => item.platinum,
                    Err(e) => {
                        warn!("Failed to find item {name}: {e}");
                        0.0
                    }
                };
                (plat, refinement.chance(reward_type))
            })
            .collect();

        let mut value = 0.0;
        for r1 in &rewards {
            for r2 in &rewards {
                for r3 in &rewards {
                    for r4 in &rewards {
                        let max_value = [r1.0, r2.0, r3.0, r4.0]
                            .into_iter()
                            .max_by(|a, b| a.total_cmp(b))
                            .unwrap_or(0.0);

                        value += max_value * r1.1 * r2.1 * r3.1 * r4.1;
                    }
                }
            }
        }
        value
    }
}

fn remove_empty_relics_from_json(value: &mut Value) -> Result<()> {
    let relics = value
        .get_mut("relics")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| DatabaseError::InvalidFormat("relics field is not an object".to_string()))?;

    for kind in relics.values_mut() {
        if let Some(kind_obj) = kind.as_object_mut() {
            kind_obj.retain(|_, relic| serde_json::from_value::<Relic>(relic.clone()).is_ok());
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    pub fn can_load_database() {
        Database::load_from_file(None, None)
            .expect("Failed to load database");
    }

    #[test]
    pub fn can_find_items() {
        let db = Database::load_from_file(None, None)
            .expect("Failed to load database");

        let item = db
            .find_item("TitaniaPrimeBlueprint", Some(0))
            .expect("Failed to find Titania Prime Blueprint in database");
        assert_eq!(item.name, "Titania Prime Blueprint");

        let item = db
            .find_item("OctaviaPrimeBlueprint", Some(0))
            .expect("Failed to find Octavia Prime Blueprint in database");
        assert_eq!(item.name, "Octavia Prime Blueprint");
    }

    #[test]
    pub fn can_find_fuzzy_items() {
        let db = Database::load_from_file(None, None)
            .expect("Failed to load database");

        let item = db
            .find_item("Akstlett Prlme Recver", None)
            .expect("Failed to fuzzy find Akstiletto Prime Receiver in database");
        assert_eq!(item.name, "Akstiletto Prime Receiver");

        let item = db
            .find_item("ctavio Prlme Blueprnt", None)
            .expect("Failed to fuzzy find Octavia Prime Blueprint in database");
        assert_eq!(item.name, "Octavia Prime Blueprint");

        let item = db
            .find_item("Oclavia Prime Syslems\nBlueprint\n", None)
            .expect("Failed to fuzzy find Octavia Prime Blueprint in database");
        assert_eq!(item.name, "Octavia Prime Systems");
    }

    #[test]
    fn validate_shared_relic_values() {
        let database = Database::load_from_file(None, None)
            .expect("Failed to load database");

        for (name, relic) in database.relics.lith.iter() {
            println!("{} {:#?}", name, relic);
            assert_relative_eq!(
                database.shared_relic_value(relic, Refinement::Radiant, 4),
                database.shared_relic_value_bruteforce(relic, Refinement::Radiant, 4),
                epsilon = 0.01
            )
        }
        for (name, relic) in database.relics.meso.iter() {
            println!("{} {:#?}", name, relic);
            assert_relative_eq!(
                database.shared_relic_value(relic, Refinement::Radiant, 4),
                database.shared_relic_value_bruteforce(relic, Refinement::Radiant, 4),
                epsilon = 0.01
            )
        }
        for (name, relic) in database.relics.neo.iter() {
            println!("{} {:#?}", name, relic);
            assert_relative_eq!(
                database.shared_relic_value(relic, Refinement::Radiant, 4),
                database.shared_relic_value_bruteforce(relic, Refinement::Radiant, 4),
                epsilon = 0.01
            )
        }
        for (name, relic) in database.relics.axi.iter() {
            println!("{} {:#?}", name, relic);
            assert_relative_eq!(
                database.shared_relic_value(relic, Refinement::Radiant, 4),
                database.shared_relic_value_bruteforce(relic, Refinement::Radiant, 4),
                epsilon = 0.01
            )
        }
    }
}
