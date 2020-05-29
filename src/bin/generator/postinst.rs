use std::io::{self, Write};
use debcrafter::{PackageInstance, ServiceInstance, ConfFormat, VarType, FileType, DbConfig, FileVar, DirRepr};
use crate::codegen::{LazyCreate, LazyCreateBuilder, WriteHeader};
use std::fmt;
use debcrafter::postinst::{HandlePostinst, Config};

struct SduHandler<H: WriteHeader> {
    out: LazyCreate<H>,
    var_written: bool,
}

fn new_sdu_handler(out: LazyCreateBuilder) -> SduHandler<impl WriteHeader> {
    let out = out
        .set_header("#!/bin/bash\nset -e\n\n. /usr/share/debconf/confmodule\n\ndeclare -A CONFIG\n\n")
        .finalize();

    SduHandler {
        out: out,
        var_written: false,
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

    fn add_user_to_groups<I>(&mut self, user: &str, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str> {
        let mut non_empty = false;
        write!(self.out, "usermod -a -G ")?;
        for group in groups {
            if non_empty {
                write!(self.out, ",{}", group.as_ref())?;
            } else {
                write!(self.out, "{}", group.as_ref())?;
                non_empty = true;
            }
        }
        writeln!(self.out, " {}", user)?;
        Ok(())
    }

    fn create_groups<I>(&mut self, groups: I) -> Result<(), Self::Error> where I: IntoIterator, <I as IntoIterator>::Item: AsRef<str> {
        for group in groups {
            writeln!(self.out, "groupadd -rf {}", group.as_ref())?;
        }
        Ok(())
    }

    fn prepare_database(&mut self, pkg: &ServiceInstance, db_type: &str, _db_config: &DbConfig) -> Result<(), Self::Error> {
        // TODO: FS-based databases (sqlite)
        if let Some(conf_d) = &pkg.spec.conf_d {
            writeln!(self.out, "mkdir -p /etc/{}/{}", pkg.name, conf_d.name)?;
            writeln!(self.out, "dbc_generate_include=template:/etc/{}/{}/database", pkg.name, conf_d.name)?;
        } else {
            writeln!(self.out, "mkdir -p /etc/{}", pkg.name)?;
            writeln!(self.out, "dbc_generate_include=template:/etc/{}/database", pkg.name)?;
        }
        // There doesn't seem to be a standardized path for templates, so we made up one
        writeln!(self.out, "dbc_generate_include_args=\"-o template_infile=/usr/share/{}/dbconfig-common/template\"", pkg.name)?;
        // We prefer config files to be owned by root, but being readable is more important
        if pkg.spec.user.group {
            writeln!(self.out, "dbc_generate_include_owner=root:{}", pkg.user_name())?;
            writeln!(self.out, "dbc_generate_include_perms=640")?;
        } else {
            writeln!(self.out, "dbc_generate_include_owner={}:root", pkg.user_name())?;
            writeln!(self.out, "dbc_generate_include_perms=460")?;
        }
        writeln!(self.out, ". /usr/share/dbconfig-common/dpkg/postinst.{}", db_type)?;
        writeln!(self.out, "dbc_go {} \"$@\"", pkg.name)?;
        Ok(())
    }

    fn prepare_config(&mut self, config: &Config) -> Result<(), Self::Error> {
        self.var_written = false;

        writeln!(self.out, "mkdir -p \"`dirname \"{}\"`\"", config.file_name)?;
        writeln!(self.out, "echo -n > \"{}\"", config.file_name)?;

        if let Some(insert_header) = &config.insert_header {
                writeln!(self.out, "cat <<EOF >> \"{}\"", config.file_name)?;
                writeln!(self.out, "{}", insert_header)?;
                writeln!(self.out, "EOF")?;
        }

        match (config.format, config.with_header) {
            (ConfFormat::Yaml, true) => {
                writeln!(self.out, "echo '---' >> \"{}\"", config.file_name)?;
                writeln!(self.out, "echo '# Automtically generated - DO NOT MODIFY!' >> \"{}\"", config.file_name)?;
            },
            (ConfFormat::Json, true) => writeln!(self.out, "echo '{{' >> \"{}\"", config.file_name)?,
            // Useful for flat includes
            (ConfFormat::Json, false) => writeln!(self.out, "echo -n >> \"{}\"", config.file_name)?,
            (_, true) => panic!("Header supported for Yaml only"),
            _ => writeln!(self.out, "echo '# Automtically generated - DO NOT MODIFY!' >> \"{}\"", config.file_name)?,
        }

        if let Some(group) = config.change_group {
            writeln!(self.out, "chgrp \"{}\" \"{}\"", group, config.file_name)?;
        }

        if config.public {
            writeln!(self.out, "chmod 644 {}", config.file_name)
        } else {
            writeln!(self.out, "chmod 640 {}", config.file_name)
        }
    }

    fn finish_config(&mut self, config: &Config) -> Result<(), Self::Error> {
        if *config.format == ConfFormat::Json && config.with_header {
            if self.var_written {
                writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
            }
            writeln!(self.out, "echo '}}' >> \"{}\"", config.file_name)?;
        }
        Ok(())
    }

    fn fetch_var(&mut self, config: &Config, package: &str, name: &str) -> Result<(), Self::Error> {
        write_fetch_var(&mut self.out, package, name)
    }

    fn generate_const_var(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, val: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "RET=\"{}\"", val)?;
        writeln!(self.out, "CONFIG[\"{}/{}\"]=\"$RET\"", package, name)
    }

    fn generate_var_using_script(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, script: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "RET=\"$({})\"", script)?;
        writeln!(self.out, "CONFIG[\"{}/{}\"]=\"$RET\"", package, name)
    }

    fn sub_object_begin(&mut self, config: &Config, name: &str) -> Result<(), Self::Error> {
        match config.format {
            ConfFormat::Plain => panic!("Plain format doesn't support structured configuration"),
            ConfFormat::Toml => unimplemented!("Structured configuration not implemented for toml"),
            ConfFormat::Yaml => unimplemented!("Structured configuration not implemented for yaml"),
            ConfFormat::Json => {
                if self.var_written {
                    writeln!(self.out, "echo ',' >> \"{}\"", config.file_name)?;
                } else {
                    writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
                }
                writeln!(self.out, "echo -n '\"{}\": {{' >> \"{}\"", name, config.file_name)?;
                self.var_written = false;
            },
        }
        Ok(())
    }

    fn sub_object_end(&mut self, config: &Config, name: &str) -> Result<(), Self::Error> {
        match &config.format {
            ConfFormat::Plain => panic!("Plain format doesn't support structured configuration"),
            ConfFormat::Toml => unimplemented!("Structured configuration not implemented for toml"),
            ConfFormat::Yaml => unimplemented!("Structured configuration not implemented for yaml"),
            ConfFormat::Json => {
                if self.var_written {
                    writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
                }
                writeln!(self.out, "echo -n '}}' >> \"{}\"", config.file_name)?;
                self.var_written = true;
            },
        }
        Ok(())
    }

    fn write_var<'a, I>(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, mut structure: I, ignore_empty: bool) -> Result<(), Self::Error> where I: Iterator<Item=&'a str> {
        let mut out_var = name;
        for var in structure {
            out_var = var;
        }
        writeln!(self.out, "RET=\"${{CONFIG[{}/{}]}}\"", package, name)?;
        if self.var_written && *config.format == ConfFormat::Json {
            writeln!(self.out, "echo ',' >> \"{}\"", config.file_name)?;
        } else {
            writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
        }

        if ignore_empty {
            write_nonempty_var(&mut self.out, config, out_var, ty)?;
        } else {
            write_var(&mut self.out, config, out_var, ty)?;
        }
        self.var_written = true;
        writeln!(self.out)
    }

    fn include_fvar<'a, I>(&mut self, config: &Config, var: &FileVar, mut structure: I, subdir: &str) -> Result<(), Self::Error> where I: Iterator<Item=&'a str> {
        match (config.format, var) {
            (ConfFormat::Json, FileVar::Dir {repr: DirRepr::Array, path, .. }) => {
                let mut out_var = structure.next().expect("Empty structure");
                for var in structure {
                    out_var = var;
                }
                let out_file = config.file_name;
                let in_dir = format!("/etc/{}/{}/", subdir, path);

                if self.var_written && *config.format == ConfFormat::Json {
                    writeln!(self.out, "echo ',' >> \"{}\"", config.file_name)?;
                } else {
                    writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
                }
                writeln!(self.out, "echo \"\\\"{}\\\": [\" >> \"{}\"", out_var, out_file)?;
                writeln!(self.out, "if [ -d \"{}\" ] && [ `ls \"{}\" | wc -l` -gt 0 ];", in_dir, in_dir)?;
                writeln!(self.out, "then")?;
                writeln!(self.out, "\twritten=0")?;
                writeln!(self.out, "\tfor file in \"{}\"/*", in_dir)?;
                writeln!(self.out, "\tdo")?;
                writeln!(self.out, "\t\ttest $written -eq 1 && echo ',' >> \"{}\"", out_file)?;
                writeln!(self.out, "\t\tcat \"$file\" >> \"{}\"", out_file)?;
                writeln!(self.out, "\twritten=1")?;
                writeln!(self.out, "\tdone")?;
                writeln!(self.out, "fi")?;
                writeln!(self.out, "echo \"]\" >> \"{}\"", out_file)?;
            },
            (ConfFormat::Plain, _) => panic!("Plain config format doesn't support file variables"),
            (x, FileVar::Dir {repr: DirRepr::Array, .. }) => unimplemented!("File variables not implemented for {}", x),
        }
        Ok(())
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

    fn activate_trigger(&mut self, trigger: &str, no_await: bool) -> Result<(), Self::Error> {
        if no_await {
            writeln!(self.out, "dpkg-trigger --no-await \"{}\"", trigger)
        } else {
            writeln!(self.out, "dpkg-trigger --await \"{}\"", trigger)
        }
    }

    fn create_tree(&mut self, path: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "mkdir -p \"{}\"", path)?;
        writeln!(self.out, "chmod 750 \"{}\"", path)
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

    fn stop_service(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error> {
        writeln!(self.out, "systemctl is-active {} && service_was_running=1 || service_was_running = 0", instance.service_name())?;
        writeln!(self.out, "systemctl stop {}", instance.service_name())
    }

    fn restart_service_if_needed(&mut self, instance: &ServiceInstance) -> Result<(), Self::Error> {
        writeln!(self.out, "if [ \"$1\" = triggered -a \"$service_was_running\" '!=' 0 ];")?;
        writeln!(self.out, "then")?;
        writeln!(self.out, "\tdeb-systemd-invoke restart {}", instance.service_name())?;
        writeln!(self.out, "fi\n")
    }

    fn trigger_config_changed(&mut self, instance: &PackageInstance) -> Result<(), Self::Error> {
        writeln!(self.out, "if [ \"$1\" '!=' triggered ];")?;
        writeln!(self.out, "then")?;
        writeln!(self.out, "\tdpkg-trigger {}-config-changed\n", instance.name)?;
        writeln!(self.out, "fi")
    }

    fn postprocess_conf_file(&mut self, command: &[String]) -> Result<(), Self::Error> {
        for arg in command {
            write!(self.out, "'")?;
            for ch in arg.chars() {
                if ch == '\'' {
                    write!(self.out, "'\\''")?;
                } else {
                    write!(self.out, "{}", ch)?;
                }
            }
            write!(self.out, "' ")?;
        }
        writeln!(self.out)?;
        Ok(())
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

fn write_stringly_yaml<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "echo -n \"{}: \\\"\" >> \"{}\"", name, config.file_name)?;
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

fn write_unquoted_yaml<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "cat << EOF >> \"{}\"", config.file_name)?;
    writeln!(&mut out, "{}: $RET", name)?;
    writeln!(&mut out, "EOF")
}

fn write_stringly_json<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "echo -n \"\\\"{}\\\": \\\"\" >> \"{}\"", name, config.file_name)?;
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

fn write_unquoted_json<W: Write>(mut out: W, config: &Config, name: &str) -> io::Result<()> {
    writeln!(&mut out, "cat << EOF >> \"{}\"", config.file_name)?;
    writeln!(&mut out, "\"{}\": $RET", name)?;
    writeln!(&mut out, "EOF")
}

fn write_var<W: Write>(mut out: W, config: &Config, name: &str, ty: &VarType) -> io::Result<()> {
    match (config.format, ty) {
        (ConfFormat::Toml, VarType::String) |
        (ConfFormat::Toml, VarType::BindHost) |
        (ConfFormat::Toml, VarType::Path { .. }) => write_stringly_toml(&mut out, config, name),
        (ConfFormat::Toml, _) => write_unquoted_toml(&mut out, config, name),
        (ConfFormat::Yaml, VarType::String) |
        (ConfFormat::Yaml, VarType::BindHost) |
        (ConfFormat::Yaml, VarType::Path { .. }) => write_stringly_yaml(&mut out, config, name),
        (ConfFormat::Yaml, _) => write_unquoted_yaml(&mut out, config, name),
        (ConfFormat::Json, VarType::String) |
        (ConfFormat::Json, VarType::BindHost) |
        (ConfFormat::Json, VarType::Path { .. }) => write_stringly_json(&mut out, config, name),
        (ConfFormat::Json, _) => write_unquoted_json(&mut out, config, name),
        (ConfFormat::Plain, _) => write_var_plain(&mut out, config, name),
    }
}

fn write_nonempty_var<W: Write>(mut out: W, config: &Config, name: &str, ty: &VarType) -> io::Result<()> {
    writeln!(&mut out, "opts=$-")?;
    writeln!(&mut out, "set +e")?;
    writeln!(&mut out, "grep -q '^..*$' << EOF")?;
    writeln!(&mut out, "$RET")?;
    writeln!(&mut out, "EOF")?;
    writeln!(&mut out, "if [ $? -eq 0 ]; then")?;
    writeln!(&mut out, "if [[ $opts =~ e ]]; then set -e; fi")?;
    match (config.format, ty) {
        (ConfFormat::Toml, VarType::String) |
        (ConfFormat::Toml, VarType::BindHost) |
        (ConfFormat::Toml, VarType::Path { .. }) => write_stringly_toml(&mut out, config, name),
        (ConfFormat::Toml, _) => write_unquoted_toml(&mut out, config, name),
        (ConfFormat::Yaml, VarType::String) |
        (ConfFormat::Yaml, VarType::BindHost) |
        (ConfFormat::Yaml, VarType::Path { .. }) => write_stringly_yaml(&mut out, config, name),
        (ConfFormat::Yaml, _) => write_unquoted_yaml(&mut out, config, name),
        /*
        (ConfFormat::Json, VarType::String) |
        (ConfFormat::Json, VarType::BindHost) |
        (ConfFormat::Json, VarType::Path { .. }) => write_stringly_json(&mut out, config, name),
        (ConfFormat::Json, _) => write_unquoted_json(&mut out, config, name),
        */
        (ConfFormat::Json, _) => unimplemented!("Unimplemented because of commas"),
        (ConfFormat::Plain, _) => write_var_plain(&mut out, config, name),
    }?;
    writeln!(&mut out, "else")?;
    writeln!(&mut out, "if [[ $opts =~ e ]]; then set -e; fi")?;
    writeln!(&mut out, "fi")
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let handler = new_sdu_handler(out);
    debcrafter::postinst::handle_instance(handler, instance)
}

