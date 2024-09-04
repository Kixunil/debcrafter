use crate::template::{self, TemplateString};
use crate::types::{NonEmptyMap, Spanned, VPackageName, VPackageNameError, Variant};
use indexmap::IndexMap as OrderedHashMap;
use std::borrow::Cow;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::path::{Path, PathBuf};

mod base;
mod conf_ext;
mod service;
mod vars;

pub use base::BasePackageSpec;
pub use conf_ext::ConfExtPackageSpec;
pub use service::{ConfParam, ServiceInstance, ServicePackageSpec};
pub use vars::{
    ExternalVar, HiddenVar, HiddenVarVal, InternalVar, InternalVarCondition, PathVar, VarType,
};

pub use crate::input::{
    Architecture, BoolOrVecTemplateString, ConfFormat, Database, DebconfPriority, DirRepr,
    FileDeps, FileType, FileVar,
};

use super::{Map, Set};
use crate::types::DynVarName;

macro_rules! require_fields {
    ($struct:expr, $($field:ident),+ $(,)?) => {
        {
            let mut missing_fields = Vec::new();
        $(
            if $struct.$field.is_none() {
                missing_fields.push(stringify!($field));
            }
        )+
            if !missing_fields.is_empty() {
                return Err(PackageError::MissingFields($struct.span, missing_fields));
            }
        }
        $(
            let $field = $struct.$field.unwrap();
        )+
    }
}

/*
macro_rules! require_fields_one_of {
    ($struct:expr, $name:expr, $([$($field:ident),+ $(,)?] => $code:block),+ $(,)?) => {
        {
            let mut found = None;
            $(
            if let ($(Some($field)),+) = ($($struct.$field),+) {
                if found.is_some() {
                    return Err(PackageError::Ambiguous($name));
                }
                found = Some($code);
            }
            )+
            found.ok_or(&[$([$($field),+]),+])?
        }
    }
}
*/

pub(crate) use require_fields;

pub trait PackageConfig {
    fn config(&self) -> &Map<TemplateString, Config>;
}

impl<'a, T> PackageConfig for &'a T
where
    T: PackageConfig,
{
    fn config(&self) -> &Map<TemplateString, Config> {
        (*self).config()
    }
}

impl<'a> PackageConfig for PackageInstance<'a> {
    fn config(&self) -> &Map<TemplateString, Config> {
        self.config
    }
}

impl<'a> PackageConfig for ServiceInstance<'a> {
    fn config(&self) -> &Map<TemplateString, Config> {
        self.config
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
    pub import_files: Vec<[TemplateString; 2]>,
    pub add_dirs: Vec<TemplateString>,
    pub add_links: Vec<TemplateString>,
    pub add_manpages: Vec<String>,
    pub alternatives: Map<String, Alternative>,
    pub patch_foreign: Map<String, String>,
    pub extra_triggers: Set<TemplateString>,
    pub migrations: Map<MigrationVersion, Migration>,
    pub plug: Vec<Plug>,
    pub custom_postrm_script: Option<TemplateString>,
}

impl Package {
    pub fn instantiate<'a>(
        &'a self,
        variant: Option<&'a Variant>,
        includes: Option<&'a Map<VPackageName, Package>>,
    ) -> PackageInstance<'a> {
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
            import_files: &self.import_files,
            add_dirs: &self.add_dirs,
            add_links: &self.add_links,
            add_manpages: &self.add_manpages,
            alternatives: &self.alternatives,
            patch_foreign: &self.patch_foreign,
            extra_triggers: &self.extra_triggers,
            migrations: &self.migrations,
            plug: self.plug.as_ref(),
            custom_postrm_script: self.custom_postrm_script.as_ref(),
        }
    }

    pub fn load_includes<P: AsRef<Path>>(
        &self,
        dir: P,
        mut deps: Option<&mut Set<PathBuf>>,
    ) -> Map<VPackageName, Package> {
        let mut result = Map::new();
        for conf in self.config.values() {
            if let ConfType::Dynamic { evars, .. } = &conf.conf_type {
                for pkg in evars.keys() {
                    let deps = deps.as_deref_mut();
                    result
                        .entry(pkg.value.to_owned())
                        .or_insert_with(load_include(dir.as_ref(), pkg, deps));
                }
            }
        }

        if let PackageSpec::ConfExt(ConfExtPackageSpec {
            extends,
            external: false,
            ..
        }) = &self.spec
        {
            result
                .entry(extends.clone())
                .or_insert_with(load_include(dir.as_ref(), extends, deps));
        }

        result
    }
}

#[derive(Default)]
struct MissingVars {
    internal: Vec<(Spanned<String>, Option<std::ops::Range<usize>>)>,
    external: Vec<Spanned<String>>,
    any: Vec<(Spanned<String>, Option<std::ops::Range<usize>>)>,
}

impl MissingVars {
    fn push_internal(&mut self, var: Spanned<String>) {
        self.internal.push((var, None))
    }

    fn push_external(&mut self, var: Spanned<String>) {
        self.external.push(var)
    }

    fn push_any(&mut self, var: Spanned<String>) {
        self.any.push((var, None))
    }

    fn check_internal<S: AsRef<str>>(&mut self, var: &Spanned<S>) {
        for error in &mut self.internal {
            if error.1.is_none() && error.0.value == var.as_ref() {
                error.1 = Some(var.span_range());
            }
        }
        self.check_any(var);
    }

    fn check_any<S: AsRef<str>>(&mut self, var: &Spanned<S>) {
        for error in &mut self.any {
            if error.1.is_none() {
                let var_name = error.0.split_at(error.0.find('/').unwrap() + 1).1;
                if var_name == var.as_ref() {
                    error.1 = Some(var.span_range());
                }
            }
        }
    }

    fn into_errors(self, errors: &mut Vec<PackageError>) {
        errors.extend(
            self.internal
                .into_iter()
                .map(|error| PackageError::IVarNotFound(error.0, error.1)),
        );
        errors.extend(self.external.into_iter().map(PackageError::EVarNotFound));
        errors.extend(
            self.any
                .into_iter()
                .map(|error| PackageError::VarNotFound(error.0, error.1)),
        );
    }
}

impl<'a> PackageInstance<'a> {
    pub fn validate(&self) -> Result<(), Vec<PackageError>> {
        self.validate_config()
    }

    fn validate_config(&self) -> Result<(), Vec<PackageError>> {
        let mut errors = Vec::new();
        let mut missing = Default::default();
        for config in self.config().values() {
            if let ConfType::Dynamic {
                ivars,
                evars,
                hvars,
                ..
            } = &config.conf_type
            {
                self.validate_ivars(ivars, evars, &mut missing)
                    .unwrap_or_else(|error| errors.extend(error));
                self.validate_evars(evars)
                    .unwrap_or_else(|error| errors.extend(error));
                self.validate_hvars(hvars, &mut missing)
                    .unwrap_or_else(|error| errors.extend(error));
            }
        }
        missing.into_errors(&mut errors);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_ivars(
        &self,
        ivars: &OrderedHashMap<Spanned<String>, InternalVar>,
        evars: &Map<Spanned<VPackageName>, Map<Spanned<String>, ExternalVar>>,
        missing: &mut MissingVars,
    ) -> Result<(), Vec<PackageError>> {
        let mut errors = Vec::new();

        let mut check_ivars = Set::new();
        for (var, var_spec) in ivars {
            match (&var_spec.ty, self.variant(), &var_spec.default) {
                (VarType::BindPort, Some(_), Some(default))
                    if default.value.components().vars().count() == 0 =>
                {
                    errors.push(PackageError::UntemplatedBindPort(
                        var.to_owned(),
                        Some(default.span_range()),
                    ));
                }
                (VarType::BindPort, Some(_), None) => {
                    errors.push(PackageError::UntemplatedBindPort(var.to_owned(), None));
                }
                _ => (),
            }

            for cond in &var_spec.conditions {
                if let InternalVarCondition::Var { name, .. } = cond {
                    match &**name {
                        DynVarName::Internal(var) => {
                            if !check_ivars.contains(&**var) {
                                let var = Spanned {
                                    value: var.clone().into(),
                                    span_start: name.span_start + 1,
                                    span_end: name.span_end - 1,
                                };
                                missing.push_internal(var);
                            }
                        }
                        DynVarName::Absolute(var_package, var)
                            if var_package.expand_to_cow(self.variant())
                                == self.config_pkg_name() =>
                        {
                            if !check_ivars.contains(&**var) {
                                let var = Spanned {
                                    value: var.clone().into(),
                                    span_start: name.span_start + 1,
                                    span_end: name.span_end - 1,
                                };
                                missing.push_internal(var);
                            }
                        }
                        DynVarName::Absolute(var_package, var) => {
                            let found = evars.get(var_package).and_then(|pkg| pkg.get(&**var));
                            if found.is_none() {
                                let error = Spanned {
                                    value: format!("{}/{}", var_package.as_raw(), var),
                                    span_start: name.span_start + 1,
                                    span_end: name.span_end - 1,
                                };
                                missing.push_external(error);
                            }
                        }
                    }
                }
            }
            check_ivars.insert(&***var);
            missing.check_internal(var);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_evars(
        &self,
        evars: &Map<Spanned<VPackageName>, Map<Spanned<String>, ExternalVar>>,
    ) -> Result<(), Vec<PackageError>> {
        let mut errors = Vec::new();

        for (pkg_name, vars) in evars {
            let pkg = match self.get_include(pkg_name) {
                Some(pkg) => pkg,
                None => {
                    errors.push(PackageError::PackageNotFound(pkg_name.to_owned()));
                    continue;
                }
            };

            for var in vars.keys() {
                let found = &pkg.config().iter().find_map(|(_, conf)| {
                    if let ConfType::Dynamic { ivars, .. } = &conf.conf_type {
                        ivars.get(&**var)
                    } else {
                        None
                    }
                });

                if found.is_none() {
                    errors.push(PackageError::EVarNotInPackage(
                        pkg_name.to_owned(),
                        var.to_owned(),
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_hvars(
        &self,
        hvars: &OrderedHashMap<Spanned<String>, HiddenVar>,
        missing: &mut MissingVars,
    ) -> Result<(), Vec<PackageError>> {
        let mut errors = Vec::new();
        let mut hvars_accum = Set::new();
        for (var, var_spec) in hvars {
            if let HiddenVarVal::Template(template) = &var_spec.val {
                for (var, var_pos) in crate::template::parse(&template.value).vars() {
                    // toml span includes quotes
                    let var_pos = var_pos + 1;
                    if let Some(pos) = var.find('/') {
                        let (pkg_name, var_name) = var.split_at(pos);
                        let var_name = &var_name[1..];

                        if pkg_name.is_empty() {
                            let found = self
                                .config()
                                .iter()
                                .find_map(|(_, conf)| {
                                    if let ConfType::Dynamic { ivars, .. } = &conf.conf_type {
                                        ivars.get(var_name)
                                    } else {
                                        None
                                    }
                                })
                                .map(drop)
                                .or_else(|| hvars_accum.get(var_name).map(drop));

                            if found.is_none() {
                                let var = Spanned {
                                    value: var.to_owned(),
                                    span_start: template.span_start + var_pos,
                                    span_end: template.span_start + var_pos + var.len(),
                                };
                                missing.push_any(var);
                            }
                        } else {
                            let spanned_pkg_name = Spanned {
                                value: pkg_name,
                                span_start: template.span_start + var_pos,
                                span_end: template.span_start + var_pos + pkg_name.len(),
                            };
                            match VPackageName::try_from(spanned_pkg_name) {
                                Ok(v_pkg_name) => {
                                    let found = self.config().iter().find_map(|(_, conf)| {
                                        if let ConfType::Dynamic { evars, .. } = &conf.conf_type {
                                            evars.get(&v_pkg_name).and_then(|pkg| pkg.get(var_name))
                                        } else {
                                            None
                                        }
                                    });
                                    if found.is_none() {
                                        let error = Spanned {
                                            value: var.to_owned(),
                                            span_start: template.span_start + var_pos,
                                            span_end: template.span_start + var_pos + var.len(),
                                        };
                                        missing.push_external(error)
                                    }
                                }
                                Err(error) => {
                                    errors.push(PackageError::InvalidPackageName(error));
                                }
                            };
                        }
                    } else {
                        use crate::template::Query;

                        if self.constants_by_variant().get(var).is_none() {
                            let error = PackageError::ConstantNotFound(Spanned {
                                value: var.to_owned(),
                                span_start: template.span_start + var_pos,
                                span_end: template.span_start + var_pos + var.len(),
                            });
                            errors.push(error);
                        }
                    }
                }
            }
            hvars_accum.insert(&***var);
            missing.check_any(var);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

pub struct Config {
    pub public: bool,
    pub external: bool,
    pub conf_type: ConfType,
}

impl TryFrom<crate::input::Config> for Config {
    type Error = PackageError;

    fn try_from(value: crate::input::Config) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        let conf_type = match (value.content, value.format) {
            (Some(content), None) => ConfType::Static {
                content,
                internal: value.internal.unwrap_or_default(),
            },
            (None, Some(format)) => {
                let ivars = value
                    .ivars
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, var)| Ok((name.into(), var.try_into()?)))
                    .collect::<Result<_, PackageError>>()?;
                let hvars = value
                    .hvars
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, var)| Ok((name.into(), var.try_into()?)))
                    .collect::<Result<_, PackageError>>()?;
                let evars = value
                    .evars
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, var)| {
                        Ok((
                            name.into(),
                            var.into_iter()
                                .map(|var| Ok((var.0.into(), var.1.try_into()?)))
                                .collect::<Result<_, PackageError>>()?,
                        ))
                    })
                    .collect::<Result<_, PackageError>>()?;
                ConfType::Dynamic {
                    format,
                    insert_header: value.insert_header,
                    with_header: value.with_header.unwrap_or_default(),
                    ivars,
                    evars,
                    hvars,
                    fvars: value.fvars.unwrap_or_default(),
                    cat_dir: value.cat_dir,
                    cat_files: value.cat_files.unwrap_or_default(),
                    comment: value.comment,
                    postprocess: value.postprocess.map(TryFrom::try_from).transpose()?,
                }
            }
            (None, None) => {
                return Err(PackageError::MissingFieldsOneOf(
                    value.span,
                    &[&["content"], &["format"]],
                ))
            }
            (Some(_), Some(_)) => {
                return Err(PackageError::Ambiguous(value.span, "configuration type"))
            }
        };

        Ok(Config {
            public: value.public.unwrap_or_default(),
            external: value.external.unwrap_or_default(),
            conf_type,
        })
    }
}

fn load_include<'a>(
    dir: &'a Path,
    name: &'a VPackageName,
    mut deps: FileDeps<'a>,
) -> impl 'a + FnMut() -> Package {
    use crate::error_report::Report;

    move || {
        let file = name.sps_path(dir);
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {}", file.display(), error));
        let package =
            toml::from_str::<crate::input::Package>(&source).expect("Failed to parse include");
        deps.as_mut().map(|deps| deps.insert(file.clone()));
        package
            .try_into()
            .unwrap_or_else(|error: PackageError| error.report(file.display().to_string(), source))
    }
}

impl TryFrom<crate::input::Package> for Package {
    type Error = PackageError;

    fn try_from(value: crate::input::Package) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        let extra_groups = value
            .extra_groups
            .unwrap_or_default()
            .into_iter()
            .map(|group| Ok((group.0, group.1.try_into()?)))
            .collect::<Result<_, PackageError>>()?;

        let spec = match (
            value.architecture,
            value.bin_package,
            value.binary,
            value.user,
            value.extends,
        ) {
            (Some(architecture), None, None, None, None) => PackageSpec::Base(BasePackageSpec {
                architecture: architecture.into_inner(),
            }),
            (None, Some(bin_package), Some(binary), Some(user), None) => {
                let service = ServicePackageSpec {
                    bin_package: template::TemplateString::try_from(bin_package).unwrap(),
                    min_patch: value.min_patch,
                    binary: template::TemplateString::try_from(binary).unwrap(),
                    conf_param: ConfParam::from_input(
                        value.conf_param,
                        value.bare_conf_param.unwrap_or_default(),
                    ),
                    conf_d: value.conf_d.map(TryInto::try_into).transpose()?,
                    user: user.try_into()?,
                    condition_path_exists: value.condition_path_exists,
                    service_type: value.service_type,
                    exec_stop: value.exec_stop,
                    after: value.after,
                    before: value.before,
                    wants: value.wants,
                    requires: value.requires,
                    binds_to: value.binds_to,
                    part_of: value.part_of,
                    wanted_by: value.wanted_by,
                    refuse_manual_start: value.refuse_manual_start.unwrap_or_default(),
                    refuse_manual_stop: value.refuse_manual_stop.unwrap_or_default(),
                    runtime_dir: value.runtime_dir.map(TryInto::try_into).transpose()?,
                    extra_service_config: value.extra_service_config,
                    allow_suid_sgid: value.allow_suid_sgid.unwrap_or_default(),
                    extra_groups,
                };
                PackageSpec::Service(service)
            }
            (None, None, None, None, Some(extends)) => PackageSpec::ConfExt(ConfExtPackageSpec {
                extends: extends.into_inner(),
                replaces: value.replaces.unwrap_or_default(),
                depends_on_extended: value.depends_on_extended.unwrap_or_default(),
                min_patch: value.min_patch,
                external: value.external.unwrap_or_default(),
                extra_groups,
            }),
            (None, None, None, None, None) => {
                return Err(PackageError::MissingFieldsOneOf(
                    value.span,
                    &[
                        &["architecture"],
                        &["bin_package", "binary", "user"],
                        &["extends"],
                    ],
                ))
            }
            (_architecture, _bin_package, _binary, _user, _extends) => {
                return Err(PackageError::Ambiguous(value.span, "package type"))
            }
        };

        let migrations = value
            .migrations
            .unwrap_or_default()
            .into_iter()
            .map(|(version, migration)| Ok((version.try_into()?, migration.try_into()?)))
            .collect::<Result<_, PackageError>>()?;

        let config = value
            .config
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| Ok((key, value.try_into()?)))
            .collect::<Result<_, PackageError>>()?;

        let plug = value
            .plug
            .unwrap_or_default()
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<_, _>>()?;

        let databases = value
            .databases
            .unwrap_or_default()
            .into_iter()
            .map(|db| Ok((db.0, db.1.try_into()?)))
            .collect::<Result<_, PackageError>>()?;

        let alternatives = value
            .alternatives
            .unwrap_or_default()
            .into_iter()
            .map(|alternative| Ok((alternative.0, alternative.1.try_into()?)))
            .collect::<Result<_, PackageError>>()?;

        require_fields!(value, name);
        Ok(Package {
            name,
            map_variants: value.map_variants.unwrap_or_default(),
            summary: value.summary.expect("missing summary"),
            long_doc: value.long_doc,
            spec,
            config,
            databases,
            depends: value.depends.unwrap_or_default(),
            provides: value.provides.unwrap_or_default(),
            recommends: value.recommends.unwrap_or_default(),
            suggests: value.suggests.unwrap_or_default(),
            conflicts: value.conflicts.unwrap_or_default(),
            extended_by: value.extended_by.unwrap_or_default(),
            add_files: value.add_files.unwrap_or_default(),
            import_files: value.import_files.unwrap_or_default(),
            add_dirs: value.add_dirs.unwrap_or_default(),
            add_links: value.add_links.unwrap_or_default(),
            add_manpages: value.add_manpages.unwrap_or_default(),
            alternatives,
            patch_foreign: value.patch_foreign.unwrap_or_default(),
            extra_triggers: value.extra_triggers.unwrap_or_default(),
            migrations,
            plug,
            custom_postrm_script: value.custom_postrm_script,
        })
    }
}

#[derive(Debug)]
pub enum PackageError {
    Ambiguous(Span, &'static str),
    MissingFields(Span, Vec<&'static str>),
    MissingFieldsOneOf(Span, &'static [&'static [&'static str]]),
    UnknownFields(Vec<toml::Spanned<String>>),
    UnknownVarType(toml::Spanned<String>),
    Migration(MigrationVersionError),
    InvalidPackageName(Spanned<VPackageNameError>),
    CreatePathWithoutType(std::ops::Range<usize>),
    IVarNotFound(Spanned<String>, Option<std::ops::Range<usize>>),
    EVarNotFound(Spanned<String>),
    VarNotFound(Spanned<String>, Option<std::ops::Range<usize>>),
    InvalidVersion(Span, String),
    ConstantNotFound(Spanned<String>),
    EVarNotInPackage(Spanned<VPackageName>, Spanned<String>),
    UntemplatedBindPort(Spanned<String>, Option<std::ops::Range<usize>>),
    ConstCond(std::ops::Range<usize>),
    PackageNotFound(Spanned<VPackageName>),
}

impl From<MigrationVersionError> for PackageError {
    fn from(value: MigrationVersionError) -> Self {
        PackageError::Migration(value)
    }
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
    pub import_files: &'a [[TemplateString; 2]],
    pub add_dirs: &'a [TemplateString],
    pub add_links: &'a [TemplateString],
    pub add_manpages: &'a [String],
    pub alternatives: &'a Map<String, Alternative>,
    pub patch_foreign: &'a Map<String, String>,
    pub extra_triggers: &'a Set<TemplateString>,
    pub migrations: &'a Map<MigrationVersion, Migration>,
    pub plug: &'a [Plug],
    pub custom_postrm_script: Option<&'a TemplateString>,
}

impl<'a> PackageInstance<'a> {
    pub fn as_service(&self) -> Option<ServiceInstance<'_>> {
        if let PackageSpec::Service(service) = &self.spec {
            Some(ServiceInstance {
                name: &self.name,
                variant: self.variant,
                map_variants: self.map_variants,
                summary: self.summary,
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
    fn extra_groups(
        &self,
    ) -> Option<NonEmptyMap<TemplateString, ExtraGroup, &'_ Map<TemplateString, ExtraGroup>>>;
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
                self.get_include(&confext.extends)
                    .unwrap_or_else(|| {
                        panic!(
                            "Package {} extended by {} not found",
                            confext.extends.expand_to_cow(self.variant),
                            self.name
                        )
                    })
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
                self.get_include(&confext.extends)
                    .unwrap_or_else(|| {
                        panic!(
                            "Package {} extended by {} not found",
                            confext.extends.expand_to_cow(self.variant),
                            self.name
                        )
                    })
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
        self.as_service()
            .map(|service| service.user_name())
            .or_else(|| {
                if let PackageSpec::ConfExt(confext) = &self.spec {
                    if confext.depends_on_extended && !confext.external {
                        self.get_include(&confext.extends)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Package {} extended by {} not found",
                                    confext.extends.expand_to_cow(self.variant),
                                    self.name
                                )
                            })
                            .instantiate(self.variant, None)
                            .service_user()
                            .map(|user| Cow::Owned(String::from(user)))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    }

    fn service_group(&self) -> Option<Cow<'_, str>> {
        self.as_service()
            .and_then(|service| ServiceInstance::service_group(&service))
            .or_else(|| {
                if let PackageSpec::ConfExt(confext) = &self.spec {
                    if confext.depends_on_extended && !confext.external {
                        self.get_include(&confext.extends)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Package {} extended by {} not found",
                                    confext.extends.expand_to_cow(self.variant),
                                    self.name
                                )
                            })
                            .instantiate(self.variant, None)
                            .service_group()
                            .map(|group| Cow::Owned(String::from(group)))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
    }

    fn extra_groups(
        &self,
    ) -> Option<NonEmptyMap<TemplateString, ExtraGroup, &'_ Map<TemplateString, ExtraGroup>>> {
        match &self.spec {
            PackageSpec::Service(service) => NonEmptyMap::from_map(&service.extra_groups),
            PackageSpec::ConfExt(confext) => {
                let groups = NonEmptyMap::from_map(&confext.extra_groups);
                if groups.is_some() && !confext.depends_on_extended {
                    // TODO: implement permission system and check if groups exist as well
                    panic!("The configuration extension {} doesn't depent on extended package yet it wants to add the user to a group. The user is not guaranteed to exist.", self.name);
                }
                groups
            }
            PackageSpec::Base(_) => None,
        }
    }

    fn get_include(&self, name: &VPackageName) -> Option<&Package> {
        self.includes
            .as_ref()
            .and_then(|includes| includes.get(name))
    }

    fn is_conf_ext(&self) -> bool {
        matches!(&self.spec, PackageSpec::ConfExt(_))
    }

    fn conf_dir(&self) -> Option<&str> {
        self.as_service().and_then(|service| {
            service
                .spec
                .conf_d
                .as_ref()
                .map(|conf_d| conf_d.name.as_ref())
        })
    }

    fn databases(&self) -> &Map<Database, DbConfig> {
        self.databases
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
            self.constants
                .get(key)?
                .get(self.variant?)
                .map(AsRef::as_ref)
        }
    }
}

pub enum PackageSpec {
    Service(ServicePackageSpec),
    ConfExt(ConfExtPackageSpec),
    Base(BasePackageSpec),
}

pub enum ConfType {
    Static {
        content: String,
        internal: bool,
    },
    Dynamic {
        format: ConfFormat,
        insert_header: Option<TemplateString>,
        with_header: bool,
        ivars: OrderedHashMap<Spanned<String>, InternalVar>,
        evars: Map<Spanned<VPackageName>, Map<Spanned<String>, ExternalVar>>,
        hvars: OrderedHashMap<Spanned<String>, HiddenVar>,
        fvars: Map<String, FileVar>,
        cat_dir: Option<String>,
        cat_files: Set<String>,
        comment: Option<String>,
        // Command to run after creating whole config file
        postprocess: Option<PostProcess>,
    },
}

pub struct GeneratedFile {
    pub ty: GeneratedType,
    pub internal: bool,
}

impl TryFrom<crate::input::GeneratedFile> for GeneratedFile {
    type Error = PackageError;

    fn try_from(value: crate::input::GeneratedFile) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        let ty = match (value.file, value.dir) {
            (Some(file), None) => GeneratedType::File(file),
            (None, Some(dir)) => GeneratedType::Dir(dir),
            (None, None) => {
                return Err(PackageError::MissingFieldsOneOf(
                    value.span,
                    &[&["file"], &["dir"]],
                ))
            }
            (Some(_), Some(_)) => {
                return Err(PackageError::Ambiguous(
                    value.span,
                    "type of generated filesystem object",
                ))
            }
        };

        Ok(GeneratedFile {
            ty,
            internal: value.internal.unwrap_or_default(),
        })
    }
}

#[derive(Eq, PartialEq)]
pub enum GeneratedType {
    File(TemplateString),
    Dir(TemplateString),
}

pub struct PostProcess {
    pub command: Vec<TemplateString>,
    pub generates: Vec<GeneratedFile>,
    pub stop_service: bool,
}

impl TryFrom<crate::input::PostProcess> for PostProcess {
    type Error = PackageError;

    fn try_from(value: crate::input::PostProcess) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        let generates = value
            .generates
            .unwrap_or_default()
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<_, _>>()?;

        require_fields!(value, command);
        Ok(PostProcess {
            command,
            generates,
            stop_service: value.stop_service.unwrap_or_default(),
        })
    }
}

pub struct Plug {
    pub run_as_user: TemplateString,
    pub run_as_group: Option<TemplateString>,
    pub register_cmd: Vec<TemplateString>,
    pub unregister_cmd: Vec<TemplateString>,
    pub read_only_root: bool,
}

impl TryFrom<crate::input::Plug> for Plug {
    type Error = PackageError;

    fn try_from(value: crate::input::Plug) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, run_as_user, register_cmd, unregister_cmd);
        Ok(Plug {
            run_as_user,
            run_as_group: value.run_as_group,
            register_cmd,
            unregister_cmd,
            read_only_root: value.read_only_root.unwrap_or(true),
        })
    }
}

pub struct Migration {
    pub config: Option<TemplateString>,
    pub postinst_finish: Option<TemplateString>,
}

impl TryFrom<crate::input::Migration> for Migration {
    type Error = PackageError;

    fn try_from(value: crate::input::Migration) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        if value.config.is_none() && value.postinst_finish.is_none() {
            return Err(PackageError::MissingFieldsOneOf(
                value.span,
                &[&["config"], &["postinst_finish"]],
            ));
        }

        Ok(Migration {
            config: value.config,
            postinst_finish: value.postinst_finish,
        })
    }
}

pub struct DbConfig {
    pub template: String,
    pub min_version: Option<String>,
    pub since: Option<String>,
    pub config_file_owner: Option<String>,
    pub config_file_group: Option<String>,
}

impl TryFrom<crate::input::DbConfig> for DbConfig {
    type Error = PackageError;

    fn try_from(value: crate::input::DbConfig) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, template);
        let since = if let Some(since) = value.since {
            let output = std::process::Command::new("dpkg")
                .args(&["--validate-version", since.get_ref()])
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("Failed to compare versions")
                .wait_with_output()
                .expect("Failed to compare versions");
            if !output.status.success() {
                let err_message =
                    String::from_utf8(output.stderr).expect("Failed to decode error message");
                return Err(PackageError::InvalidVersion(
                    Span::from(&since),
                    err_message,
                ));
            }
            Some(since.into_inner())
        } else {
            None
        };

        Ok(DbConfig {
            template,
            min_version: value.min_version,
            since,
            config_file_owner: value.config_file_owner,
            config_file_group: value.config_file_group,
        })
    }
}

pub struct ExtraGroup {
    pub create: bool,
}

impl TryFrom<crate::input::ExtraGroup> for ExtraGroup {
    type Error = PackageError;

    fn try_from(value: crate::input::ExtraGroup) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, create);
        Ok(ExtraGroup { create })
    }
}

pub struct RuntimeDir {
    pub mode: String,
}

impl TryFrom<crate::input::RuntimeDir> for RuntimeDir {
    type Error = PackageError;

    fn try_from(value: crate::input::RuntimeDir) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, mode);
        Ok(RuntimeDir { mode })
    }
}

pub struct ConfDir {
    pub param: String,
    pub name: String,
}

impl TryFrom<crate::input::ConfDir> for ConfDir {
    type Error = PackageError;

    fn try_from(value: crate::input::ConfDir) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, param, name);
        Ok(ConfDir { param, name })
    }
}

#[derive(Debug)]
pub struct UserSpec {
    pub name: Option<TemplateString>,
    pub group: bool,
    pub create: Option<CreateUser>,
}

impl TryFrom<crate::input::UserSpec> for UserSpec {
    type Error = PackageError;

    fn try_from(value: crate::input::UserSpec) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        Ok(UserSpec {
            name: value.name,
            group: value.group.unwrap_or_default(),
            create: value.create.map(TryInto::try_into).transpose()?,
        })
    }
}

#[derive(Debug)]
pub struct CreateUser {
    pub home: bool,
}

impl TryFrom<crate::input::CreateUser> for CreateUser {
    type Error = PackageError;

    fn try_from(value: crate::input::CreateUser) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, home);
        Ok(CreateUser { home })
    }
}

#[derive(Clone, Debug)]
pub struct Alternative {
    pub name: String,
    pub dest: String,
    pub priority: u32,
}

impl TryFrom<crate::input::Alternative> for Alternative {
    type Error = PackageError;

    fn try_from(value: crate::input::Alternative) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, name, dest, priority);
        Ok(Alternative {
            name,
            dest,
            priority,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Span {
    pub(crate) begin: usize,
    pub(crate) end: usize,
}

impl From<Span> for std::ops::Range<usize> {
    fn from(value: Span) -> Self {
        value.begin..value.end
    }
}

impl<'a, T> From<&'a toml::Spanned<T>> for Span {
    fn from(spanned: &'a toml::Spanned<T>) -> Self {
        Span {
            begin: spanned.span().0,
            end: spanned.span().1,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct MigrationVersion(String);

impl MigrationVersion {
    pub fn version(&self) -> &str {
        &self.0[3..]
    }
}

impl std::cmp::Ord for MigrationVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self == other {
            return std::cmp::Ordering::Equal;
        }
        if std::process::Command::new("dpkg")
            .args(&["--compare-versions", self.version(), "lt", &other.version()])
            .spawn()
            .expect("Failed to compare versions")
            .wait()
            .expect("Failed to compare versions")
            .success()
        {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    }
}

impl std::cmp::PartialOrd for MigrationVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<toml::Spanned<String>> for MigrationVersion {
    type Error = MigrationVersionError;

    fn try_from(string: toml::Spanned<String>) -> Result<Self, Self::Error> {
        let span = Span::from(&string);
        let string = string.into_inner();

        if !string.starts_with("<< ") {
            let error = MigrationVersionError {
                error: MigrationVersionErrorInner::BadPrefix(string),
                span,
            };
            return Err(error);
        }
        let output = std::process::Command::new("dpkg")
            .args(&["--validate-version", &string[3..]])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to compare versions")
            .wait_with_output()
            .expect("Failed to compare versions");
        if output.status.success() {
            Ok(MigrationVersion(string))
        } else {
            let err_message =
                String::from_utf8(output.stderr).expect("Failed to decode error message");
            let error = MigrationVersionError {
                error: MigrationVersionErrorInner::Invalid(err_message),
                span,
            };
            Err(error)
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum MigrationVersionErrorInner {
    BadPrefix(String),
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct MigrationVersionError {
    pub(crate) error: MigrationVersionErrorInner,
    // Debug is fine for now actually
    #[allow(dead_code)]
    pub(crate) span: Span,
}

impl fmt::Display for MigrationVersionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // strip_prefix method is in str since 1.45, we support 1.34
        let strip_prefix = "dpkg: warning: ";
        match &self.error {
            MigrationVersionErrorInner::BadPrefix(string) => write!(
                f,
                "invalid migration version '{}', the version must start with '<< '",
                string
            ),
            MigrationVersionErrorInner::Invalid(string) if string.starts_with(strip_prefix) => {
                write!(f, "{}", &string[strip_prefix.len()..])
            }
            MigrationVersionErrorInner::Invalid(string) => write!(f, "{}", string),
        }
    }
}

fn check_unknown_fields(unknown: Vec<toml::Spanned<String>>) -> Result<(), PackageError> {
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(PackageError::UnknownFields(unknown))
    }
}
