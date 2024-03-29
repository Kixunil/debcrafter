use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, ConfType, FileVar};
use debcrafter::Set;
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    use debcrafter::im_repr::{PackageConfig, PackageOps};

    let mut dirs = Set::new();
    let mut files_no_await = Set::new();
    let mut files_await = Set::new();
    let mut configs_changed = Set::new();

    // A package needs triggers only if it's doing something "interesting".
    // Currently it's either being a service or having postprocess script.
    let needs_triggers = if let Some(instance) = instance.as_service() {
        files_no_await.insert(instance.spec.binary.clone());
        if let Some(conf_dir) = &instance.spec.conf_d {
            dirs.insert(format!("/etc/{}/{}", instance.name, conf_dir.name.trim_end_matches('/')));
        }
        true
    } else {
        let has_patches = !instance.patch_foreign.is_empty();

        has_patches || instance
            .config()
            .values()
            .any(|config| match &config.conf_type {
                ConfType::Dynamic { postprocess: Some(_), .. } => true,
                ConfType::Dynamic { cat_dir: Some(_), .. } => true,
                ConfType::Dynamic { cat_files, .. } if !cat_files.is_empty() => true,
                _ => false,
            }) || !instance.extended_by.is_empty() || !instance.extra_triggers.is_empty()
    };

    if needs_triggers {
        let mut out = out.finalize();

        for package in instance.extended_by {
            configs_changed.insert(package);
        }

        for (file, config) in instance.config() {
            match &config.conf_type {
                ConfType::Static { .. } =>  {
                    files_await.insert(format!("/etc/{}/{}", instance.config_sub_dir(), file.expand(instance.constants_by_variant())));
                },
                ConfType::Dynamic { evars, cat_dir, cat_files, fvars, .. } =>  {
                    for package in evars.keys() {
                        writeln!(out, "interest-noawait {}-config-changed", package.expand_to_cow(instance.variant()))?;
                    }
                    if let Some(cat_dir) = cat_dir {
                        dirs.insert(format!("/etc/{}/{}", instance.config_sub_dir(), cat_dir.trim_end_matches('/')));
                    }
                    for file in cat_files {
                        files_await.insert(format!("/etc/{}/{}", instance.config_sub_dir(), file));
                    }
                    for var in fvars.values() {
                        match var {
                            FileVar::Dir { path, .. } => {
                                dirs.insert(format!("/etc/{}/{}", instance.config_sub_dir(), path.trim_end_matches('/')));
                            },
                        }
                    }
                },
            }
        }

        for dest in instance.patch_foreign.keys() {
            files_no_await.insert(format!("{}.distrib", dest));
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
        for trigger in &configs_changed {
            writeln!(out, "interest-noawait {}-config-changed", trigger.expand(instance.constants_by_variant()))?;
        }

        for trigger in instance.extra_triggers {
            writeln!(out, "interest-noawait {}", trigger.expand(instance.constants_by_variant()))?;
        }
    }
    Ok(())
}
