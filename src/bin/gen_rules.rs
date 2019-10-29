use codegen::{GenFileName, LazyCreateBuilder};
use debcrafter::PackageInstance;
use debcrafter::postinst::Package;
use std::io::{self, Write};

mod codegen;

fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    writeln!(out, "#!/usr/bin/make -f")?;
    writeln!(out)?;
    writeln!(out, "%:")?;
    if instance.as_service().is_some() {
        writeln!(out, "\tdh $@ --with systemd")?;
    }
    if let Some(name) = instance.service_name() {
        writeln!(out)?;
        writeln!(out, "override_dh_systemd_enable:")?;
        writeln!(out, "\tdh_systemd_enable --name={}", name)?;
    }
    Ok(())
}

fn main() {
    codegen::generate(GenFileName::Raw("rules"), generate);
}
