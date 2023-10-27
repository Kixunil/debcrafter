use crate::im_repr::{BoolOrVecTemplateString, ExtraGroup};
use crate::types::VPackageName;
use crate::template::TemplateString;
use crate::Map;

pub struct ConfExtPackageSpec {
    pub extends: VPackageName,
    pub replaces: BoolOrVecTemplateString,
    pub depends_on_extended: bool,
    pub min_patch: Option<String>,
    pub external: bool,
    pub extra_groups: Map<TemplateString, ExtraGroup>,
}
