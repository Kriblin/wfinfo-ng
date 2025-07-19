use std::{collections::HashMap, fs::read_to_string, path::Path};

use levenshtein::levenshtein;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::{DatabaseError, Result},
    statistics::{self, Bucket},
    wfinfo_data::{
        item_data::{EquipmentType, FilteredItems, Refinement, Relic, Relics},
        price_data::PriceItem,
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
        // download file from: https://api.warframestat.us/wfinfo/prices
        let prices_path = prices.unwrap_or_else(|| Path::new("prices.json"));
        let text = read_to_string(prices_path)
            .map_err(|e| DatabaseError::FileNotFound(prices_path.to_path_buf(), Some(e.to_string())))?;

        let price_list: Vec<PriceItem> = serde_json::from_str(&text)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to parse prices JSON: {}", e)))?;

        let price_table: HashMap<String, f32> = price_list
            .into_iter()
            .map(|item| (item.name, item.custom_avg))
            .collect();

        let filtered_items_path = filtered_items.unwrap_or_else(|| Path::new("filtered_items.json"));
        let text = read_to_string(filtered_items_path)
            .map_err(|e| DatabaseError::FileNotFound(filtered_items_path.to_path_buf(), Some(e.to_string())))?;

        let mut json = serde_json::from_str(&text)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to parse filtered items JSON: {}", e)))?;

        remove_empty_relics_from_json(&mut json)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to remove empty relics: {}", e)))?;

        let filtered_items: FilteredItems = serde_json::from_value(json)
            .map_err(|e| DatabaseError::InvalidFormat(format!("Failed to convert JSON to FilteredItems: {}", e)))?;

        let mut items: Vec<_> = filtered_items
            .eqmt
            .iter()
            .flat_map(|(_name, equipment_item)| {
                equipment_item
                    .parts
                    .iter()
                    .filter_map(|(name, ducat_item)| {
                        let item_is_part = name.ends_with("Systems")
                            || name.ends_with("Neuroptics")
                            || name.ends_with("Chassis")
                            || name.ends_with("Harness")
                            || name.ends_with("Wings");
                        let drop_name = match equipment_item.item_type {
                            EquipmentType::Warframes | EquipmentType::Archwing
                                if item_is_part && !name.ends_with("Blueprint") =>
                            {
                                name.to_owned() + " Blueprint"
                            }
                            _ => name.to_owned(),
                        };
                        let platinum = *match price_table
                            .get(name)
                            .or_else(|| price_table.get(&format!("{name} Blueprint")))
                        {
                            Some(plat) => plat,
                            None => {
                                println!("Failed to find price for item: {name}");
                                return None;
                            }
                        };
                        let ducats = ducat_item.ducats;

                        Some(Item {
                            name: name.to_string(),
                            drop_name,
                            platinum,
                            ducats,
                        })
                    })
            })
            .chain(filtered_items.ignored_items.keys().map(|name| Item {
                name: name.to_owned(),
                drop_name: name.to_owned(),
                platinum: 0.0,
                ducats: 0,
            }))
            .collect();

        if let Some(item) = items.iter_mut().find(|item| item.name == "Forma Blueprint") {
            item.platinum = 35.0 / 3.0;
        };

        let relics = filtered_items.relics;

        Ok(Database { items, relics })
    }

    pub fn find_item(&self, needle: &str, threshold: Option<usize>) -> Option<&Item> {
        let best_match = self
            .items
            .iter()
            .filter(|item| !item.name.ends_with("Set"))
            .min_by_key(|item| levenshtein(&item.drop_name, needle));

        best_match.and_then(|item| {
            if levenshtein(&item.drop_name.replace(' ', ""), needle)
                <= threshold.unwrap_or(item.drop_name.len() / 3)
            {
                Some(item)
            } else {
                None
            }
        })
    }

    pub fn find_item_exact(&self, needle: &str) -> Result<&Item> {
        self.items.iter().find(|item| item.name == needle)
            .ok_or_else(|| DatabaseError::ItemNotFound(needle.to_string()).into())
    }

    fn relic_to_bucket(&self, relic: &Relic, refinement: Refinement) -> Result<Bucket> {
        let common_chance = refinement.common_chance();
        let uncommon_chance = refinement.uncommon_chance();
        let rare_chance = refinement.rare_chance();

        let item_names = [
            (&relic.common1, common_chance),
            (&relic.common2, common_chance),
            (&relic.common3, common_chance),
            (&relic.uncommon1, uncommon_chance),
            (&relic.uncommon2, uncommon_chance),
            (&relic.rare1, rare_chance),
        ];

        let mut items = Vec::with_capacity(item_names.len());
        for (name, chance) in item_names {
            let item = self.find_item_exact(name)?;
            items.push(statistics::Item {
                value: item.platinum,
                probability: chance,
            });
        }

        Ok(Bucket::new(items))
    }

    pub fn single_relic_value(&self, relic: &Relic, refinement: Refinement) -> f32 {
        let common_chance = refinement.common_chance();
        let uncommon_chance = refinement.uncommon_chance();
        let rare_chance = refinement.rare_chance();

        // Define a helper function to safely get item platinum or log error and return 0.0
        let get_platinum = |name: &str, item_type: &str| -> f32 {
            match self.find_item_exact(name) {
                Ok(item) => item.platinum,
                Err(e) => {
                    eprintln!("Failed to find {} item {}: {}", item_type, name, e);
                    0.0
                }
            }
        };

        let value = 0.0
            + get_platinum(&relic.common1, "common1") * common_chance
            + get_platinum(&relic.common2, "common2") * common_chance
            + get_platinum(&relic.common3, "common3") * common_chance
            + get_platinum(&relic.uncommon1, "uncommon1") * uncommon_chance
            + get_platinum(&relic.uncommon2, "uncommon2") * uncommon_chance
            + get_platinum(&relic.rare1, "rare1") * rare_chance;

        let item_names = [
            (&relic.common1, common_chance),
            (&relic.common2, common_chance),
            (&relic.common3, common_chance),
            (&relic.uncommon1, uncommon_chance),
            (&relic.uncommon2, uncommon_chance),
            (&relic.rare1, rare_chance),
        ];
        let value2: f32 = item_names
            .into_iter()
            .map(|(name, chance)| {
                let plat = match self.find_item_exact(name) {
                    Ok(item) => item.platinum,
                    Err(e) => {
                        eprintln!("Failed to find item {}: {}", name, e);
                        0.0
                    }
                };
                println!("{plat} * {chance}");
                plat * chance
            })
            .sum();
        println!("{value} vs {value2}");

        value
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
                eprintln!("Error calculating relic value: {}", e);
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
        let common_chance = refinement.common_chance();
        let uncommon_chance = refinement.uncommon_chance();
        let rare_chance = refinement.rare_chance();

        let items = [
            (&relic.common1, common_chance),
            (&relic.common2, common_chance),
            (&relic.common3, common_chance),
            (&relic.uncommon1, uncommon_chance),
            (&relic.uncommon2, uncommon_chance),
            (&relic.rare1, rare_chance),
        ];

        let mut value = 0.0;
        for item1 in items.iter() {
            for item2 in items.iter() {
                for item3 in items.iter() {
                    for item4 in items.iter() {
                        // Get platinum values for all items, handling errors
                        let platinum_values: Vec<f32> = [item1.0, item2.0, item3.0, item4.0]
                            .iter()
                            .map(|name| {
                                match self.find_item_exact(name) {
                                    Ok(item) => item.platinum,
                                    Err(e) => {
                                        eprintln!("Failed to find item {}: {}", name, e);
                                        0.0
                                    }
                                }
                            })
                            .collect();

                        // Find maximum value, defaulting to 0.0 if the vector is empty
                        let max_value = platinum_values.iter()
                            .max_by(|a, b| a.total_cmp(b))
                            .copied()
                            .unwrap_or(0.0);

                        value += max_value * item1.1 * item2.1 * item3.1 * item4.1;
                    }
                }
            }
        }

        value
    }
}

fn remove_empty_relics_from_json(value: &mut Value) -> Result<()> {
    let relics = &mut value["relics"];
    let relics_obj = relics.as_object_mut()
        .ok_or_else(|| DatabaseError::InvalidFormat("relics field is not an object".to_string()))?;

    for (_, kind) in relics_obj {
        let kind_obj = kind.as_object_mut()
            .ok_or_else(|| DatabaseError::InvalidFormat("relic kind is not an object".to_string()))?;

        kind_obj.retain(|_name, relic| serde_json::from_value::<Relic>(relic.clone()).is_ok());
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
