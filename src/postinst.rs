use crate::{PackageInstance, ServiceInstance, PackageSpec, ConfType, VarType, ConfFormat, FileType, HiddenVarVal, PackageConfig, DbConfig, FileVar, GeneratedType, Set, Map, VPackageName};
use std::fmt;
use std::borrow::Cow;
use itertools::Either;
use std::cmp::Ordering;
use std::convert::TryFrom;

#[derive(Clone)]
pub struct Config<'a> {
    pub package_name: &'a str,
    pub file_name: &'a str,
    pub format: &'a ConfFormat,
    pub insert_header: Option<Cow<'a, str>>,
    pub with_header: bool,
    pub public: bool,
    pub change_group: Option<Cow<'a, str>>,
    pub extension: bool,
}

pub trait HandlePostinst: Sized {
    type Error: fmt::Debug + fmt::Display;

    fn prepare_user<T: fmt::Display>(&mut self, name: &str, group: bool, home: Option<T>) -> Result<(), Self::Error>;
    fn add_user_to_groups<I>(&mut self, user: &str, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str>;
    fn create_groups<I>(&mut self, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str>;
    fn prepare_database(&mut self, instance: &ServiceInstance, name: &str, config: &DbConfig) -> Result<(), Self::Error>;
    fn prepare_config(&mut self, config: &Config) -> Result<(), Self::Error>;
    fn finish_config(&mut self, config: &Config) -> Result<(), Self::Error>;
    fn fetch_var(&mut self, config: &Config, package: &str, name: &str) -> Result<(), Self::Error>;
    fn generate_const_var(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, val: &str) -> Result<(), Self::Error>;
    fn generate_var_using_script(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, script: &str) -> Result<(), Self::Error>;
    fn generate_var_using_template(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, template: &str, constatnts: ConstantsByVariant<'_>) -> Result<(), Self::Error>;
    fn sub_object_begin(&mut self, config: &Config, name: &str) -> Result<(), Self::Error>;
    fn sub_object_end(&mut self, config: &Config, name: &str) -> Result<(), Self::Error>;
    fn write_var<'a, I>(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, structure: I, ignore_empty: bool) -> Result<(), Self::Error> where I: Iterator<Item=&'a str>;
    fn include_fvar<'a, I>(&mut self, config: &Config, var: &FileVar, structure: I, subdir: &str) -> Result<(), Self::Error> where I: Iterator<Item=&'a str>;
    fn reload_apparmor(&mut self) -> Result<(), Self::Error>;
    fn stop_service(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error>;
    fn restart_service_if_needed(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error>;
    fn trigger_config_changed(&mut self, instance: &PackageInstance) -> Result<(), Self::Error>;
    fn include_conf_dir<T: fmt::Display>(&mut self, config: &Config, dir: T) -> Result<(), Self::Error>;
    fn include_conf_file<T: fmt::Display>(&mut self, config: &Config, file: T) -> Result<(), Self::Error>;
    fn postprocess_conf_file<I>(&mut self, command: I) -> Result<(), Self::Error> where I: IntoIterator, I::Item: fmt::Display;
    fn write_comment(&mut self, config: &Config, comment: &str) -> Result<(), Self::Error>;
    fn register_alternatives<A, B, I>(&mut self, alternatives: I) -> Result<(), Self::Error> where I: IntoIterator<Item=(A, B)>, A: AsRef<str>, B: std::borrow::Borrow<crate::Alternative>;
    fn patch_files<A, B, I>(&mut self, pkg_name: &str, patches: I) -> Result<(), Self::Error> where I: IntoIterator<Item=(A, B)>, A: AsRef<str>, B: AsRef<str>;
    fn activate_trigger(&mut self, trigger: &str, no_await: bool) -> Result<(), Self::Error>;
    fn create_tree(&mut self, path: &str) -> Result<(), Self::Error>;
    fn create_path(&mut self, config: &Config, var_name: &str, file_type: &FileType, mode: u16, owner: &str, group: &str, only_parent: bool) -> Result<(), Self::Error>;
    fn finish(self) -> Result<(), Self::Error>;
}

pub struct ConstantsByVariant<'a> {
    variant: Option<&'a str>,
    constants: &'a Map<String, Map<String, String>>,
}

impl<'a> crate::template::Query for ConstantsByVariant<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        if key == "variant" {
            self.variant
        } else {
            self.constants.get(key)?.get(self.variant?).map(AsRef::as_ref)
        }
    }
}

pub trait Package<'a>: PackageConfig {
    fn config_pkg_name(&self) -> &str;
    fn variant(&self) -> Option<&str>;
    fn constants_by_variant(&self) -> ConstantsByVariant<'_>;
    fn config_sub_dir(&self) -> Cow<'a, str>;
    fn internal_config_sub_dir(&self) -> Cow<'a, str>;
    fn service_name(&self) -> Option<&str>;
    fn service_user(&self) -> Option<Cow<'_, str>>;
    fn service_group(&self) -> Option<Cow<'_, str>>;
    fn get_include(&self, name: &VPackageName) -> Option<&super::Package>;
    fn is_conf_ext(&self) -> bool;
    fn conf_dir(&self) -> Option<&str>;
}

impl<'a> Package<'a> for ServiceInstance<'a> {
    fn config_pkg_name(&self) -> &str {
        &self.name
    }

    fn variant(&self) -> Option<&str> {
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

    fn get_include(&self, name: &VPackageName) -> Option<&super::Package> {
        self.includes.as_ref().and_then(|includes| includes.get(name))
    }

    fn is_conf_ext(&self) -> bool {
        false
    }

    fn conf_dir(&self) -> Option<&str> {
        self.spec.conf_d.as_ref().map(|conf_d| conf_d.name.as_ref())
    }
}

impl<'a> Package<'a> for PackageInstance<'a> {
    fn config_pkg_name(&self) -> &str {
        &self.name
    }

    fn variant(&self) -> Option<&str> {
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
        self.as_service().map(|service| service.user_name())
    }

    fn service_group(&self) -> Option<Cow<'_, str>> {
        self.as_service().and_then(|service| ServiceInstance::service_group(&service))
    }

    fn get_include(&self, name: &VPackageName) -> Option<&super::Package> {
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
}

fn compute_structure<'a>(name: &'a str, structure: &'a Option<Vec<String>>) -> impl Iterator<Item=&'a str> + DoubleEndedIterator + Clone + std::fmt::Debug {
    structure
        .as_ref()
        .map(|structure| Either::Left(structure.iter().map(AsRef::as_ref)))
        .unwrap_or(Either::Right(std::iter::once(name)))
}

#[derive(Debug)]
struct WriteVar<'a, I> where I: Iterator<Item=&'a str> + Clone {
    structure: I,
    ty: WriteVarType<'a>,
}

#[derive(Debug)]
enum WriteVarType<'a> {
    Simple {
        ty: &'a VarType,
        package: Cow<'a, str>,
        name: &'a str,
        ignore_empty: bool,
    },
    File {
        data: &'a FileVar,
    }
}

impl<'a, I> PartialOrd for WriteVar<'a, I> where I: Iterator<Item=&'a str> + Clone {
    fn partial_cmp(&self, other: &WriteVar<'a, I>) -> Option<Ordering> {
        let i0 = self.structure.clone();
        let i1 = other.structure.clone();

        Some(i0.cmp(i1))
    }
}

impl<'a, I> Ord for WriteVar<'a, I> where I: Iterator<Item=&'a str> + Clone {
    fn cmp(&self, other: &WriteVar<'a, I>) -> Ordering {
        let i0 = self.structure.clone();
        let i1 = other.structure.clone();

        i0.cmp(i1)
    }
}

impl<'a, I> PartialEq for WriteVar<'a, I> where I: Iterator<Item=&'a str> + Clone {
    fn eq(&self, other: &WriteVar<'a, I>) -> bool {
        let i0 = self.structure.clone();
        let i1 = other.structure.clone();

        i0.cmp(i1) == Ordering::Equal
    }
}

impl<'a, I> Eq for WriteVar<'a, I> where I: Iterator<Item=&'a str> + Clone {}

fn handle_postprocess<'a, 'b, T: HandlePostinst, P: Package<'a>>(handler: &mut T, package: &P, triggers: &mut Set<Cow<'b, str>>, postprocess: &'b crate::PostProcess) -> Result<(), T::Error> {
    for generated in &postprocess.generates {
        let path = match &generated.ty {
            GeneratedType::File(path) => path,
            GeneratedType::Dir(path) => path,
        };
        let path = if path.starts_with('/') {
            Cow::<str>::Borrowed(&path)
        } else {
            Cow::<str>::Owned(format!("/etc/{}/{}", package.config_sub_dir(), path))
        };
        if let Some(pos) = path.rfind('/') {
            handler.create_tree(&path[..pos])?;
        } else {
            handler.create_tree(&path)?;
        }
        triggers.insert(path);
    }
    handler.postprocess_conf_file(postprocess.command.iter().map(|arg| arg.expand(package.constants_by_variant())))?;
    Ok(())
}

fn handle_config<'a, T: HandlePostinst, P: Package<'a>>(handler: &mut T, package: &P) -> Result<(), T::Error> {
    let mut triggers = Set::<Cow<str>>::new();
    let mut interested = Set::<String>::new();
    let mut needs_stopped_service = false;
    for (conf_name, config) in package.config() {
        if let ConfType::Dynamic { ivars, evars, hvars, fvars, format, comment, insert_header, with_header, .. } = &config.conf_type {
            let file_name = format!("/etc/{}/{}", package.config_sub_dir(), conf_name.expand(package.constants_by_variant()));
            // Manual scope due to borrowing issues.
            {
                let config_ctx = Config {
                    package_name: package.config_pkg_name(),
                    file_name: &file_name,
                    insert_header: insert_header.as_ref().map(|header| header.expand_to_cow(package.constants_by_variant())),
                    with_header: *with_header,
                    format,
                    public: config.public,
                    extension: package.is_conf_ext(),
                    change_group: package.service_group(),
                };
                handler.prepare_config(&config_ctx)?;
                if let Some(comment) = comment {
                    handler.write_comment(&config_ctx, comment)?;
                }

                for var in ivars.keys() {
                    handler.fetch_var(&config_ctx, config_ctx.package_name, var)?;
                }

                for (pkg_name, vars) in evars {
                    let pkg_name = pkg_name.expand_to_cow(package.variant());
                    for (var, _var_spec) in vars {
                        handler.fetch_var(&config_ctx, &pkg_name, var)?;
                    }

                }

                for (var, var_spec) in hvars {
                    match &var_spec.val {
                        HiddenVarVal::Constant(val) => handler.generate_const_var(&config_ctx, config_ctx.package_name, var, &var_spec.ty, val)?,
                        HiddenVarVal::Script(script) => handler.generate_var_using_script(&config_ctx, config_ctx.package_name, var, &var_spec.ty, &script.expand_to_cow(package.constants_by_variant()))?,
                        HiddenVarVal::Template(template) => handler.generate_var_using_template(&config_ctx, config_ctx.package_name, var, &var_spec.ty, template, package.constants_by_variant())?,
                    }
                }

                let mut write_vars = Vec::new();

                for (var, var_spec) in ivars {
                    match (&var_spec.ty, package.variant(), &var_spec.default) {
                         (VarType::BindPort, Some(_), Some(default)) if default.components().vars().count() == 0 => {
                            panic!("Error: bind port of variable {} in package {} is not templated! This will cause bind failures!", var, package.config_pkg_name())
                        }
                        (VarType::BindPort, Some(_), None) => {
                            panic!("Error: bind port of variable {} in package {} is not templated! This will cause bind failures!", var, package.config_pkg_name())
                         }
                        _ => (),
                    }

                    if var_spec.store {
                        write_vars.push(WriteVar {
                            structure: compute_structure(&var, &var_spec.structure),
                            ty: WriteVarType::Simple {
                                ty: &var_spec.ty,
                                package: Cow::Borrowed(config_ctx.package_name),
                                name: var,
                                ignore_empty: var_spec.ignore_empty,
                            },
                        });
                    }
                }

                for (pkg_name, vars) in evars {
                    let pkg = package.get_include(pkg_name).ok_or(pkg_name).expect("Package not found");
                    for (var, var_spec) in vars {
                        if var_spec.store {
                            let ty = &pkg
                                .config()
                                .iter()
                                .find_map(|(_, conf)| if let ConfType::Dynamic { ivars, .. } = &conf.conf_type {
                                    ivars.get(var)
                                } else {
                                    None
                                })
                                .unwrap_or_else(|| panic!("Variable {} not found in {}", var, pkg_name.expand_to_cow(package.variant())))
                                .ty;

                            let out_var = var_spec.name.as_ref().unwrap_or(var);
                            write_vars.push(WriteVar {
                                structure: compute_structure(&out_var, &var_spec.structure),
                                ty: WriteVarType::Simple {
                                    ty,
                                    package: pkg_name.expand_to_cow(package.variant()),
                                    name: var,
                                    ignore_empty: var_spec.ignore_empty,
                                },
                            });
                        }
                    }
                }

                let mut hvars_accum = Set::new();
                for (var, var_spec) in hvars {
                    if let HiddenVarVal::Template(template) = &var_spec.val {
                        for var in crate::template::parse(template).vars() {
                            if let Some(pos) = var.find('/') {
                                let (pkg_name, var_name) = var.split_at(pos);
                                let var_name = &var_name[1..];

                                if pkg_name.is_empty() {
                                    package
                                        .config()
                                        .iter()
                                        .find_map(|(_, conf)| if let ConfType::Dynamic { ivars, .. } = &conf.conf_type {
                                            ivars.get(var_name)
                                        } else {
                                            None
                                        })
                                        .map(drop)
                                        .or_else(|| hvars_accum.get(var_name).map(drop))
                                        .unwrap_or_else(|| panic!("Variable {} not found in {}", var_name, package.config_pkg_name()));
                                } else {
                                    let v_pkg_name = VPackageName::try_from(String::from(pkg_name))
                                        .unwrap_or_else(|error| panic!("Invalid package name in template {}: {}", template, error));
                                    package
                                        .config()
                                        .iter()
                                        .find_map(|(_, conf)| if let ConfType::Dynamic { evars, .. } = &conf.conf_type {
                                            evars.get(&v_pkg_name).and_then(|pkg| pkg.get(var_name))
                                        } else {
                                            None
                                        })
                                        .unwrap_or_else(|| panic!("Variable {} not found in {}", var_name, pkg_name));
                                }
                            } else {
                                use crate::template::Query;

                                if package.constants_by_variant().get(var).is_none() {
                                    panic!("Unknown constant {}");
                                }
                            }
                        }
                    }

                    if var_spec.store {
                        write_vars.push(WriteVar {
                            structure: compute_structure(&var, &var_spec.structure),
                            ty: WriteVarType::Simple {
                                ty: &var_spec.ty,
                                package: Cow::Borrowed(config_ctx.package_name),
                                name: var,
                                ignore_empty: var_spec.ignore_empty,
                            },
                        });
                    }
                    hvars_accum.insert(&**var);
                }

                for (var, var_spec) in fvars {
                    match var_spec {
                        FileVar::Dir { structure, .. } => {
                            write_vars.push(WriteVar {
                                structure: compute_structure(&var, structure),
                                ty: WriteVarType::File {
                                    data: var_spec,
                                },
                            });
                        }
                    }
                }

                write_vars.sort_unstable();

                static STUPID_HACK: Option<Vec<String>> = None;
                #[allow(unused_assignments)]
                let mut previous = Some(compute_structure("", &STUPID_HACK));
                previous = None;
                for var in write_vars {
                    if let Some(previous) = previous {
                        let mut cur = var.structure.clone().peekable();
                        // manual impl of peekable for prev because peekable impls
                        // DoubleEndedIterator since 1.38 and Debian has only 1.34.
                        let mut prev = previous;
                        let mut prev_peeked = prev.next();

                        while let (Some(a), Some(b)) = (prev_peeked, cur.peek()) {
                            if a != *b {
                                break;
                            }

                            prev_peeked = prev.next();
                            cur.next();
                        }

                        // We iterate previous in reverse in case we implement XML in the future
                        let mut prev = prev.rev().chain(prev_peeked);
                        prev.next();

                        for item in prev {
                            handler.sub_object_end(&config_ctx, item)?;
                        }

                        while let Some(item) = cur.next() {
                            if cur.peek().is_some() {
                                handler.sub_object_begin(&config_ctx, item)?;
                            }
                        }
                    } else {
                        let mut structure = var.structure.clone().peekable();
                        while let Some(item) = structure.next() {
                            if structure.peek().is_some() {
                                handler.sub_object_begin(&config_ctx, item)?;
                            }
                        }
                    }
                    match var.ty {
                        WriteVarType::Simple {
                            package,
                            name,
                            ty,
                            ignore_empty,
                        } => handler.write_var(&config_ctx, &package, name, ty, var.structure.clone(), ignore_empty)?,
                        WriteVarType::File { data, } => handler.include_fvar(&config_ctx, data, var.structure.clone(), &package.config_sub_dir())?,
                    }
                    
                    previous = Some(var.structure);
                }

                if let Some(previous) = previous {
                    let mut prev = previous.rev();
                    prev.next();

                    for item in prev {
                        handler.sub_object_end(&config_ctx, item)?;
                    }
                }

                let ivars_iter = ivars
                    .iter()
                    .map(|(var, spec)| (var, &spec.ty));

                let hvars_iter = hvars
                    .iter()
                    .map(|(var, spec)| (var, &spec.ty));

                // We must not include evars as they create the dir in their package.

                for (var, ty) in ivars_iter.chain(hvars_iter) {
                    match ty {
                        VarType::Path { file_type: Some(file_type), create: Some(create) } => {
                            let owner = create.owner.expand_to_cow(package.constants_by_variant());
                            let owner = if let Cow::Borrowed("$service") = owner {
                                package.service_user().expect("Attempt to use service user but the package is not a service.")
                            } else {
                                owner
                            };

                            let group = create.group.expand_to_cow(package.constants_by_variant());
                            let group = if let Cow::Borrowed("$service") = group {
                                package.service_user().expect("Attempt to use service group but it's missing or the package is not a service.")
                            } else {
                                group
                            };

                            handler.create_path(&config_ctx, var, file_type, create.mode, &owner, &group, create.only_parent)?;
                        },
                        VarType::Path { file_type: None, create: Some(_) } => panic!("Invalid specification: path can't be created without specifying type"),
                        _ => (),
                    }
                }

            }

            triggers.insert(Cow::Owned(file_name));
        }
    }

    for (conf_name, config) in package.config() {
        if let ConfType::Dynamic { format, cat_dir, cat_files, postprocess, insert_header, with_header, .. } = &config.conf_type {
            let file_name = format!("/etc/{}/{}", package.config_sub_dir(), conf_name.expand(package.constants_by_variant()));

            let config_ctx = Config {
                package_name: package.config_pkg_name(),
                file_name: &file_name,
                insert_header: insert_header.as_ref().map(|header| header.expand_to_cow(package.constants_by_variant())),
                with_header: *with_header,
                format,
                public: config.public,
                extension: package.is_conf_ext(),
                change_group: package.service_group(),
            };

            if let Some(cat_dir) = cat_dir {
                let conf_dir = format!("/etc/{}/{}", package.config_sub_dir(), cat_dir);
                handler.include_conf_dir(&config_ctx, &conf_dir)?;
                interested.insert(conf_dir);
            }

            for file in cat_files {
                let conf_file = format!("/etc/{}/{}", package.config_sub_dir(), file);
                handler.include_conf_file(&config_ctx, &conf_file)?;
                interested.insert(conf_file);
            }

            handler.finish_config(&config_ctx)?;

            if let Some(postprocess) = postprocess {
                if postprocess.stop_service {
                    needs_stopped_service = true;
                } else {
                    handle_postprocess(handler, package, &mut triggers, postprocess)?;
                }
            }
        }
    }


    if needs_stopped_service {
        for config in package.config().values() {
            if let ConfType::Dynamic { postprocess: Some(postprocess @ crate::PostProcess { stop_service: true, .. }), .. } = &config.conf_type {
                handle_postprocess(handler, package, &mut triggers, postprocess)?;
            }
        }
    }

    let abs_config_dir = format!("/etc/{}", package.config_sub_dir());

    if let Some(conf_dir) = package.conf_dir() {
        interested.insert(format!("/etc/{}/{}", package.config_sub_dir(), conf_dir.trim_end_matches('/')));
    }

    let mut activated = Set::new();

    for trigger in &triggers {
        if let Some(pos) = trigger.rfind('/') {
            let parent = &trigger[..pos];
            if !interested.contains(&**trigger) && !interested.contains(parent) {
                handler.activate_trigger(&format!("`realpath \"{}\"`", trigger), true)?;
                if parent != abs_config_dir && !triggers.contains(parent) && !activated.contains(parent) {
                    handler.activate_trigger(&format!("`realpath \"{}\"`", trigger), true)?;
                    activated.insert(parent);
                }
            }
        }
    }

    Ok(())
}

pub fn handle_instance<T: HandlePostinst>(mut handler: T, instance: &PackageInstance) -> Result<(), <T as HandlePostinst>::Error> {
    if let Some(service) = instance.as_service() {
        if let Some(create_user) = &service.spec.user.create {
            let user = service.user_name();
            if create_user.home {
                handler.prepare_user(&user, service.spec.user.group, Some(format_args!("/var/lib/{}", user)))?;
            } else {
                handler.prepare_user(&user, service.spec.user.group, Option::<&str>::None)?;
            }

            if service.spec.extra_groups.len() > 0 {
                handler.create_groups(service.spec.extra_groups.iter().filter(|(_, cfg)| cfg.create).map(|(group, _)| group.expand_to_cow(instance.constants_by_variant())))?;
                handler.add_user_to_groups(&user, service.spec.extra_groups.iter().map(|(group, _)| group.expand_to_cow(instance.constants_by_variant())))?;
            }
        }

        assert!(service.spec.databases.len() < 2, "More than one database not supported yet");
        if let Some((db_type, db_config)) = service.spec.databases.iter().next() {
            handler.prepare_database(&service, &db_type, &db_config)?;
        }
    }

    let patches = match &instance.spec {
        PackageSpec::Service(spec) => &spec.patch_foreign,
        PackageSpec::ConfExt(spec) => &spec.patch_foreign,
        PackageSpec::Base(spec) => &spec.patch_foreign,
    };

    handler.patch_files(&instance.name, patches)?;

    let apparmor_needs_reload = patches.keys().any(|file| file.starts_with("/etc/apparmor.d/"));
    if apparmor_needs_reload {
        handler.reload_apparmor()?;
    }

    handle_config(&mut handler, instance)?;

    let alternatives = match &instance.spec {
        PackageSpec::Service(spec) => &spec.alternatives,
        PackageSpec::ConfExt(spec) => &spec.alternatives,
        PackageSpec::Base(spec) => &spec.alternatives,
    };

    handler.register_alternatives(alternatives)?;

    if let Some(service) = instance.as_service() {
        if !service.spec.refuse_manual_start && !service.spec.refuse_manual_stop {
            handler.restart_service_if_needed(&service)?;
        }
    }

    handler.trigger_config_changed(instance)?;
    handler.finish()
}
