use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq)]
pub enum HoconValue {
    Object(IndexMap<String, HoconValue>),
    Array(Vec<HoconValue>),
    Scalar(ScalarValue),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}
