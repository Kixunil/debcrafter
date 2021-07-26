use std::io::{self, Write};
use debcrafter::im_repr::{PackageSpec, PackageInstance, PackageConfig, ConfType, PackageOps};
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

    let additional_links = match &instance.spec {
        PackageSpec::Service(spec) => &spec.add_links,
        PackageSpec::ConfExt(spec) => &spec.add_links,
        PackageSpec::Base(spec) => &spec.add_links,
    };

    for link in additional_links {
        writeln!(out, "{}", link.expand(instance.constants_by_variant()))?;
    }

    Ok(())
}
