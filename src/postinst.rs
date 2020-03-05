use crate::{PackageInstance, ServiceInstance, PackageSpec, ConfType, VarType, ConfFormat, FileType, HiddenVarVal, PackageConfig, DbConfig};
use std::fmt;
use std::borrow::Cow;

#[derive(Copy, Clone)]
pub struct Config<'a> {
    pub package_name: &'a str,
    pub file_name: &'a str,
    pub format: &'a ConfFormat,
    pub public: bool,
    pub change_group: Option<&'a str>,
    pub extension: bool,
}

pub trait HandlePostinst: Sized {
    type Error: fmt::Debug + fmt::Display;

    fn prepare_user<T: fmt::Display>(&mut self, name: &str, group: bool, home: Option<T>) -> Result<(), Self::Error>;
    fn add_user_to_groups<I>(&mut self, user: &str, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str>;
    fn create_groups<I>(&mut self, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str>;
    fn prepare_database(&mut self, instance: &ServiceInstance, name: &str, config: &DbConfig) -> Result<(), Self::Error>;
    fn prepare_config(&mut self, config: &Config) -> Result<(), Self::Error>;
    fn write_internal_var(&mut self, config: &Config, name: &str, ty: &VarType, ignore_empty: bool) -> Result<(), Self::Error>;
    fn write_external_var(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, rename: &Option<String>) -> Result<(), Self::Error>;
    fn fetch_external_var(&mut self, config: &Config, package: &str, name: &str) -> Result<(), Self::Error>;
    fn restart_service_if_needed(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error>;
    fn trigger_config_changed(&mut self, instance: &PackageInstance) -> Result<(), Self::Error>;
    fn write_hidden_const(&mut self, config: &Config, name: &str, ty: &VarType, val: &str) -> Result<(), Self::Error>;
    fn write_hidden_script(&mut self, config: &Config, name: &str, ty: &VarType, script: &str) -> Result<(), Self::Error>;
    fn include_conf_dir<T: fmt::Display>(&mut self, config: &Config, dir: T) -> Result<(), Self::Error>;
    fn include_conf_file<T: fmt::Display>(&mut self, config: &Config, file: T) -> Result<(), Self::Error>;
    fn postprocess_conf_file(&mut self, config: &Config, command: &[String]) -> Result<(), Self::Error>;
    fn write_comment(&mut self, config: &Config, comment: &str) -> Result<(), Self::Error>;
    fn create_path(&mut self, config: &Config, var_name: &str, file_type: &FileType, mode: u16, owner: &str, group: &str, only_parent: bool) -> Result<(), Self::Error>;
    fn finish(self) -> Result<(), Self::Error>;
}

pub trait Package<'a>: PackageConfig {
    fn config_pkg_name(&self) -> &str;
    fn config_sub_dir(&self) -> Cow<'a, str>;
    fn internal_config_sub_dir(&self) -> Cow<'a, str>;
    fn service_name(&self) -> Option<&str>;
    fn service_user(&self) -> Option<&str>;
    fn service_group(&self) -> Option<&str>;
    fn get_include(&self, name: &str) -> Option<&super::Package>;
    fn is_conf_ext(&self) -> bool;
}

impl<'a> Package<'a> for ServiceInstance<'a> {
    fn config_pkg_name(&self) -> &str {
        &self.name
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

    fn service_user(&self) -> Option<&str> {
        Some(self.user_name())
    }

    fn service_group(&self) -> Option<&str> {
        if self.spec.user.group {
            Some(self.user_name())
        } else {
            None
        }
    }

    fn get_include(&self, name: &str) -> Option<&super::Package> {
        self.includes.as_ref().and_then(|includes| includes.get(name))
    }

    fn is_conf_ext(&self) -> bool {
        false
    }
}

impl<'a> Package<'a> for PackageInstance<'a> {
    fn config_pkg_name(&self) -> &str {
        &self.name
    }

    fn config_sub_dir(&self) -> Cow<'a, str> {
        if let PackageSpec::ConfExt(confext) = &self.spec {
            if confext.external {
                "/".into()
            } else {
                self
                    .get_include(&confext.extends)
                    .unwrap_or_else(|| panic!("Package {} extended by {} not found", confext.extends, self.name))
                    .instantiate(self.variant, None)
                    .unwrap_or_else(|| panic!("Package {} extended by {} doesn't know variant {}", confext.extends, self.name, self.variant.unwrap()))
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
                    .unwrap_or_else(|| panic!("Package {} extended by {} not found", confext.extends, self.name))
                    .instantiate(self.variant, None)
                    .unwrap_or_else(|| panic!("Package {} extended by {} doesn't know variant {}", confext.extends, self.name, self.variant.unwrap()))
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

    fn service_user(&self) -> Option<&str> {
        self.as_service().map(|service| service.user_name())
    }

    fn service_group(&self) -> Option<&str> {
        self.as_service().and_then(|service| ServiceInstance::service_group(&service))
    }

    fn get_include(&self, name: &str) -> Option<&super::Package> {
        self.includes.as_ref().and_then(|includes| includes.get(name))
    }

    fn is_conf_ext(&self) -> bool {
        if let PackageSpec::ConfExt(_) = &self.spec {
            true
        } else {
            false
        }
    }
}

fn handle_config<'a, T: HandlePostinst, P: Package<'a>>(handler: &mut T, package: &P) -> Result<(), T::Error> {
    for (conf_name, config) in package.config() {
        if let ConfType::Dynamic { ivars, evars, hvars, format, comment, cat_dir, cat_files, postprocess, .. } = &config.conf_type {
            let file_name = format!("/etc/{}/{}", package.config_sub_dir(), conf_name);
            let config_ctx = Config {
                package_name: package.config_pkg_name(),
                file_name: &file_name,
                format,
                public: config.public,
                extension: package.is_conf_ext(),
                change_group: package.service_group(),
            };
            handler.prepare_config(&config_ctx)?;
            if let Some(comment) = comment {
                handler.write_comment(&config_ctx, comment)?;
            }

            if let Some(cat_dir) = cat_dir {
                handler.include_conf_dir(&config_ctx, format_args!("/etc/{}/{}", package.config_sub_dir(), cat_dir))?;
            }

            for file in cat_files {
                handler.include_conf_file(&config_ctx, format_args!("/etc/{}/{}", package.config_sub_dir(), file))?;
            }

            for (var, var_spec) in ivars {
                handler.write_internal_var(&config_ctx, var, &var_spec.ty, var_spec.ignore_empty)?;
            }

            for (pkg_name, vars) in evars {
                let pkg = package.get_include(pkg_name).expect("Package not found");

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
                            .unwrap_or_else(|| panic!("Variable {} not found in {}", var, pkg_name))
                            .ty;

                        handler.write_external_var(&config_ctx, pkg_name, var, ty, &var_spec.name)?;
                    } else {
                        handler.fetch_external_var(&config_ctx, pkg_name, var)?;
                    }
                }

            }

            for (var, var_spec) in hvars {
                match &var_spec.val {
                    HiddenVarVal::Constant(val) => handler.write_hidden_const(&config_ctx, var, &var_spec.ty, val)?,
                    HiddenVarVal::Script(script) => handler.write_hidden_script(&config_ctx, var, &var_spec.ty, script)?,
                }
            }

            for (var, var_spec) in ivars {
                match &var_spec.ty {
                    VarType::Path { file_type: Some(file_type), create: Some(create) } => {
                        let owner = if create.owner == "$service" {
                            package.service_user().expect("Attempt to use service user but the package is not a service.")
                        } else {
                            &create.owner
                        };

                        let group = if create.group == "$service" {
                            package.service_user().expect("Attempt to use service group but it's missing or the package is not a service.")
                        } else {
                            &create.group
                        };

                        handler.create_path(&config_ctx, var, file_type, create.mode, owner, group, create.only_parent)?;
                    },
                    VarType::Path { file_type: None, create: Some(_) } => panic!("Invalid specification: path can't be created without specifying type"),
                    _ => (),
                }
            }

            if let Some(postprocess) = postprocess {
                handler.postprocess_conf_file(&config_ctx, postprocess)?;
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
                handler.prepare_user(user, service.spec.user.group, Some(format_args!("/var/lib/{}", user)))?;
            } else {
                handler.prepare_user(user, service.spec.user.group, Option::<&str>::None)?;
            }

            if service.spec.extra_groups.len() > 0 {
                handler.create_groups(service.spec.extra_groups.iter().filter(|(_, cfg)| cfg.create).map(|(group, _)| group))?;
                handler.add_user_to_groups(user, service.spec.extra_groups.iter().map(|(group, _)| group))?;
            }
        }

        assert!(service.spec.databases.len() < 2, "More than one database not supported yet");
        if let Some((db_type, db_config)) = service.spec.databases.iter().next() {
            handler.prepare_database(&service, &db_type, &db_config)?;
        }
    }

    handle_config(&mut handler, instance)?;

    if let Some(service) = instance.as_service() {
        handler.restart_service_if_needed(&service)?;
    }

    handler.trigger_config_changed(instance)?;
    handler.finish()
}
