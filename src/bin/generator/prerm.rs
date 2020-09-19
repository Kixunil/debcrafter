use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageSpec, ConfType, postinst::Package, GeneratedType, Set};
use crate::codegen::{LazyCreateBuilder};
use std::borrow::Cow;

fn write_alternatives<W: io::Write>(mut out: W, instance: &PackageInstance) -> io::Result<()> {
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

fn write_patches<W: io::Write>(mut out: W, instance: &PackageInstance) -> io::Result<()> {
    let patches = match &instance.spec {
        PackageSpec::Service(spec) => &spec.patch_foreign,
        PackageSpec::ConfExt(spec) => &spec.patch_foreign,
        PackageSpec::Base(spec) => &spec.patch_foreign,
    };

    for (dest, _) in patches {
        writeln!(out, "if [ `dpkg-divert --list \"{}\" | wc -l` -gt 0 ];", dest)?;
        writeln!(out, "then")?;
        writeln!(out, "\trm -f \"{}\"", dest)?;
        writeln!(out, "\tdpkg-divert --remove --rename \"{}\"", dest)?;
        writeln!(out, "fi")?;
    }

    let apparmor_needs_reload = patches.keys().any(|file| file.starts_with("/etc/apparmor.d/"));
    if apparmor_needs_reload {
        writeln!(out, "if aa-enabled &> /dev/null && systemctl is-active apparmor;")?;
        writeln!(out, "then")?;
        writeln!(out, "\tsystemctl reload apparmor")?;
        writeln!(out, "fi")?;
    }

    Ok(())
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let out = out.set_header("#!/bin/bash\n\nset -e\n\n");
    let mut out = out.finalize();

    write_alternatives(&mut out, instance)?;
    write_patches(&mut out, instance)?;

    Ok(())
}
