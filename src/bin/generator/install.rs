use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();
    for (file_name, conf) in instance.config() {
        if let ConfType::Static { .. } = &conf.conf_type {
            let dir = file_name.rfind('/').map(|pos| &file_name[..pos+1]).unwrap_or("");
            writeln!(out, "{}/etc/{}/{} /etc/{}{}", instance.name, instance.config_sub_dir(), file_name, instance.config_sub_dir(), dir)?;
        }
    }

    Ok(())
}
