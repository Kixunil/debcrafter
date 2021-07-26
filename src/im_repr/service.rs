use crate::input::{ConfDir, RuntimeDir, ExtraGroup, UserSpec};
use crate::template::TemplateString;
use crate::Map;

pub struct ServicePackageSpec {
    pub bin_package: String,
    pub min_patch: Option<String>,
    pub binary: String,
    pub bare_conf_param: bool,
    pub conf_param: Option<String>,
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
