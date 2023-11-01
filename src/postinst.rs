use crate::{Set, Map};
use crate::types::{NonEmptyMap, VPackageName, VarName};
use crate::im_repr::{PackageOps, PackageInstance, PackageConfig, ServiceInstance, ConstantsByVariant, ConfType, VarType, ConfFormat, FileType, HiddenVarVal, FileVar, GeneratedType, ExtraGroup, Database, MigrationVersion, Migration, Alternative, PostProcess, InternalVarCondition};
use std::fmt;
use std::borrow::Cow;
use either::Either;
use std::cmp::Ordering;
use std::convert::TryFrom;
use crate::template::TemplateString;

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

pub struct CreateDbRequest<'a> {
    pub pkg_name: &'a str,
    pub db_type: &'a Database,
    pub config_path: &'a str,
    pub config_mode: &'a str,
    pub config_owner: &'a str,
    pub config_group: &'a str,
    pub config_template: &'a str,
}

pub struct CommandPrivileges<'a> {
    pub user: &'a str,
    pub group: &'a str,
    pub allow_new_privileges: bool,
    pub read_only_root: bool,
}

pub struct CommandEnv<'a> {
    pub restrict_privileges: Option<CommandPrivileges<'a>>,
}

pub trait HandlePostinst: Sized {
    type Error: fmt::Debug + fmt::Display;

    fn prepare_user<T: fmt::Display>(&mut self, name: &str, group: bool, home: Option<T>) -> Result<(), Self::Error>;
    fn add_user_to_groups<I>(&mut self, user: &str, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str>;
    fn create_groups<I>(&mut self, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str>;
    fn prepare_database(&mut self, request: CreateDbRequest<'_>) -> Result<(), Self::Error>;
    fn prepare_config(&mut self, config: &Config) -> Result<(), Self::Error>;
    fn finish_config(&mut self, config: &Config) -> Result<(), Self::Error>;
    fn fetch_var(&mut self, config: &Config, package: &str, name: &str) -> Result<(), Self::Error>;
    fn generate_const_var(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, val: &str) -> Result<(), Self::Error>;
    fn generate_var_using_script(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, script: &str) -> Result<(), Self::Error>;
    fn generate_var_using_template(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, template: &str, constatnts: ConstantsByVariant<'_>) -> Result<(), Self::Error>;
    fn sub_object_begin(&mut self, config: &Config, name: &str) -> Result<(), Self::Error>;
    fn sub_object_end(&mut self, config: &Config, name: &str) -> Result<(), Self::Error>;
    fn condition_begin<'a>(&mut self, instance: &impl PackageOps<'a>, conditions: &[InternalVarCondition]) -> Result<(), Self::Error>;
    fn condition_end(&mut self) -> Result<(), Self::Error>;
    fn write_var<'a, I>(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, structure: I, ignore_empty: bool) -> Result<(), Self::Error> where I: Iterator<Item=&'a str>;
    fn include_fvar<'a, I>(&mut self, config: &Config, var: &FileVar, structure: I, subdir: &str) -> Result<(), Self::Error> where I: Iterator<Item=&'a str>;
    fn reload_apparmor(&mut self) -> Result<(), Self::Error>;
    fn stop_service(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error>;
    fn restart_service_if_needed(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error>;
    fn trigger_config_changed(&mut self, instance: &PackageInstance) -> Result<(), Self::Error>;
    fn include_conf_dir<T: fmt::Display>(&mut self, config: &Config, dir: T) -> Result<(), Self::Error>;
    fn include_conf_file<T: fmt::Display>(&mut self, config: &Config, file: T) -> Result<(), Self::Error>;
    fn run_command<I>(&mut self, command: I, env: &CommandEnv<'_>) -> Result<(), Self::Error> where I: IntoIterator, I::Item: fmt::Display;
    fn write_comment(&mut self, config: &Config, comment: &str) -> Result<(), Self::Error>;
    fn register_alternatives<A, B, I>(&mut self, alternatives: I) -> Result<(), Self::Error> where I: IntoIterator<Item=(A, B)>, A: AsRef<str>, B: std::borrow::Borrow<Alternative>;
    fn patch_files<A, B, I>(&mut self, pkg_name: &str, patches: I) -> Result<(), Self::Error> where I: IntoIterator<Item=(A, B)>, A: AsRef<str>, B: AsRef<str>;
    fn activate_trigger(&mut self, trigger: &str, no_await: bool) -> Result<(), Self::Error>;
    fn create_tree(&mut self, path: &str) -> Result<(), Self::Error>;
    fn create_path(&mut self, config: &Config, var_name: &str, file_type: &FileType, mode: u16, owner: &str, group: &str, only_parent: bool) -> Result<(), Self::Error>;
    fn finalize_migrations(&mut self, migrations: &Map<MigrationVersion, Migration>, constatnts: ConstantsByVariant<'_>) -> Result<(), Self::Error>;
    fn finish(self) -> Result<(), Self::Error>;
}

fn compute_structure<'a>(name: &'a str, structure: &'a Option<Vec<String>>) -> impl Iterator<Item=&'a str> + DoubleEndedIterator + Clone + std::fmt::Debug {
    structure
        .as_ref()
        .map(|structure| Either::Left(structure.iter().map(AsRef::as_ref)))
        .unwrap_or(Either::Right(std::iter::once(name)))
}

struct WriteVar<'a, I> where I: Iterator<Item=&'a str> + Clone {
    structure: I,
    ty: WriteVarType<'a>,
    conditions: &'a [InternalVarCondition],
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

fn handle_postprocess<'a, 'b, T: HandlePostinst, P: PackageOps<'a>>(handler: &mut T, package: &P, triggers: &mut Set<Cow<'b, str>>, postprocess: &'b PostProcess) -> Result<(), T::Error> {
    for generated in &postprocess.generates {
        let path = match &generated.ty {
            GeneratedType::File(path) => path,
            GeneratedType::Dir(path) => path,
        }
        .expand_to_cow(package.constants_by_variant());
        let path = if path.starts_with('/') {
            path
        } else {
            Cow::<str>::Owned(format!("/etc/{}/{}", package.config_sub_dir(), path))
        };
        let last_slash_pos = path.rfind('/').expect("error: entered unreachable code: path always contains a slash");
        handler.create_tree(&path[..last_slash_pos])?;
        triggers.insert(path);
    }
    let env = CommandEnv {
        restrict_privileges: None,
    };
    handler.run_command(postprocess.command.iter().map(|arg| arg.expand(package.constants_by_variant())), &env)?;
    Ok(())
}

fn handle_config<'a, T: HandlePostinst, P: PackageOps<'a>>(handler: &mut T, package: &P) -> Result<(), T::Error> {
    let mut triggers = Set::<Cow<str>>::new();
    let mut interested = Set::<String>::new();
    let mut needs_stopped_service = false;

    for (conf_name, config) in package.config() {
        if let ConfType::Dynamic { ivars, evars, format, insert_header, with_header, .. } = &config.conf_type {
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

                for var in ivars.keys() {
                    handler.fetch_var(&config_ctx, config_ctx.package_name, var)?;
                }

                for (pkg_name, vars) in evars {
                    let pkg_name = pkg_name.expand_to_cow(package.variant());
                    for (var, _var_spec) in vars {
                        handler.fetch_var(&config_ctx, &pkg_name, var)?;
                    }
                }
            }
        }
    }

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

                for (var, var_spec) in hvars {
                    match &var_spec.val {
                        HiddenVarVal::Constant(val) => handler.generate_const_var(&config_ctx, config_ctx.package_name, var, &var_spec.ty, val)?,
                        HiddenVarVal::Script(script) => handler.generate_var_using_script(&config_ctx, config_ctx.package_name, var, &var_spec.ty, &script.expand_to_cow(package.constants_by_variant()))?,
                        HiddenVarVal::Template(template) => handler.generate_var_using_template(&config_ctx, config_ctx.package_name, var, &var_spec.ty, template, package.constants_by_variant())?,
                    }
                }

                let mut write_vars = Vec::new();
                let mut check_ivars = Set::new();

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

                    for cond in &var_spec.conditions {
                        if let InternalVarCondition::Var { name, .. } = cond {
                            match name {
                                VarName::Internal(var) => assert!(check_ivars.contains(&**var), "Unknown variable {:?}", name),
                                VarName::Absolute(var_package, var) if var_package.expand_to_cow(package.variant()) == package.config_pkg_name() => assert!(check_ivars.contains(&**var), "Unknown variable {:?}", name),
                                VarName::Absolute(var_package, var) => {
                                    let pkg = evars.get(var_package)
                                        .unwrap_or_else(|| panic!("Unknown variable {:?}", name));
                                    assert!(pkg.get(&**var).is_some(), "Unknown variable {:?}", name);
                                },
                                VarName::Constant(_) => panic!("constants can't be used to skip ivars"),
                            }
                        }
                    }
                    check_ivars.insert(&**var);

                    if var_spec.store {
                        write_vars.push(WriteVar {
                            structure: compute_structure(&var, &var_spec.structure),
                            ty: WriteVarType::Simple {
                                ty: &var_spec.ty,
                                package: Cow::Borrowed(config_ctx.package_name),
                                name: var,
                                ignore_empty: var_spec.ignore_empty,
                            },
                            conditions: &var_spec.conditions,
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
                                conditions: &[],
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
                                    panic!("Unknown constant {}", var);
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
                            conditions: &[],
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
                                conditions: &[],
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
                    if !var.conditions.is_empty() {
                        handler.condition_begin(package, &var.conditions)?;
                    }
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
                    
                    if !var.conditions.is_empty() {
                        handler.condition_end()?;
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
            if let ConfType::Dynamic { postprocess: Some(postprocess @ PostProcess { stop_service: true, .. }), .. } = &config.conf_type {
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

pub fn handle_groups<T: HandlePostinst>(handler: &mut T, user: &str, extra_groups: &NonEmptyMap<TemplateString, ExtraGroup, &Map<TemplateString, ExtraGroup>>, constants_by_variant: &ConstantsByVariant) -> Result<(), <T as HandlePostinst>::Error> {
    handler.create_groups(extra_groups.iter().filter(|(_, cfg)| cfg.create).map(|(group, _)| group.expand_to_cow(constants_by_variant)))?;
    handler.add_user_to_groups(user, extra_groups.iter().map(|(group, _)| group.expand_to_cow(constants_by_variant)))?;

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
        }
    }

    match (instance.extra_groups(), instance.service_user()) {
        (None, _) => (),
        (Some(_), None) => panic!("Can't set extra_groups in package {}, it is supported only for services or extension packages that extend services", instance.name),
        (Some(extra_groups), Some(user_name)) => handle_groups(&mut handler, &user_name, &extra_groups, &instance.constants_by_variant())?,
    }

    let databases = instance.databases();
    assert!(databases.len() < 2, "More than one database not supported yet");
    if let Some((db_type, db_config)) = databases.iter().next() {
        let mut config_mode = "640";

        let config_group = db_config.config_file_group
            .as_ref()
            .map(String::as_ref)
            .map(Cow::Borrowed)
            .or_else(|| instance.service_group());

        let config_owner = db_config.config_file_owner
            .as_ref()
            .map(String::as_ref)
            .map(Cow::Borrowed)
            .or_else(|| if config_group.is_none() {
                // We prefer config files to be owned by root, but being readable is more important
                let user = instance.service_user();
                if user.is_some() {
                    config_mode = "460";
                }
                user
            } else {
                None
            })
            .unwrap_or(Cow::Borrowed("root"));

        let config_group = config_group.unwrap_or(Cow::Borrowed("root"));

        let path = match instance.conf_dir() {
            Some(dir) => format!("/etc/{}/{}/database", instance.name, dir),
            None => format!("/etc/{}/database", instance.name),
        };

        let request = CreateDbRequest {
            pkg_name: &instance.name,
            db_type,
            config_path: &path,
            config_mode,
            config_owner: &config_owner,
            config_group: &config_group,
            config_template: &db_config.template,
        };
        handler.prepare_database(request)?;
    }

    handler.patch_files(&instance.name, instance.patch_foreign)?;

    let apparmor_needs_reload = instance.patch_foreign
        .keys()
        .any(|file| file.starts_with("/etc/apparmor.d/"));
    if apparmor_needs_reload {
        handler.reload_apparmor()?;
    }

    handle_config(&mut handler, instance)?;

    handler.register_alternatives(instance.alternatives)?;

    if let Some(service) = instance.as_service() {
        if !service.spec.refuse_manual_start && !service.spec.refuse_manual_stop {
            handler.restart_service_if_needed(&service)?;
        }
    }

    if instance.migrations.values().any(|migration| migration.postinst_finish.is_some()) {
        handler.finalize_migrations(&instance.migrations, instance.constants_by_variant())?;
    }

    handler.trigger_config_changed(instance)?;
    for plug in instance.plug {
        let user = plug.run_as_user.expand_to_cow(instance.constants_by_variant());
        let group;
        let restrict_privileges = if user != "root" {
            group = plug.run_as_group.as_ref().map(|group| group.expand_to_cow(instance.constants_by_variant())).unwrap_or(Cow::Borrowed(&*user));
            Some(CommandPrivileges {
                user: &user,
                group: &group,
                allow_new_privileges: false,
                read_only_root: plug.read_only_root,
            })
        } else {
            None
        };

        let env = CommandEnv {
            restrict_privileges,
        };

        handler.run_command(plug.register_cmd.iter().map(|arg| arg.expand(instance.constants_by_variant())), &env)?;
    }
    handler.finish()
}
