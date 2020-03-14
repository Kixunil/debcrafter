use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let out = out.set_header("#!/bin/bash\n\nif [ \"$1\" = purge ];\nthen\n");
    let mut out = out.finalize();
    let mut trigger_dir = false;
    for (file_name, conf) in instance.config() {
        if let ConfType::Dynamic { .. } = &conf.conf_type {
            writeln!(out, "\trm -f /etc/{}/{}", instance.config_sub_dir(), file_name)?;
            writeln!(out, "\tdpkg-trigger /etc/{}/{}", instance.config_sub_dir(), file_name)?;
            if let Some(pos) = file_name.rfind('/') {
                // We need to trigger the relevant directories in order to update other packages
                writeln!(out, "\tdpkg-trigger /etc/{}/{}", instance.config_sub_dir(), &file_name[..pos])?;
            } else {
                trigger_dir = true;
            }
        }
    }

    if trigger_dir {
        writeln!(out, "\tdpkg-trigger /etc/{}", instance.config_sub_dir())?;
    }

    if let Some(out) = out.created() {
        writeln!(out, "fi")?;
        writeln!(out)?;
        writeln!(out, "#DEBHELPER#")?;
        writeln!(out)?;
        writeln!(out, "exit 0")?;
    }

    Ok(())
}
