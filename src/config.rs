// config.rs

use std::collections::HashMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub base_url: String,
    pub endpoints: Vec<String>,
    pub trading_pairs: Vec<String>,
    pub topics: HashMap<String, Vec<String>>,
}

