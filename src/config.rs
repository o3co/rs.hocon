use indexmap::IndexMap;
use crate::value::HoconValue;
use crate::error::ConfigError;

pub struct Config {
    root: IndexMap<String, HoconValue>,
}

impl Config {
    pub fn new(root: IndexMap<String, HoconValue>) -> Self {
        Self { root }
    }
}
