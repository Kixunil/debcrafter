use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, PackageConfig, ConfType, PackageOps};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();
    for (file_name, conf) in instance.config() {
        if let ConfType::Static { internal, .. } = &conf.conf_type {
            if *internal {
                writeln!(out, "/usr/share/{}/internal_config/{} /etc/{}/{}", instance.internal_config_sub_dir(), file_name.expand(instance.constants_by_variant()), instance.internal_config_sub_dir(), file_name.expand(instance.constants_by_variant()))?;
            }
        }
    }

    for link in instance.add_links {
        writeln!(out, "{}", link.expand(instance.constants_by_variant()))?;
    }

    Ok(())
}
