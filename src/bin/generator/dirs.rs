use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, PackageOps};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    for dir in instance.add_dirs {
        writeln!(out, "{}", dir.expand(instance.constants_by_variant()))?;
    }

    Ok(())
}
