use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, PackageConfig, ConfType, PackageOps};
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

    for file in instance.add_files {
        writeln!(out, "{}", file.expand(instance.constants_by_variant()))?;
    }

    for files in instance.import_files {
        let file = files[1].expand_to_cow(instance.constants_by_variant());
        let pos = file.rfind('/').expect("dest import path must be absolute");
        let dir = &file[..pos];
        writeln!(out, "debian/debcrafter/external-files/{} {}", file, dir)?;
    }

    Ok(())
}
