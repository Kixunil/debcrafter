use std::io::{self, Write};
use debcrafter::im_repr::{PackageSpec, PackageInstance, PackageOps};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    let additional_dirs = match &instance.spec {
        PackageSpec::Service(spec) => &spec.add_dirs,
        PackageSpec::ConfExt(spec) => &spec.add_dirs,
        PackageSpec::Base(spec) => &spec.add_dirs,
    };

    for dir in additional_dirs {
        writeln!(out, "{}", dir.expand(instance.constants_by_variant()))?;
    }

    Ok(())
}
