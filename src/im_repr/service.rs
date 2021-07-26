use crate::input::{ConfDir, RuntimeDir, ExtraGroup, UserSpec};
use crate::template::TemplateString;
use crate::Map;

pub enum ConfParam {
    WithSpace(String),
    WithoutSpace(String),
    Bare,
}

impl ConfParam {
    pub fn from_input(conf_param: Option<String>, bare: bool) -> Option<Self> {
        match (conf_param, bare) {
            (None, false) => None,
            (None, true) => Some(ConfParam::Bare),
            (Some(param), false) if param.ends_with('=') => Some(ConfParam::WithoutSpace(param)),
            (Some(param), false) => Some(ConfParam::WithSpace(param)),
            (Some(_), true) => panic!("Can not use both conf_param and bare_conf_param"),
        }
    }

    pub fn param(&self) -> &str {
        match self {
            ConfParam::WithSpace(param) | ConfParam::WithoutSpace(param) => &param,
            ConfParam::Bare => "",
        }
    }

    pub fn separator(&self) -> &str {
        match self {
            ConfParam::WithSpace(_) => " ",
            ConfParam::WithoutSpace(_) | ConfParam::Bare => "",
        }
    }
}

pub struct ServicePackageSpec {
    pub bin_package: String,
    pub min_patch: Option<String>,
    pub binary: String,
    pub conf_param: Option<ConfParam>,
    pub conf_d: Option<ConfDir>,
    pub user: UserSpec,
    pub condition_path_exists: Option<TemplateString>,
    pub service_type: Option<String>,
    pub exec_stop: Option<String>,
    pub after: Option<TemplateString>,
    pub before: Option<TemplateString>,
    pub wants: Option<TemplateString>,
    pub requires: Option<TemplateString>,
    pub binds_to: Option<TemplateString>,
    pub part_of: Option<TemplateString>,
    pub wanted_by: Option<TemplateString>,
    pub refuse_manual_start: bool,
    pub refuse_manual_stop: bool,
    pub runtime_dir: Option<RuntimeDir>,
    pub extra_service_config: Option<TemplateString>,
    pub extra_groups: Map<TemplateString, ExtraGroup>,
    pub allow_suid_sgid: bool,
}
