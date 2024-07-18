use std::io;
use debcrafter::im_repr::{PackageInstance, PackageOps};
use crate::codegen::{LazyCreateBuilder};

/*
fn write_patches<W: io::Write>(mut out: W, instance: &PackageInstance) -> io::Result<()> {
    use debcrafter::PackageSpec;

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
*/

pub fn generate(_instance: &PackageInstance, _out: LazyCreateBuilder) -> io::Result<()> {
    //let out = out.set_header("#!/bin/bash\n\nset -e\n\n");
    //let mut out = out.finalize();

    // "use of dbc preinst hooks is now unnecesary and deprecated."

    // This is actually a bad idea because it could leave the dependency in a corrupt state
    // but maybe there's a reason to activate it in the future?
    //write_patches(&mut out, instance)?;

    Ok(())
}
