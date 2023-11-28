use std::fmt;
use debcrafter::im_repr::{PackageOps, InternalVarCondition};

struct ShellEscaper<W: fmt::Write>(W);

impl<W: fmt::Write> fmt::Write for ShellEscaper<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            if ch == '\'' {
                write!(self.0, "'\\''")?;
            } else {
                write!(self.0, "{}", ch)?;
            }
        }
        Ok(())
    }
}

pub(crate) struct DisplayEscaped<D: fmt::Display>(pub D);

impl<D: fmt::Display> fmt::Display for DisplayEscaped<D> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use fmt::Write;

        write!(f, "'")?;
        write!(ShellEscaper(&mut *f), "{}", self.0)?;
        write!(f, "'")
    }
}

pub struct SecureCommand<'a, I> {
    program: &'a str,
    args: I,
    user: &'a str,
    group: &'a str,
    allow_new_privs: bool,
    keep_env: bool,
    share_tmp: bool,
    rw_root: bool,
    hide_pid: HidePid,
}

impl<'a, I> SecureCommand<'a, I> where I: IntoIterator, I::Item: fmt::Display {
    pub fn new(program: &'a str, args: I, user: &'a str, group: &'a str) -> Self {
        SecureCommand {
            program,
            args,
            user,
            group,
            allow_new_privs: false,
            keep_env: false,
            share_tmp: false,
            rw_root: false,
            hide_pid: HidePid::Strict,
        }
    }

    pub fn allow_new_privileges(mut self, allow_new_privs: bool) -> Self {
        self.allow_new_privs = allow_new_privs;
        self
    }

    pub fn keep_env(mut self, keep_env: bool) -> Self {
        self.keep_env = keep_env;
        self
    }

    pub fn share_tmp(mut self, share_tmp: bool) -> Self {
        self.share_tmp = share_tmp;
        self
    }

    pub fn rw_root(mut self, rw_root: bool) -> Self {
        self.rw_root = rw_root;
        self
    }

    pub fn hide_pid(mut self, hide_pid: HidePid) -> Self {
        self.hide_pid = hide_pid;
        self
    }

    pub fn generate_script<W: fmt::Write>(self, mut out: W) -> fmt::Result {
        use fmt::Write;

        let skip_unshare = self.user == "root" || (self.share_tmp && self.rw_root && self.hide_pid == HidePid::No);

        let mut escaper;
        let cmd_writer = if !skip_unshare {
            write!(out, "unshare -m bash -c '")?;
            escaper = ShellEscaper(&mut out);
            if !self.share_tmp {
                write!(escaper, "mount --make-private -t tmpfs tmpfs /tmp && ")?;
            }
            match &self.hide_pid {
                HidePid::No => (),
                HidePid::Access | HidePid::Strict => write!(escaper, "mount --make-private -o remount,rw,hidepid={} -t proc proc /proc && ", self.hide_pid.mount_param())?,
            }
            if !self.rw_root {
                write!(escaper, "mount --make-private -o remount,bind,ro / && ")?;
            }
            write!(escaper, "exec ")?;
            &mut escaper as &mut dyn fmt::Write
        } else {
            &mut out as &mut dyn fmt::Write
        };

        if self.user != "root" {
            let no_new_privs = if self.allow_new_privs {
                ""
            } else {
                "--no-new-privs "
            };
            let reset_env = if self.keep_env {
                ""
            } else {
                "--reset-env "
            };
            // Solution for setpriv: libcap-ng is too old for "all" caps
            // https://github.com/SinusBot/docker/pull/40
            write!(cmd_writer, "setpriv --reuid={} --regid={} --init-groups --inh-caps=-cap_$(seq -s ,-cap_ 0 $(cat /proc/sys/kernel/cap_last_cap)) {}{}-- ", self.user, self.group, no_new_privs, reset_env)?;
        }
        write!(cmd_writer, "{}", DisplayEscaped(self.program))?;
        for arg in self.args {
            write!(cmd_writer, " {}", DisplayEscaped(arg))?;
        }

        if !skip_unshare {
            write!(out, "'")?;
        }
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Clone, Eq, PartialEq)]
pub enum HidePid {
    No,
    Access,
    Strict,
}

impl HidePid {
    fn mount_param(&self) -> u8 {
        match self {
            HidePid::No => 0,
            HidePid::Access => 1,
            HidePid::Strict => 2,
        }
    }
}

pub(crate) fn write_ivar_conditions<'a, W: fmt::Write, P: PackageOps<'a>>(mut out: W, instance: &P, conditions: &[InternalVarCondition]) -> fmt::Result {
    let mut first = true;
    write!(out, "if ")?;
    for cond in conditions {
        if !first {
            write!(&mut out, " && ")?;
        } else {
            first = false;
        }
        match cond {
            InternalVarCondition::Var { name, value, } => {
                write!(&mut out, "[ \"${{CONFIG[{}]}}\" = {} ]", name.expand(instance.config_pkg_name(), instance.variant()), DisplayEscaped(value.expand(instance.constants_by_variant())))?;
            },
            InternalVarCondition::Command { run, user, group, invert, } => {
                let user = user.expand_to_cow(instance.constants_by_variant());
                let group = group.expand_to_cow(instance.constants_by_variant());

                if *invert {
                    write!(&mut out, "! ")?;
                }

		let (program, args) = run.split_first();
		let args = args.iter().map(|arg| arg.expand(instance.constants_by_variant()));
		crate::codegen::bash::SecureCommand::new(&program.expand_to_cow(instance.constants_by_variant()), args, &user, &group)
		    .generate_script(&mut out)?;
            }
        }
    }
    writeln!(out, ";\nthen")?;
    Ok(())
}
