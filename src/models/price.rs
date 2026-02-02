use serde::Deserialize;
use serde_aux::prelude::deserialize_number_from_string;

#[derive(Clone, Debug, Deserialize)]
pub struct PriceItem {
    pub name: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub custom_avg: f32,
}
