use std::io::{self, Write};
use debcrafter::{PackageInstance};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    let additional_manpages = match &instance.spec {
        debcrafter::PackageSpec::Service(spec) => &spec.add_manpages,
        debcrafter::PackageSpec::ConfExt(spec) => &spec.add_manpages,
        debcrafter::PackageSpec::Base(spec) => &spec.add_manpages,
    };

    for manpage in additional_manpages {
        writeln!(out, "{}", manpage)?;
    }

    Ok(())
}
