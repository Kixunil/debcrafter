use std::io::{self, Write};
use debcrafter::{PackageInstance, ConfType, Config};
use crate::codegen::{LazyCreateBuilder};
use std::collections::{HashSet, HashMap};

fn filter_configs<'a>(configs: &'a HashMap<String, Config>, conf_dir: Option<&str>) -> HashSet<&'a str> {
    let mut result = configs.iter().map(|(k, _)| k.as_ref()).collect::<HashSet<&str>>();
    let non_zero = result.len() > 0;
    let mut filter_dirs = conf_dir.into_iter().collect::<HashSet<_>>();
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

    result.retain(|item| item.rfind('/').map(|pos| !filter_dirs.contains(&item[..pos])).unwrap_or(true));
    if result.len() == 0 && non_zero {
        eprintln!("Warning: All config files elliminated. Circular dependencies?")
    }
    result
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    if let Some(instance) = instance.as_service() {
        let mut out = out.finalize();

        writeln!(out, "[Unit]")?;
        if let Some(summary) = &instance.spec.summary {
            writeln!(out, "Description={}", summary)?;
        }
        if let Some(after) = &instance.spec.after {
            writeln!(out, "After={}", after)?;
        }
        writeln!(out)?;
        writeln!(out, "[Service]")?;
        writeln!(out, "Type=exec")?;
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

        writeln!(out, "User={}", instance.user_name())?;
        if instance.spec.user.group {
            writeln!(out, "Group={}", instance.user_name())?;
        }

        if let Some(extra) = &instance.spec.extra_service_config {
            writeln!(out, "{}", extra)?;
        }

        writeln!(out)?;
        writeln!(out, "[Install]")?;
        writeln!(out, "WantedBy=multi-user.target")?;
    }

    Ok(())
}
