pub mod cache;
mod load;
mod matching;
mod valuation;

use serde::Deserialize;

use crate::models::item::Relics;

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

#[cfg(test)]
mod test {
    use approx::assert_relative_eq;

    use crate::models::item::Refinement;

    use super::*;

    #[test]
    pub fn can_load_database() {
        Database::load_from_file(None, None).expect("Failed to load database");
    }

    #[test]
    pub fn can_find_items() {
        let db = Database::load_from_file(None, None).expect("Failed to load database");

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
        let db = Database::load_from_file(None, None).expect("Failed to load database");

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
        let database = Database::load_from_file(None, None).expect("Failed to load database");

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
