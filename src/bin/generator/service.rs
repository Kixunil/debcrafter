use std::io::{self, Write};
use std::borrow::Cow;
use debcrafter::im_repr::{PackageOps, PackageInstance, ConstantsByVariant, ConfType, Config};
use debcrafter::{Map, Set};
use crate::codegen::{LazyCreateBuilder};
use debcrafter::template::TemplateString;

fn filter_configs<'a>(configs: &'a Map<TemplateString, Config>, conf_dir: Option<&str>, constants_by_variant: ConstantsByVariant<'_>) -> Set<Cow<'a, str>> {
    let mut result = configs.iter().filter(|(_, config)| !config.external).map(|(k, _)| k.expand_to_cow(&constants_by_variant)).collect::<Set<_>>();
    let non_zero = result.len() > 0;
    let mut filter_dirs = conf_dir.into_iter().collect::<Set<_>>();
    for (_, config) in configs {
        if let ConfType::Dynamic { cat_dir, cat_files, .. } = &config.conf_type {
            if let Some(dir) = cat_dir {
                filter_dirs.insert(dir.trim_end_matches('/'));
            }

            for file in cat_files {
                result.remove(&**file);
            }
        }
    }

    let result = result.into_iter().filter(|item| item.rfind('/').map(|pos| !filter_dirs.contains(&item[..pos])).unwrap_or(true)).collect::<Set<_>>();
    if result.len() == 0 && non_zero {
        eprintln!("Warning: All config files elliminated. Circular dependencies?")
    }
    result
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    if let Some(instance) = instance.as_service() {
        let mut out = out.finalize();

        fn write_kv_opt<W: Write, D: std::fmt::Display>(mut out: W, name: &str, opt: Option<D>) -> io::Result<()> {
            if let Some(value) = opt {
                writeln!(out, "{}={}", name, value)?;
            }
            Ok(())
        }

        writeln!(out, "[Unit]")?;
        write_kv_opt(&mut out, "Description", instance.spec.summary.as_ref().map(|summary| summary.expand(instance.constants_by_variant())))?;
        write_kv_opt(&mut out, "After", instance.spec.after.as_ref().map(|template| template.expand(instance.constants_by_variant())))?;
        write_kv_opt(&mut out, "Before", instance.spec.before.as_ref().map(|template| template.expand(instance.constants_by_variant())))?;
        write_kv_opt(&mut out, "Wants", instance.spec.wants.as_ref().map(|template| template.expand(instance.constants_by_variant())))?;
        write_kv_opt(&mut out, "BindsTo", instance.spec.binds_to.as_ref().map(|template| template.expand(instance.constants_by_variant())))?;
        write_kv_opt(&mut out, "PartOf", instance.spec.part_of.as_ref().map(|template| template.expand(instance.constants_by_variant())))?;
        write_kv_opt(&mut out, "ConditionPathExists", instance.spec.condition_path_exists.as_ref().map(|template| template.expand(instance.constants_by_variant())))?;
        if instance.spec.refuse_manual_start {
            writeln!(out, "RefuseManualStart=true")?;
        }
        if instance.spec.refuse_manual_stop {
            writeln!(out, "RefuseManualStop=true")?;
        }
        writeln!(out)?;
        writeln!(out, "[Service]")?;
        if let Some(service_type) = &instance.spec.service_type {
            writeln!(out, "Type={}", service_type)?;
        } else {
            writeln!(out, "Type=exec")?;
        }
        write!(out, "ExecStart={}", instance.spec.binary)?;
        let conf_dir_name = if let Some(conf_dir) = &instance.spec.conf_d {
            if conf_dir.param.ends_with('=') {
                write!(out, " {}/etc/{}/{}", conf_dir.param, instance.name, conf_dir.name)?;
            } else {
                write!(out, " {} /etc/{}/{}", conf_dir.param, instance.name, conf_dir.name)?;
            }
            Some(conf_dir.name.as_ref())
        } else {
            None
        };

        let param = match (&instance.spec.conf_param, instance.spec.bare_conf_param) {
            (None, false) => None,
            (None, true) => Some(("", "")),
            (Some(param), false) if param.ends_with('=') => Some((&**param, "")),
            (Some(param), false) => Some((&**param, " ")),
            (Some(_), true) => panic!("Can not use both conf_param and bare_conf_param"),
        };

        if let Some((param, separator)) = param {
            for file in filter_configs(&instance.spec.config, conf_dir_name, instance.constants_by_variant()) {
                write!(out, " {}{}/etc/{}/{}", param, separator, instance.name, file)?;
            }
        }
        writeln!(out, " $DEBCRAFTER_EXTRA_SERVICE_ARGS")?;
        write_kv_opt(&mut out, "ExecStop", instance.spec.exec_stop.as_ref())?;

        writeln!(out, "User={}", instance.user_name())?;
        if instance.spec.user.group {
            writeln!(out, "Group={}", instance.user_name())?;
        }

        if !instance.spec.allow_suid_sgid {
            writeln!(out, "NoNewPrivileges=true")?;
        }
        writeln!(out, "ProtectSystem=full")?;
        writeln!(out, "ProtectHome=true")?;
        writeln!(out, "PrivateTmp=true")?;
        writeln!(out, "PrivateDevices=true")?;
        if let Some(runtime_dir) = &instance.spec.runtime_dir {
            writeln!(out, "RuntimeDirectory={}", instance.service_name())?;
            writeln!(out, "RuntimeDirectoryMode={}", runtime_dir.mode)?;
        }

        if let Some(extra) = &instance.spec.extra_service_config {
            writeln!(out, "{}", extra.expand(instance.constants_by_variant()))?;
        }

        writeln!(out)?;
        writeln!(out, "[Install]")?;
        if let Some(wanted_by) = &instance.spec.wanted_by {
            writeln!(out, "WantedBy={}", wanted_by.expand(instance.constants_by_variant()))?;
        } else {
            writeln!(out, "WantedBy=multi-user.target")?;
        }
    }

    Ok(())
}
