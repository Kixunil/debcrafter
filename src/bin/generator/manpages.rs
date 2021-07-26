use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    for manpage in instance.add_manpages {
        writeln!(out, "{}", manpage)?;
    }

    Ok(())
}
