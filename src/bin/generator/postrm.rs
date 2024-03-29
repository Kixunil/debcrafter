use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, PackageConfig, ConfType, PackageOps, GeneratedType};
use debcrafter::Set;
use crate::codegen::{LazyCreateBuilder};
use std::borrow::Cow;

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let out = out.set_header("#!/bin/bash\n\nif [ \"$1\" = purge ];\nthen\n");
    let mut out = out.finalize();
    let mut triggers = Set::new();
    if let Some(custom_script) = &instance.custom_postrm_script {
        writeln!(out, "{}", custom_script.expand(instance.constants_by_variant()))?;
    }
    for (file_name, conf) in instance.config() {
        if let ConfType::Dynamic { postprocess, .. } = &conf.conf_type {
            let abs_file = format!("/etc/{}/{}", instance.config_sub_dir(), file_name.expand(instance.constants_by_variant()));
            writeln!(out, "\trm -f {}", abs_file)?;

            triggers.insert(Cow::Owned(abs_file));

            if let Some(postprocess) = postprocess {
                for generated in &postprocess.generates {
                    let (path, is_dir) = match &generated.ty {
                        GeneratedType::File(path) => (path, false),
                        GeneratedType::Dir(path) => (path, true),
                    };
                    let path = path.expand_to_cow(instance.constants_by_variant());
                    let path = if path.starts_with('/') {
                        path
                    } else {
                        Cow::<str>::Owned(format!("/etc/{}/{}", instance.config_sub_dir(), path))
                    };
                    if is_dir {
                        writeln!(out, "\trm -rf {}", path)?;
                    } else {
                        writeln!(out, "\trm -f {}", path)?;
                    }
                    triggers.insert(path);
                }
            }
        }
    }

    let mut activated = Set::new();

    for trigger in &triggers {
        writeln!(out, "\tdpkg-trigger \"`realpath \"{}\"`\"", trigger)?;
        if let Some(pos) = trigger.rfind('/') {
            let parent = &trigger[..pos];
            if parent != instance.config_sub_dir() && !triggers.contains(parent) && !activated.contains(parent) {
                writeln!(out, "\tdpkg-trigger \"`realpath \"{}\"`\"", parent)?;
                activated.insert(parent);
            }
        }
    }

    writeln!(out, "\tdpkg-trigger \"{}-config-changed\"", instance.name)?;

    if let Some(out) = out.created() {
        writeln!(out, "fi")?;
        writeln!(out)?;
        writeln!(out, "#DEBHELPER#")?;
        writeln!(out)?;
        writeln!(out, "exit 0")?;
    }

    Ok(())
}
