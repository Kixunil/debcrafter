use std::io::{self, Write};
use debcrafter::{PackageInstance, ConfType, FileVar, Set};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    use debcrafter::PackageConfig;
    use debcrafter::postinst::Package;

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
        let has_patches = match &instance.spec {
            debcrafter::PackageSpec::Service(spec) => !spec.patch_foreign.is_empty(),
            debcrafter::PackageSpec::ConfExt(spec) => !spec.patch_foreign.is_empty(),
            debcrafter::PackageSpec::Base(spec) => !spec.patch_foreign.is_empty(),
        };
        has_patches || instance
            .config()
            .values()
            .find(|config| match &config.conf_type {
                ConfType::Dynamic { postprocess: Some(_), .. } => true,
                ConfType::Dynamic { cat_dir: Some(_), .. } => true,
                ConfType::Dynamic { cat_files, .. } if cat_files.len() > 0 => true,
                _ => false,
            })
            .is_some() || !instance.extended_by.is_empty() || !instance.extra_triggers.is_empty()
    };

    if needs_triggers {
        let mut out = out.finalize();

        for package in instance.extended_by {
            configs_changed.insert(package);
        }

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

        let patches = match &instance.spec {
            debcrafter::PackageSpec::Service(spec) => &spec.patch_foreign,
            debcrafter::PackageSpec::ConfExt(spec) => &spec.patch_foreign,
            debcrafter::PackageSpec::Base(spec) => &spec.patch_foreign,
        };

        for (dest, _) in patches {
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
            writeln!(out, "interest-noawait {}-config-changed", trigger)?;
        }

        for trigger in instance.extra_triggers {
            writeln!(out, "interest-noawait {}", trigger)?;
        }
    }
    Ok(())
}
