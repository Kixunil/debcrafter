use std::borrow::Cow;
use std::convert::TryFrom;
use crate::template::TemplateString;
use crate::types::{VPackageName, Variant, NonEmptyMap};

mod base;
mod service;
mod conf_ext;

pub use base::BasePackageSpec;
pub use service::{ServicePackageSpec, ConfParam, ServiceInstance};
pub use conf_ext::ConfExtPackageSpec;

pub use crate::input::{Plug, FileDeps, Migration, MigrationVersion, Database, ExtraGroup, Architecture, RuntimeDir, BoolOrVecTemplateString, ConfDir, UserSpec, CreateUser, Config, ConfType, DebconfPriority, DirRepr, GeneratedType, VarType, FileVar, FileType, ConfFormat, DbConfig, HiddenVarVal, Alternative, PostProcess};
use super::{Map, Set};

pub trait PackageConfig {
    fn config(&self) -> &Map<TemplateString, Config>;
}

impl<'a, T> PackageConfig for &'a T where T: PackageConfig {
    fn config(&self) -> &Map<TemplateString, Config> {
        (*self).config()
    }
}

impl<'a> PackageConfig for PackageInstance<'a> {
    fn config(&self) -> &Map<TemplateString, Config> {
        &self.config
    }
}

impl<'a> PackageConfig for ServiceInstance<'a> {
    fn config(&self) -> &Map<TemplateString, Config> {
        &self.config
    }
}

pub struct Package {
    pub name: VPackageName,
    pub summary: TemplateString,
    pub long_doc: Option<TemplateString>,
    pub map_variants: Map<String, Map<Variant, String>>,
    pub spec: PackageSpec,
    pub config: Map<TemplateString, Config>,
    pub databases: Map<Database, DbConfig>,
    pub depends: Set<TemplateString>,
    pub provides: Set<TemplateString>,
    pub recommends: Set<TemplateString>,
    pub suggests: Set<TemplateString>,
    pub conflicts: Set<TemplateString>,
    pub extended_by: Set<TemplateString>,
    pub add_files: Vec<TemplateString>,
    pub add_dirs: Vec<TemplateString>,
    pub add_links: Vec<TemplateString>,
    pub add_manpages: Vec<String>,
    pub alternatives: Map<String, Alternative>,
    pub patch_foreign: Map<String, String>,
    pub extra_triggers: Set<TemplateString>,
    pub migrations: Map<MigrationVersion, Migration>,
    pub plug: Vec<Plug>,
}

impl Package {
    pub fn instantiate<'a>(&'a self, variant: Option<&'a Variant>, includes: Option<&'a Map<VPackageName, Package>>) -> PackageInstance<'a> {
        let name = self.name.expand_to_cow(variant);

        PackageInstance {
            name,
            variant,
            map_variants: &self.map_variants,
            summary: &self.summary,
            long_doc: self.long_doc.as_ref(),
            spec: &self.spec,
            config: &self.config,
            databases: &self.databases,
            includes,
            depends: &self.depends,
            provides: &self.provides,
            recommends: &self.recommends,
            suggests: &self.suggests,
            conflicts: &self.conflicts,
            extended_by: &self.extended_by,
            add_files: &self.add_files,
            add_dirs: &self.add_dirs,
            add_links: &self.add_links,
            add_manpages: &self.add_manpages,
            alternatives: &self.alternatives,
            patch_foreign: &self.patch_foreign,
            extra_triggers: &self.extra_triggers,
            migrations: &self.migrations,
            plug: self.plug.as_ref(),
        }
    }
}

impl TryFrom<crate::input::Package> for Package {
    type Error = PackageError;

    fn try_from(value: crate::input::Package) -> Result<Self, Self::Error> {
        use crate::input;

        let (spec, summary, long_doc, config, databases, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign) = match value.spec {
            input::PackageSpec::Base(input::BasePackageSpec { architecture, config, summary, long_doc, databases, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign, }) => (PackageSpec::Base(BasePackageSpec { architecture, }), summary, long_doc, config, databases, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign),
            input::PackageSpec::Service(input::ServicePackageSpec { bin_package, min_patch, binary, bare_conf_param, conf_param, conf_d, user, config, condition_path_exists, service_type, exec_stop, after, before, wants, requires, binds_to, part_of, wanted_by, refuse_manual_start, refuse_manual_stop, runtime_dir, extra_service_config, summary, long_doc, databases, extra_groups, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign, allow_suid_sgid, }) => (PackageSpec::Service(ServicePackageSpec { bin_package, min_patch, binary, conf_param: ConfParam::from_input(conf_param, bare_conf_param), conf_d, user, condition_path_exists, service_type, exec_stop, after, before, wants, requires, binds_to, part_of, wanted_by, refuse_manual_start, refuse_manual_stop, runtime_dir, extra_service_config, allow_suid_sgid, extra_groups, }), summary, long_doc, config, databases, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign),
            input::PackageSpec::ConfExt(input::ConfExtPackageSpec { extends, replaces, depends_on_extended, min_patch, external, config, summary, long_doc, databases, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign, extra_groups, }) => (PackageSpec::ConfExt(ConfExtPackageSpec { extends, replaces, depends_on_extended, min_patch, external, extra_groups, }), summary, long_doc, config, databases, add_files, add_dirs, add_links, add_manpages, alternatives, patch_foreign),
        };

        Ok(Package {
            name: value.name,
            map_variants: value.map_variants,
            summary: summary.expect("missing summary"),
            long_doc,
            spec,
            config,
            databases,
            depends: value.depends,
            provides: value.provides,
            recommends: value.recommends,
            suggests: value.suggests,
            conflicts: value.conflicts,
            extended_by: value.extended_by,
            add_files,
            add_dirs,
            add_links,
            add_manpages,
            alternatives,
            patch_foreign,
            extra_triggers: value.extra_triggers,
            migrations: value.migrations,
            plug: value.plug,
        })
    }
}

#[derive(Debug)]
pub enum PackageError {
}

impl PackageConfig for Package {
    fn config(&self) -> &Map<TemplateString, Config> {
        &self.config
    }
}

pub struct PackageInstance<'a> {
    pub name: Cow<'a, str>,
    pub variant: Option<&'a Variant>,
    pub map_variants: &'a Map<String, Map<Variant, String>>,
    pub summary: &'a TemplateString,
    pub long_doc: Option<&'a TemplateString>,
    pub spec: &'a PackageSpec,
    pub config: &'a Map<TemplateString, Config>,
    pub databases: &'a Map<Database, DbConfig>,
    pub includes: Option<&'a Map<VPackageName, Package>>,
    pub depends: &'a Set<TemplateString>,
    pub provides: &'a Set<TemplateString>,
    pub recommends: &'a Set<TemplateString>,
    pub suggests: &'a Set<TemplateString>,
    pub conflicts: &'a Set<TemplateString>,
    pub extended_by: &'a Set<TemplateString>,
    pub add_files: &'a [TemplateString],
    pub add_dirs: &'a [TemplateString],
    pub add_links: &'a [TemplateString],
    pub add_manpages: &'a [String],
    pub alternatives: &'a Map<String, Alternative>,
    pub patch_foreign: &'a Map<String, String>,
    pub extra_triggers: &'a Set<TemplateString>,
    pub migrations: &'a Map<MigrationVersion, Migration>,
    pub plug: &'a [Plug],
}

impl<'a> PackageInstance<'a> {
    pub fn as_service<'b>(&'b self) -> Option<ServiceInstance<'b>> {
        if let PackageSpec::Service(service) = &self.spec {
            Some(ServiceInstance {
                name: &self.name,
                variant: self.variant,
                map_variants: &self.map_variants,
                summary: &self.summary,
                spec: service,
                config: self.config,
                databases: self.databases,
                includes: self.includes,
            })
        } else {
            None
        }
    }
}

pub trait PackageOps<'a>: PackageConfig {
    fn config_pkg_name(&self) -> &str;
    fn variant(&self) -> Option<&Variant>;
    fn constants_by_variant(&self) -> ConstantsByVariant<'_>;
    fn config_sub_dir(&self) -> Cow<'a, str>;
    fn internal_config_sub_dir(&self) -> Cow<'a, str>;
    fn service_name(&self) -> Option<&str>;
    fn service_user(&self) -> Option<Cow<'_, str>>;
    fn service_group(&self) -> Option<Cow<'_, str>>;
    fn extra_groups(&self) -> Option<NonEmptyMap<TemplateString, ExtraGroup, &'_ Map<TemplateString, ExtraGroup>>>;
    fn get_include(&self, name: &VPackageName) -> Option<&Package>;
    fn is_conf_ext(&self) -> bool;
    fn conf_dir(&self) -> Option<&str>;
    fn databases(&self) -> &Map<Database, DbConfig>;
}

impl<'a> PackageOps<'a> for PackageInstance<'a> {
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
        if let PackageSpec::ConfExt(confext) = &self.spec {
            if confext.external {
                "/".into()
            } else {
                self
                    .get_include(&confext.extends)
                    .unwrap_or_else(|| panic!("Package {} extended by {} not found", confext.extends.expand_to_cow(self.variant), self.name))
                    .instantiate(self.variant, None)
                    .config_sub_dir()
                    .into_owned()
                    .into()
            }
        } else {
            self.name.clone().into_owned().into()
        }
    }

    fn internal_config_sub_dir(&self) -> Cow<'a, str> {
        if let PackageSpec::ConfExt(confext) = &self.spec {
            if confext.external {
                "/".into()
            } else {
                self
                    .get_include(&confext.extends)
                    .unwrap_or_else(|| panic!("Package {} extended by {} not found", confext.extends.expand_to_cow(self.variant), self.name))
                    .instantiate(self.variant, None)
                    .config_sub_dir()
                    .into_owned()
                    .into()
            }
        } else {
            self.name.clone().into_owned().into()
        }
    }

    fn service_name(&self) -> Option<&str> {
        if let PackageSpec::Service(_) = &self.spec {
            Some(&self.name)
        } else {
            None
        }
    }

    fn service_user(&self) -> Option<Cow<'_, str>> {
        self.as_service().map(|service| service.user_name()).or_else(|| if let PackageSpec::ConfExt(confext) = &self.spec {
            if confext.depends_on_extended && !confext.external {
                self
                    .get_include(&confext.extends)
                    .unwrap_or_else(|| panic!("Package {} extended by {} not found", confext.extends.expand_to_cow(self.variant), self.name))
                    .instantiate(self.variant, None)
                    .service_user()
                    .map(|user| Cow::Owned(String::from(user)))
            } else {
                None
            }
        } else {
            None
        })
    }

    fn service_group(&self) -> Option<Cow<'_, str>> {
        self.as_service().and_then(|service| ServiceInstance::service_group(&service)).or_else(|| if let PackageSpec::ConfExt(confext) = &self.spec {
            if confext.depends_on_extended && !confext.external {
                self
                    .get_include(&confext.extends)
                    .unwrap_or_else(|| panic!("Package {} extended by {} not found", confext.extends.expand_to_cow(self.variant), self.name))
                    .instantiate(self.variant, None)
                    .service_group()
                    .map(|group| Cow::Owned(String::from(group)))
            } else {
                None
            }
        } else {
            None
        })
    }

    fn extra_groups(&self) -> Option<NonEmptyMap<TemplateString, ExtraGroup, &'_ Map<TemplateString, ExtraGroup>>> {
        match &self.spec {
            PackageSpec::Service(service) => NonEmptyMap::from_map(&service.extra_groups),
            PackageSpec::ConfExt(confext) => {
                let groups = NonEmptyMap::from_map(&confext.extra_groups);
                if groups.is_some() && !confext.depends_on_extended {
                    // TODO: implement permission system and check if groups exist as well
                    panic!("The configuration extension {} doesn't depent on extended package yet it wants to add the user to a group. The user is not guaranteed to exist.", self.name);
                }
                groups
            },
            PackageSpec::Base(_) => None,
        }
    }

    fn get_include(&self, name: &VPackageName) -> Option<&Package> {
        self.includes.as_ref().and_then(|includes| includes.get(name))
    }

    fn is_conf_ext(&self) -> bool {
        if let PackageSpec::ConfExt(_) = &self.spec {
            true
        } else {
            false
        }
    }

    fn conf_dir(&self) -> Option<&str> {
        self.as_service().and_then(|service| service.spec.conf_d.as_ref().map(|conf_d| conf_d.name.as_ref()))
    }

    fn databases(&self) -> &Map<Database, DbConfig> {
        &self.databases
    }
}

pub struct ConstantsByVariant<'a> {
    variant: Option<&'a Variant>,
    constants: &'a Map<String, Map<Variant, String>>,
}

impl<'a> ConstantsByVariant<'a> {
    pub fn get_variant(&self) -> Option<&'a Variant> {
        self.variant
    }
}

impl<'a> crate::template::Query for ConstantsByVariant<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        if key == "variant" {
            self.variant.map(Variant::as_str)
        } else {
            self.constants.get(key)?.get(self.variant?).map(AsRef::as_ref)
        }
    }
}

pub enum PackageSpec {
    Service(ServicePackageSpec),
    ConfExt(ConfExtPackageSpec),
    Base(BasePackageSpec),
}
