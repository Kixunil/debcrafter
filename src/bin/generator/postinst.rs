use std::io::{self, Write};
use debcrafter::{PackageInstance, ServiceInstance, ConfFormat, VarType, FileType};
use crate::codegen::{LazyCreate, LazyCreateBuilder, WriteHeader};
use std::fmt;
use debcrafter::postinst::{HandlePostinst, Config};

struct SduHandler<H: WriteHeader> {
    out: LazyCreate<H>,
}

fn new_sdu_handler(out: LazyCreateBuilder) -> SduHandler<impl WriteHeader> {
    let out = out
        .set_header("#!/bin/bash\nset -e\n\n. /usr/share/debconf/confmodule\n\ndeclare -A CONFIG\n\n")
        .finalize();

    SduHandler {
        out: out,
    }
}

impl<H: WriteHeader> HandlePostinst for SduHandler<H> {
    type Error = io::Error;

    fn prepare_user<T: fmt::Display>(&mut self, name: &str, group: bool, home: Option<T>) -> Result<(), Self::Error> {
        let group_arg = if group {
            " --group"
        } else {
            ""
        };

        if let Some(home) = home {
            writeln!(self.out, "adduser --system --quiet{} --home \"{}\" {}\n", group_arg, home, name)
        } else {
            writeln!(self.out, "adduser --system --quiet{} {} \n", group_arg, name)
        }
    }

    fn prepare_config(&mut self, config: &Config) -> Result<(), Self::Error> {
        if config.extension {
            writeln!(self.out, "dpkg-trigger --await {}", config.file_name)?;
            if let Some(pos) = config.file_name.rfind('/') {
                writeln!(self.out, "dpkg-trigger --await {}", &config.file_name[..pos])?;
            }
        }

        writeln!(self.out, "mkdir -p \"`dirname \"{}\"`\"", config.file_name)?;
        writeln!(self.out, "echo '# Automtically generated - DO NOT MODIFY!' > \"{}\"", config.file_name)?;
        if let Some(group) = config.change_group {
            writeln!(self.out, "chgrp \"{}\" \"{}\"", group, config.file_name)?;
        }

        if config.public {
            writeln!(self.out, "chmod 644 {}", config.file_name)
        } else {
            writeln!(self.out, "chmod 640 {}", config.file_name)
        }
    }

    fn write_internal_var(&mut self, config: &Config, name: &str, ty: &VarType) -> Result<(), Self::Error> {
        write_fetch_var(&mut self.out, config.package_name, name)?;
        write_var(&mut self.out, config, name, ty)?;
        writeln!(self.out)
    }

    fn write_external_var(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, rename: &Option<String>) -> Result<(), Self::Error> {
        write_fetch_var(&mut self.out, package, name)?;
        write_var(&mut self.out, config, rename.as_ref().map(AsRef::as_ref).unwrap_or(name), ty)?;
        writeln!(self.out)
    }

    fn fetch_external_var(&mut self, _config: &Config, package: &str, name: &str) -> Result<(), Self::Error> {
        write_fetch_var(&mut self.out, package, name)?;
        writeln!(self.out)
    }

    fn write_hidden_const(&mut self, config: &Config, name: &str, ty: &VarType, val: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "RET=\"{}\"", val)?;
        write_var(&mut self.out, config, name, ty)?;
        writeln!(self.out)
    }

    fn write_hidden_script(&mut self, config: &Config, name: &str, ty: &VarType, script: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "RET=\"$({})\"", script)?;
        write_var(&mut self.out, config, name, ty)?;
        writeln!(self.out)
    }

    fn include_conf_dir<T: fmt::Display>(&mut self, config: &Config, dir: T) -> Result<(), Self::Error> {
        writeln!(self.out, "if [ -d \"{}\" ] && [ `ls \"{}\" | wc -l` -gt 0 ];", dir, dir)?;
        writeln!(self.out, "then")?;
        writeln!(self.out, "\tcat \"{}\"/* >> \"{}\"", dir, config.file_name)?;
        writeln!(self.out, "fi\n")
    }

    fn include_conf_file<T: fmt::Display>(&mut self, config: &Config, file: T) -> Result<(), Self::Error> {
        writeln!(self.out, "cat \"{}\" >> \"{}\"", file, config.file_name)
    }

    fn create_path(&mut self, config: &Config, var_name: &str, file_type: &FileType, mode: u16, owner: &str, group: &str, only_parent: bool) -> Result<(), Self::Error> {
        match (file_type, only_parent) {
            (_, true) => {
                writeln!(self.out, "create_path=\"`dirname \"${{CONFIG[{}/{}]}}\"`\"", config.package_name, var_name)?;
                writeln!(self.out, "mkdir -p \"$create_path\"")?;
            },
            (FileType::Regular, false) => {
                writeln!(self.out, "create_path=\"${{CONFIG[{}/{}]}}\"", config.package_name, var_name)?;
                writeln!(self.out, "mkdir -p \"`dirname \"$create_path\"`\"")?;
                writeln!(self.out, "touch \"$create_path\"")?;
            },
            (FileType::Dir, false) => {
                writeln!(self.out, "create_path=\"${{CONFIG[{}/{}]}}\"", config.package_name, var_name)?;
                writeln!(self.out, "mkdir -p \"$create_path\"")?;
            },
        }
        writeln!(self.out, "chown {} \"$create_path\"", owner)?;
        writeln!(self.out, "chgrp {} \"$create_path\"", group)?;
        writeln!(self.out, "chmod {} \"$create_path\"", mode)?;
        writeln!(self.out)
    }

    fn write_comment(&mut self, config: &Config, comment: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "cat << EOF >> \"{}\"", config.file_name)?;
        for line in comment.split('\n') {
            writeln!(self.out, "# {}", line)?;
        }
        writeln!(self.out, "EOF\n")
    }

    fn restart_service_if_needed(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error> {
        writeln!(self.out, "if [ \"$1\" = triggered ];")?;
        writeln!(self.out, "then")?;
        writeln!(self.out, "\tdeb-systemd-invoke restart {}", instance.service_name())?;
        writeln!(self.out, "fi\n")
    }

    fn trigger_config_changed(&mut self, instance: &PackageInstance) -> Result<(), Self::Error> {
        writeln!(self.out, "dpkg-trigger {}-config-changed\n", instance.name)
    }

    fn finish(mut self) -> Result<(), Self::Error> {
        writeln!(self.out, "#DEBHELPER#\n")?;
        writeln!(self.out, "exit 0")
    }
}

fn write_fetch_var<W: Write>(mut out: W, package_name: &str, var_name: &str) -> io::Result<()> {
    writeln!(&mut out, "db_get {}/{}", package_name, var_name)?;
    writeln!(&mut out, "CONFIG[\"{}/{}\"]=\"$RET\"", package_name, var_name)
}

fn write_var_plain<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "cat << EOF >> \"{}\"", config.file_name)?;
    writeln!(&mut out, "{}=$RET", name)?;
    writeln!(&mut out, "EOF")
}

fn write_stringly_toml<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "echo -n \"{}=\\\"\" >> \"{}\"", name, config.file_name)?;
    writeln!(&mut out, "if [ $(cat << EOF | wc -c")?;
    writeln!(&mut out, "$RET")?;
    writeln!(&mut out, "EOF")?;
    writeln!(&mut out, ") -gt 1 ]; then")?;
    writeln!(&mut out, "cat << EOF | perl -pe 'chomp if eof' | sed -e 's/\\\\/\\\\\\\\/' -e 's/\"/\\\\\"/' | awk 1 ORS='\\n' | sed 's/$/\"/' >> \"{}\"", config.file_name)?;
    writeln!(&mut out, "$RET")?;
    writeln!(&mut out, "EOF")?;
    writeln!(&mut out, "else")?;
    writeln!(&mut out, "echo '\"' >> \"{}\"", config.file_name)?;
    writeln!(&mut out, "fi")
}

fn write_unquoted_toml<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "cat << EOF >> \"{}\"", config.file_name)?;
    writeln!(&mut out, "{} = $RET", name)?;
    writeln!(&mut out, "EOF")
}

fn write_var<W: Write>(mut out: W, config: &Config, name: &str, ty: &VarType) -> io::Result<()> {
    match (config.format, ty) {
        (ConfFormat::Toml, VarType::String) |
        (ConfFormat::Toml, VarType::BindHost) |
        (ConfFormat::Toml, VarType::Path { .. }) => write_stringly_toml(&mut out, config, name),
        (ConfFormat::Toml, _) => write_unquoted_toml(&mut out, config, name),
        (ConfFormat::Plain, _) => write_var_plain(&mut out, config, name),
    }
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let handler = new_sdu_handler(out);
    debcrafter::postinst::handle_instance(handler, instance)
}

