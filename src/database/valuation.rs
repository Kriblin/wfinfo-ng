use log::{error, warn};

use crate::{
    error::Result,
    models::item::{Refinement, Relic},
    statistics::{self, Bucket},
};

use super::Database;

impl Database {
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
                let platinum = match self.find_item_exact(name) {
                    Ok(item) => item.platinum,
                    Err(err) => {
                        warn!("Failed to find item {name}: {err}");
                        0.0
                    }
                };
                platinum * refinement.chance(reward_type)
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
            Err(err) => {
                error!("Error calculating relic value: {err}");
                0.0
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
                let platinum = match self.find_item_exact(name) {
                    Ok(item) => item.platinum,
                    Err(err) => {
                        warn!("Failed to find item {name}: {err}");
                        0.0
                    }
                };
                (platinum, refinement.chance(reward_type))
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
