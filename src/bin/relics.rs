use std::collections::HashMap;

use wfinfo::{
    database::Database,
    models::item::{Refinement, Relic},
};

fn relic_values(database: &Database, relics: &HashMap<String, Relic>, relic_count: u32) {
    let mut sorted_relics: Vec<(String, Refinement, f32)> = relics
        .iter()
        .filter_map(|(name, item): (&String, &Relic)| {
            let refinements = [
                Refinement::Intact,
                Refinement::Exceptional,
                Refinement::Flawless,
                Refinement::Radiant,
            ];

            // Calculate values for each refinement
            let values: Vec<(Refinement, f32)> = refinements
                .into_iter()
                .map(|refinement| {
                    (
                        refinement,
                        database.shared_relic_value(item, refinement, relic_count),
                    )
                })
                .collect();

            // Find the refinement with the maximum value
            let max_value = values.iter().max_by(|a, b| a.1.total_cmp(&b.1));

            match max_value {
                Some((refinement, value)) => Some((name.to_owned(), *refinement, *value)),
                None => {
                    eprintln!(
                        "Warning: Could not determine best refinement for relic {}",
                        name
                    );
                    None
                }
            }
        })
        .collect();
    sorted_relics.sort_by(|a, b| b.2.total_cmp(&a.2));

    let list_length = 800;
    sorted_relics
        .iter()
        .take(list_length / 2)
        .for_each(|(name, refinement, value)| println!("{}:\t{:?}\t{}", name, refinement, value));
    if sorted_relics.len() > list_length / 2 {
        println!("...");
        sorted_relics
            .iter()
            .rev()
            .take((list_length / 2).min(sorted_relics.len() - (list_length / 2)))
            .rev()
            .for_each(|(name, refinement, value)| {
                println!("{}:\t{:?}\t{}", name, refinement, value)
            });
    }
}

fn best_trace_dump(database: &Database) {
    let mut relics = Vec::new();
    for (prefix, relic_group) in [
        ("Lith", &database.relics.lith),
        ("Meso", &database.relics.meso),
        ("Neo", &database.relics.neo),
        ("Axi", &database.relics.axi),
    ] {
        for (name, relic) in relic_group.iter() {
            let intact = database.shared_relic_value(relic, Refinement::Intact, 4);
            let radiant = database.shared_relic_value(relic, Refinement::Radiant, 4);
            relics.push((format!("{prefix} {name}"), radiant - intact));
        }
    }

    let mut sorted_relics = relics;
    sorted_relics.sort_by(|a, b| a.1.total_cmp(&b.1));

    println!("...");
    sorted_relics
        .iter()
        .for_each(|(name, value)| println!("{}:  \t{}", name, value));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load the database
    let database = Database::load_from_file(None, None)
        .map_err(|e| format!("Error loading database: {}", e))?;

    let mut args = std::env::args().skip(1);

    // Get the relic type from arguments
    let relic_type = match args.next() {
        Some(arg) => arg,
        None => {
            eprintln!("Usage: relics <relic_type> [relic_count]");
            eprintln!("  relic_type: lith, meso, neo, axi, or tracedump");
            eprintln!("  relic_count: number of relics (default: 4)");
            return Err("No relic type provided".into());
        }
    };

    // Process based on relic type
    let relics = match relic_type.to_lowercase().as_str() {
        "lith" => &database.relics.lith,
        "meso" => &database.relics.meso,
        "neo" => &database.relics.neo,
        "axi" => &database.relics.axi,
        "tracedump" => {
            best_trace_dump(&database);
            return Ok(());
        }
        s => {
            eprintln!("Usage: relics <relic_type> [relic_count]");
            eprintln!("  relic_type: lith, meso, neo, axi, or tracedump");
            return Err(format!("Invalid relic type: {}", s).into());
        }
    };

    // Get the relic count from arguments
    let relic_count: u32 = match args.next() {
        Some(count_str) => match count_str.parse() {
            Ok(count) => count,
            Err(e) => {
                return Err(format!("Failed to parse relic count '{}': {}", count_str, e).into());
            }
        },
        None => 4, // Default value
    };

    // Calculate and display relic values
    relic_values(&database, relics, relic_count);

    Ok(())
}
