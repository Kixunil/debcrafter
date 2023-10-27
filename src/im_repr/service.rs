use crate::im_repr::{ConfDir, RuntimeDir, ExtraGroup, UserSpec};
use crate::template::TemplateString;
use crate::Map;
use crate::types::{VPackageName, Variant, NonEmptyMap};
use super::{PackageOps, Config, DbConfig, Database, Package, ConstantsByVariant};
use std::borrow::Cow;

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

pub struct ServiceInstance<'a> {
    pub name: &'a Cow<'a, str>,
    pub variant: Option<&'a Variant>,
    pub map_variants: &'a Map<String, Map<Variant, String>>,
    pub summary: &'a TemplateString,
    pub spec: &'a ServicePackageSpec,
    pub config: &'a Map<TemplateString, Config>,
    pub databases: &'a Map<Database, DbConfig>,
    pub includes: Option<&'a Map<VPackageName, Package>>,
}

impl<'a> ServiceInstance<'a> {
    pub fn user_name(&self) -> Cow<'a, str> {
        self.spec.user.name.as_ref().map(|user_name| user_name.expand_to_cow(self.constants_by_variant())).unwrap_or(Cow::Borrowed(&self.name.as_ref()))
    }

    pub fn service_name(&self) -> &'a str {
        &**self.name
    }

    pub fn service_group(&self) -> Option<Cow<'a, str>> {
        if self.spec.user.group {
            Some(self.spec.user.name.as_ref().map(|user_name| user_name.expand_to_cow(self.constants_by_variant())).unwrap_or(Cow::Borrowed(&self.name.as_ref())))
        } else {
            None
        }
    }
}

impl<'a> PackageOps<'a> for ServiceInstance<'a> {
    fn config_pkg_name(&self) -> &str {
        &self.name
    }

    fn variant(&self) -> Option<&Variant> {
        self.variant
    }

    fn constants_by_variant(&self) -> ConstantsByVariant<'_> {
        ConstantsByVariant {
            variant: self.variant,
            constants: self.map_variants,
        }
    }

    fn config_sub_dir(&self) -> Cow<'a, str> {
        (&**self.name).into()
    }

    fn internal_config_sub_dir(&self) -> Cow<'a, str> {
        (&**self.name).into()
    }

    fn service_name(&self) -> Option<&str> {
        Some(ServiceInstance::service_name(self))
    }

    fn service_user(&self) -> Option<Cow<'_, str>> {
        Some(self.user_name())
    }

    fn service_group(&self) -> Option<Cow<'_, str>> {
        if self.spec.user.group {
            Some(self.user_name())
        } else {
            None
        }
    }

    fn extra_groups(&self) -> Option<NonEmptyMap<TemplateString, ExtraGroup, &'_ Map<TemplateString, ExtraGroup>>> {
        NonEmptyMap::from_map(&self.spec.extra_groups)
    }

    fn get_include(&self, name: &VPackageName) -> Option<&Package> {
        self.includes.as_ref().and_then(|includes| includes.get(name))
    }

    fn is_conf_ext(&self) -> bool {
        false
    }

    fn conf_dir(&self) -> Option<&str> {
        self.spec.conf_d.as_ref().map(|conf_d| conf_d.name.as_ref())
    }

    fn databases(&self) -> &Map<Database, DbConfig> {
        &self.databases
    }
}

