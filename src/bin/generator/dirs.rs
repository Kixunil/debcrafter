use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageConfig, ConfType, postinst::Package};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    let additional_dirs = match &instance.spec {
        debcrafter::PackageSpec::Service(spec) => &spec.add_dirs,
        debcrafter::PackageSpec::ConfExt(spec) => &spec.add_dirs,
        debcrafter::PackageSpec::Base(spec) => &spec.add_dirs,
    };

    for dir in additional_dirs {
        writeln!(out, "{}", dir)?;
    }

    Ok(())
}
