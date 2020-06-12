use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageSpec, ConfType, postinst::Package, GeneratedType};
use crate::codegen::{LazyCreateBuilder};
use std::borrow::Cow;
use std::collections::HashSet;

fn write_patches<W: io::Write>(mut out: W, instance: &PackageInstance) -> io::Result<()> {
    let patches = match &instance.spec {
        PackageSpec::Service(spec) => &spec.patch_foreign,
        PackageSpec::ConfExt(spec) => &spec.patch_foreign,
        PackageSpec::Base(spec) => &spec.patch_foreign,
    };

    for (dest, _) in patches {
        writeln!(out, "dpkg-divert --add --rename --package \"{}\" \"{}\"", instance.name, dest)?;
    }

    Ok(())
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let out = out.set_header("#!/bin/bash\n\nset -e\n\n");
    let mut out = out.finalize();

    write_patches(&mut out, instance)?;

    Ok(())
}
