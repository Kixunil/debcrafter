use std::io::{self, Write};
use debcrafter::{PackageInstance, ConfType, Config, Map, Set};
use crate::codegen::{LazyCreateBuilder};

fn filter_configs<'a>(configs: &'a Map<String, Config>, conf_dir: Option<&str>) -> Set<&'a str> {
    let mut result = configs.iter().map(|(k, _)| k.as_ref()).collect::<Set<&str>>();
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

        fn write_kv_opt<W: Write>(mut out: W, name: &str, opt: &Option<String>) -> io::Result<()> {
            if let Some(value) = opt {
                writeln!(out, "{}={}", name, value)?;
            }
            Ok(())
        }

        writeln!(out, "[Unit]")?;
        write_kv_opt(&mut out, "Description", &instance.spec.summary)?;
        write_kv_opt(&mut out, "After", &instance.spec.after)?;
        write_kv_opt(&mut out, "Before", &instance.spec.before)?;
        write_kv_opt(&mut out, "Wants", &instance.spec.wants)?;
        write_kv_opt(&mut out, "BindsTo", &instance.spec.binds_to)?;
        write_kv_opt(&mut out, "PartOf", &instance.spec.part_of)?;
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
        if let Some(param) = &instance.spec.conf_param {
            if param.ends_with('=') {
                for file in filter_configs(&instance.spec.config, conf_dir_name) {
                    write!(out, " {}/etc/{}/{}", param, instance.name, file)?;
                }
            } else {
                for file in filter_configs(&instance.spec.config, conf_dir_name) {
                    write!(out, " {} /etc/{}/{}", param, instance.name, file)?;
                }
            }
        }
        writeln!(out)?;
        write_kv_opt(&mut out, "ExecStop", &instance.spec.exec_stop)?;

        writeln!(out, "User={}", instance.user_name())?;
        if instance.spec.user.group {
            writeln!(out, "Group={}", instance.user_name())?;
        }

        if let Some(extra) = &instance.spec.extra_service_config {
            writeln!(out, "{}", extra)?;
        }

        writeln!(out)?;
        writeln!(out, "[Install]")?;
        if let Some(wanted_by) = &instance.spec.wanted_by {
            writeln!(out, "WantedBy={}", wanted_by)?;
        } else {
            writeln!(out, "WantedBy=multi-user.target")?;
        }
    }

    Ok(())
}
