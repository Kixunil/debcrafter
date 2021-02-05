use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();
    for (file_name, conf) in instance.config() {
        let file_name = file_name.expand_to_cow(instance.constants_by_variant());
        if let ConfType::Static { internal, .. } = &conf.conf_type {
            let dir = file_name.rfind('/').map(|pos| &file_name[..pos+1]).unwrap_or("");
            if *internal {
                writeln!(out, "{}/usr/share/{}/internal_config/{} /usr/share/{}/internal_config/{}", instance.name, instance.internal_config_sub_dir(), file_name, instance.internal_config_sub_dir(), dir)?;
            } else {
                writeln!(out, "{}/etc/{}/{} /etc/{}/{}", instance.name, instance.config_sub_dir(), file_name, instance.config_sub_dir(), dir)?;
            }
        }
    }

    if !instance.databases().is_empty() {
        writeln!(out, "{}/usr/share/{}/dbconfig-common/template /usr/share/{}/dbconfig-common", instance.name, instance.internal_config_sub_dir(), instance.internal_config_sub_dir())?;
    }

    let additional_files = match &instance.spec {
        debcrafter::PackageSpec::Service(spec) => &spec.add_files,
        debcrafter::PackageSpec::ConfExt(spec) => &spec.add_files,
        debcrafter::PackageSpec::Base(spec) => &spec.add_files,
    };

    for file in additional_files {
        writeln!(out, "{}", file.expand(instance.constants_by_variant()))?;
    }

    Ok(())
}
