use std::convert::TryFrom;
use serde_derive::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(try_from = "String")]
pub struct Variant(String);

impl Variant {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Variant {
    type Error = VariantError;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        for c in string.chars() {
            if (c < 'a' || c > 'z') && (c < '0' || c > '9') {
                return Err(VariantError { c, string });
            }
        }
        Ok(Variant(string))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid character {c} in variant {string}")]
pub struct VariantError {
    c: char,
    string: String,
}
