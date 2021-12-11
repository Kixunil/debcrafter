use std::convert::TryFrom;
use crate::types::package_name::VPackageName;
use std::borrow::Cow;
use crate::types::package_name::VPackageNameError;
use serde_derive::Deserialize;
use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(try_from = "String")]
pub enum VarName<'a> {
    Internal(Cow<'a, str>),
    Absolute(VPackageName, Cow<'a, str>),
    Constant(Cow<'a, str>),
}

impl<'a> VarName<'a> {
    pub fn expand<'b>(&'b self, this_package: &'b str, variant: Option<&'b super::Variant>) -> Result<DisplayVar<'b>, &'b str> {
        match self {
            VarName::Internal(variable) => Ok(DisplayVar { package: Cow::Borrowed(this_package), variable, }),
            VarName::Absolute(package, variable) => Ok(DisplayVar { package: package.expand_to_cow(variant), variable, }),
            VarName::Constant(constant) => Err(constant),
        }
    }
}

impl TryFrom<String> for VarName<'static> {
    type Error = Error;

    fn try_from(mut string: String) -> Result<Self, Self::Error> {
        match string.find('/') {
            Some(0) => {
                string.remove(0);
                Ok(VarName::Internal(Cow::Owned(string)))
            },
            Some(pos) => {
                let pkg_name = VPackageName::try_from(&string[..pos]).map_err(Error)?;
                let var_name = Cow::Owned(string[(pos + 1)..].to_owned());
                Ok(VarName::Absolute(pkg_name, var_name))
            },
            None => {
                Ok(VarName::Constant(Cow::Owned(string)))
            }
        }
    }
}

impl<'a> TryFrom<&'a str> for VarName<'a> {
    type Error = Error;

    fn try_from(string: &'a str) -> Result<Self, Self::Error> {
        match string.find('/') {
            Some(0) => {
                Ok(VarName::Internal(Cow::Borrowed(&string[1..])))
            },
            Some(pos) => {
                let pkg_name = VPackageName::try_from(&string[..pos]).map_err(Error)?;
                let var_name = Cow::Borrowed(&string[(pos + 1)..]);
                Ok(VarName::Absolute(pkg_name, var_name))
            },
            None => {
                Ok(VarName::Constant(Cow::Borrowed(string)))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct Error(VPackageNameError);

pub struct DisplayVar<'a> {
    package: Cow<'a, str>,
    variable: &'a str,
}

impl<'a> fmt::Display for DisplayVar<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.package, self.variable)
    }
}
