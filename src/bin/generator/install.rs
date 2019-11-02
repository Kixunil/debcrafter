use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();
    for (file_name, conf) in instance.config() {
        if let ConfType::Static { internal, .. } = &conf.conf_type {
            let dir = file_name.rfind('/').map(|pos| &file_name[..pos+1]).unwrap_or("");
            writeln!(out, "{}/etc/{}/{} /etc/{}/{}", instance.name, instance.config_sub_dir(), file_name, instance.config_sub_dir(), dir)?;
            if *internal {
                writeln!(out, "{}/usr/share/{}/internal_config/{} /usr/share/{}/internal_config/{}", instance.name, instance.internal_config_sub_dir(), file_name, instance.internal_config_sub_dir(), dir)?;
            }
        }
    }

    Ok(())
}
