use std::io;
use std::borrow::Cow;
use debcrafter::im_repr::{PackageInstance, PackageOps};
use debcrafter::postinst::{CommandEnv, CommandPrivileges};
use crate::codegen::{LazyCreateBuilder};
use crate::generator::postinst::DisplayEscaped;

fn write_alternatives<W: io::Write>(mut out: W, instance: &PackageInstance) -> io::Result<()> {
    let mut written = false;

    for (provider, alternative) in instance.alternatives {
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
    for (dest, _) in instance.patch_foreign {
        writeln!(out, "if [ `dpkg-divert --list \"{}\" | wc -l` -gt 0 ];", dest)?;
        writeln!(out, "then")?;
        writeln!(out, "\trm -f \"{}\"", dest)?;
        writeln!(out, "\tdpkg-divert --remove --rename \"{}\"", dest)?;
        writeln!(out, "fi")?;
    }

    let apparmor_needs_reload = instance.patch_foreign
        .keys()
        .any(|file| file.starts_with("/etc/apparmor.d/"));
    if apparmor_needs_reload {
        writeln!(out, "if aa-enabled &> /dev/null && systemctl is-active apparmor;")?;
        writeln!(out, "then")?;
        writeln!(out, "\tsystemctl reload apparmor")?;
        writeln!(out, "fi")?;
    }

    Ok(())
}

fn write_plug<W: io::Write>(mut out: W, instance: &PackageInstance) -> io::Result<()> {
    for plug in instance.plug.iter().rev() {
        let user = plug.run_as_user.expand_to_cow(instance.constants_by_variant());
        let group = plug.run_as_group.as_ref().map(|group| group.expand_to_cow(instance.constants_by_variant())).unwrap_or(Cow::Borrowed(&*user));
        let privileges = CommandPrivileges {
            user: &user,
            group: &group,
            allow_new_privileges: false,
        };

        let env = CommandEnv {
            restrict_privileges: Some(privileges),
        };

        let mut iter = plug.unregister_cmd.iter().map(|arg| arg.expand(instance.constants_by_variant()));

        write!(out, "MAINTSCRIPT_ACTION=\"$1\" MAINTSCRIPT_VERSION=\"$2\" ")?;
        if let Some(restrictions) = &env.restrict_privileges {
            write!(out, "setpriv --reuid={} --regid={} --init-groups --inh-caps=-all", restrictions.user, restrictions.group)?;
            if !restrictions.allow_new_privileges {
                write!(out, " --no-new-privs")?;
            }
            write!(out, " -- ")?;
        }
        // sanity check
        write!(out, "{}", iter.next().expect("Can't run command: missing program name"))?;
        for arg in iter {
            write!(out, " {}", DisplayEscaped(arg))?;
        }
        writeln!(out)?;
    }
    Ok(())
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let out = out.set_header("#!/bin/bash\n\nset -e\n\n");
    let mut out = out.finalize();

    write_plug(&mut out, instance)?;
    write_alternatives(&mut out, instance)?;
    write_patches(&mut out, instance)?;

    Ok(())
}
