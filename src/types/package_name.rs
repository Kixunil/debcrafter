use std::borrow::Cow;
use std::convert::TryFrom;
use std::path::{Path, PathBuf};
use serde_derive::Deserialize;

const PKG_NAME_VARIANT_SUFFIX: &str = "-@variant";

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(try_from = "String")]
pub struct VPackageName(String);

impl VPackageName {
    fn _base(string: &str) -> &str {
        if string.ends_with(PKG_NAME_VARIANT_SUFFIX) {
            &string[..(string.len() - PKG_NAME_VARIANT_SUFFIX.len())]
        } else {
            &string
        }
    }

    pub fn is_templated(&self) -> bool {
        self.0.ends_with(PKG_NAME_VARIANT_SUFFIX)
    }

    fn base(&self) -> &str {
        Self::_base(&self.0)
    }

    pub fn sps_path(&self, parent_dir: &Path) -> PathBuf {
        let mut path = parent_dir.join(&self.0);
        path.set_extension("sps");
        path
    }

    pub fn expand_to_cow(&self, variant: Option<&super::Variant>) -> Cow<'_, str> {
        match (variant, self.0.ends_with(PKG_NAME_VARIANT_SUFFIX)) {
            (None, true) => panic!("can't expand {}: missing variant", self.0),
            (Some(variant), true) => Cow::Owned([self.base(), variant.as_str()].join("-")),
            // We intentionally allow NOT expanding.
            // Not all packages need to be expanded. The validity will be checked anyway.
            (Some(_), false) | (None, false) => Cow::Borrowed(&self.0),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid character {c} in package name {string}")]
pub struct VPackageNameError {
    c: char,
    string: String,
}

impl TryFrom<String> for VPackageName {
    type Error = VPackageNameError;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        for c in Self::_base(&string).chars() {
            if c != '-' && (c < 'a' || c > 'z') && (c < '0' || c > '9') {
                return Err(VPackageNameError { c, string });
            }
        }
        Ok(VPackageName(string))
    }
}

