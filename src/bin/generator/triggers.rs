use std::io::{self, Write};
use debcrafter::{PackageInstance, ConfType};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    use std::collections::HashSet;

    if let Some(instance) = instance.as_service() {
        let mut out = out.finalize();
        let mut dirs = HashSet::new();
        let mut files_no_await = HashSet::new();
        let mut files_await = HashSet::new();

        files_no_await.insert(instance.spec.binary.clone());
        if let Some(conf_dir) = &instance.spec.conf_d {
            dirs.insert(format!("/etc/{}/{}", instance.name, conf_dir.name.trim_end_matches('/')));
        }
        for (file, config) in &instance.spec.config {
            match &config.conf_type {
                ConfType::Static { .. } =>  {
                    files_await.insert(format!("/etc/{}/{}", instance.name, file));
                },
                ConfType::Dynamic { evars, cat_dir, cat_files, .. } =>  {
                    for (package, _) in evars {
                        writeln!(out, "interest-noawait {}-config-changed", package)?;
                    }
                    if let Some(cat_dir) = cat_dir {
                        dirs.insert(format!("/etc/{}/{}", instance.name, cat_dir.trim_end_matches('/')));
                    }
                    for file in cat_files {
                        files_await.insert(format!("/etc/{}/{}", instance.name, file));
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
