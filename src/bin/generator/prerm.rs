use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageSpec, ConfType, postinst::Package, GeneratedType};
use crate::codegen::{LazyCreateBuilder};
use std::borrow::Cow;
use std::collections::HashSet;

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let out = out.set_header("#!/bin/bash\n\nset -e\n\n");
    let mut out = out.finalize();

    let alternatives = match &instance.spec {
        PackageSpec::Service(spec) => &spec.alternatives,
        PackageSpec::ConfExt(spec) => &spec.alternatives,
        PackageSpec::Base(spec) => &spec.alternatives,
    };

    let mut written = false;

    for (provider, alternative) in alternatives {
        if !written {
            writeln!(out, "if [ \"$1\" = remove ] || [ \"$1\" = deconfigure ];")?;
            writeln!(out, "then")?;
            written = true;
        }

        writeln!(out, "update-alternatives --remove \"{}\" \"{}\"", alternative.name, provider)?;
    }

    if written {
        writeln!(out, "fi")?;
    }

    Ok(())
}
