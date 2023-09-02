use anyhow::Result;
use serde::Serialize;
use std::{
    collections::hash_map::DefaultHasher,
    fmt::Display,
    hash::{Hash, Hasher},
};
/// The unique identifier associated with a NFC tag
#[derive(Debug, Clone, Serialize)]
pub struct TagId(String);

impl AsRef<str> for TagId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for TagId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TagId:{:#}", self.0)
    }
}

impl TagId {
    pub fn new(name: &str) -> Result<Self> {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);

        Ok(TagId(format!("{:x}", hasher.finish())))
    }
}
