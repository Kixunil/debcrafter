use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt;
use std::path::{Path, PathBuf};
use serde_derive::Deserialize;
use super::Spanned;

const PKG_NAME_VARIANT_SUFFIX: &str = "-@variant";

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
#[serde(try_from = "String")]
pub struct VPackageName(String);

impl VPackageName {
    fn _base(string: &str) -> &str {
        string.strip_suffix(PKG_NAME_VARIANT_SUFFIX).unwrap_or(string)
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

    pub fn as_raw(&self) -> &str {
        &self.0
    }

    fn parse(string: impl std::ops::Deref<Target=str> + Into<String>) -> Result<Self, VPackageNameError> {
        let mut invalid_chars = Vec::new();
        for (i, c) in Self::_base(&string).char_indices() {
            if c != '-' && (c < 'a' || c > 'z') && (c < '0' || c > '9') {
                invalid_chars.push(i);
            }
        }
        if invalid_chars.is_empty() {
            Ok(VPackageName(string.into()))
        } else {
            Err(VPackageNameError { invalid_chars, string: string.into() })
        }
    }
}

#[derive(Debug)]
pub struct VPackageNameError {
    pub(crate) invalid_chars: Vec<usize>,
    pub(crate) string: String,
}

impl fmt::Display for VPackageNameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid char '{}' in package name '{}'", self.invalid_chars.first().unwrap(), self.string)
    }
}

impl std::error::Error for VPackageNameError { }

impl TryFrom<String> for VPackageName {
    type Error = VPackageNameError;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        Self::parse(string)
    }
}

impl<'a> TryFrom<&'a str> for VPackageName {
    type Error = VPackageNameError;

    fn try_from(string: &'a str) -> Result<Self, Self::Error> {
        Self::parse(string)
    }
}

impl TryFrom<toml::Spanned<String>> for VPackageName {
    type Error = Spanned<VPackageNameError>;

    fn try_from(string: toml::Spanned<String>) -> Result<Self, Self::Error> {
        let (span_start, span_end) = string.span();
        Self::parse(string.into_inner()).map_err(|error| Spanned { value: error, span_start, span_end })
    }
}

impl TryFrom<Spanned<String>> for VPackageName {
    type Error = Spanned<VPackageNameError>;

    fn try_from(Spanned { value, span_start, span_end }: Spanned<String>) -> Result<Self, Self::Error> {
        Self::parse(value).map_err(|error| {
            Spanned {
                value: error,
                span_start,
                span_end,
            }
        })
    }
}

impl<'a> TryFrom<Spanned<&'a str>> for VPackageName {
    type Error = Spanned<VPackageNameError>;

    fn try_from(string: Spanned<&'a str>) -> Result<Self, Self::Error> {
        Self::parse(string.value).map_err(|error| {
            Spanned {
                value: error,
                span_start: string.span_start,
                span_end: string.span_end,
            }
        })
    }
}
