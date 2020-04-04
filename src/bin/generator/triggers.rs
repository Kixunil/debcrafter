use std::io::{self, Write};
use debcrafter::{PackageInstance, ConfType, FileVar};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    use std::collections::HashSet;
    use debcrafter::PackageConfig;
    use debcrafter::postinst::Package;

    let mut dirs = HashSet::new();
    let mut files_no_await = HashSet::new();
    let mut files_await = HashSet::new();

    // A package needs triggers only if it's doing something "interesting".
    // Currently it's either being a service or having postprocess script.
    let needs_triggers = if let Some(instance) = instance.as_service() {
        files_no_await.insert(instance.spec.binary.clone());
        if let Some(conf_dir) = &instance.spec.conf_d {
            dirs.insert(format!("/etc/{}/{}", instance.name, conf_dir.name.trim_end_matches('/')));
        }
        true
    } else {
        instance
            .config()
            .values()
            .find(|config| match &config.conf_type {
                ConfType::Dynamic { postprocess: Some(_), .. } => true,
                ConfType::Dynamic { cat_dir: Some(_), .. } => true,
                ConfType::Dynamic { cat_files, .. } if cat_files.len() > 0 => true,
                _ => false,
            })
            .is_some()
    };

    if needs_triggers {
        let mut out = out.finalize();

        for (file, config) in instance.config() {
            match &config.conf_type {
                ConfType::Static { .. } =>  {
                    files_await.insert(format!("/etc/{}/{}", instance.config_sub_dir(), file));
                },
                ConfType::Dynamic { evars, cat_dir, cat_files, fvars, .. } =>  {
                    for (package, _) in evars {
                        writeln!(out, "interest-noawait {}-config-changed", package)?;
                    }
                    if let Some(cat_dir) = cat_dir {
                        dirs.insert(format!("/etc/{}/{}", instance.config_sub_dir(), cat_dir.trim_end_matches('/')));
                    }
                    for file in cat_files {
                        files_await.insert(format!("/etc/{}/{}", instance.config_sub_dir(), file));
                    }
                    for (_, var) in fvars {
                        match var {
                            FileVar::Dir { path, .. } => {
                                dirs.insert(format!("/etc/{}/{}", instance.config_sub_dir(), path.trim_end_matches('/')));
                            },
                        }
                    }
                },
            }
        }

        for trigger in &dirs {
            writeln!(out, "interest {}", trigger)?;
        }
        for trigger in files_await {
            if let Some(pos) = trigger.rfind('/') {
                if dirs.contains(&trigger[..pos]) {
                    continue;
                }
            }

            writeln!(out, "interest {}", trigger)?;
        }
        for trigger in files_no_await {
            if let Some(pos) = trigger.rfind('/') {
                if dirs.contains(&trigger[..pos]) {
                    continue;
                }
            }

            writeln!(out, "interest-noawait {}", trigger)?;
        }
    }
    Ok(())
}
