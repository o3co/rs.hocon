//! `java.util.Properties` files as HOCON config.

use super::{config_from_object, AdapterError};
use crate::properties::properties_to_hocon;
use crate::Config;

/// Read Properties-syntax text.
///
/// Shares its syntax layer with `include "x.properties"`, so the two cannot
/// drift apart. Values are all strings, and a `${a.b}` among them stays that
/// literal text (spec F0.2, F2.2).
pub fn parse(input: &str, origin: Option<&str>) -> Result<Config, AdapterError> {
    let value = properties_to_hocon(input).map_err(AdapterError::new)?;
    Ok(config_from_object(value, origin))
}

/// Read a Properties file, using its path as the origin description.
pub fn parse_file(path: impl AsRef<std::path::Path>) -> Result<Config, AdapterError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path)
        .map_err(|e| AdapterError::new(format!("properties: {}: {e}", path.display())))?;
    parse(&text, Some(&path.display().to_string()))
}
