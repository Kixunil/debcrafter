use std::io;
use std::borrow::Cow;
use debcrafter::im_repr::{PackageInstance, PackageOps};
use debcrafter::postinst::{CommandEnv, CommandPrivileges};
use crate::codegen::{LazyCreateBuilder};

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
        let group;
        let restrict_privileges = if user != "root" {
            group = plug.run_as_group.as_ref().map(|group| group.expand_to_cow(instance.constants_by_variant())).unwrap_or(Cow::Borrowed(&*user));
            Some(CommandPrivileges {
                user: &user,
                group: &group,
                allow_new_privileges: false,
                read_only_root: plug.read_only_root,
            })
        } else {
            None
        };

        let env = CommandEnv {
            restrict_privileges,
        };

        let mut iter = plug.unregister_cmd.iter().map(|arg| arg.expand(instance.constants_by_variant()));

        write!(out, "MAINTSCRIPT_ACTION=\"$1\" MAINTSCRIPT_VERSION=\"$2\" ")?;
        let (user, group, allow_new_privs, read_only_root) = if let Some(restrictions) = &env.restrict_privileges {
            (restrictions.user, restrictions.group, restrictions.allow_new_privileges, restrictions.read_only_root)
        } else {
            ("root", "root", true, false)
        };
        let program = iter.next().expect("Can't run command: missing program name").to_string();
        fmt2io::write(&mut out, |writer|
            crate::codegen::bash::SecureCommand::new(&program, iter, user, group)
                .allow_new_privileges(allow_new_privs)
                .keep_env(true)
                .rw_root(!read_only_root)
                .generate_script(writer)
        )?;
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
