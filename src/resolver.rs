use std::collections::HashMap;
use crate::error::ResolveError;
use crate::parser::AstNode;
use crate::value::HoconValue;

pub fn resolve(_ast: AstNode, _env: &HashMap<String, String>) -> Result<HoconValue, ResolveError> {
    Ok(HoconValue::Object(indexmap::IndexMap::new()))
}
