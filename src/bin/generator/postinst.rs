use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, ServiceInstance, ConstantsByVariant, ConfFormat, VarType, FileType, FileVar, DirRepr, Migration, MigrationVersion, PackageOps};
use debcrafter::Map;
use debcrafter::types::VPackageName;
use crate::codegen::{LazyCreate, LazyCreateBuilder, WriteHeader};
use std::fmt;
use debcrafter::postinst::{HandlePostinst, Config, CreateDbRequest, CommandEnv};
use std::convert::TryFrom;
use crate::codegen::bash::write_ivar_conditions;
use debcrafter::input::InternalVarCondition;

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


struct SduHandler<H: WriteHeader> {
    out: LazyCreate<H>,
    var_written: bool,
    var_depth: usize,
}

fn new_sdu_handler(out: LazyCreateBuilder) -> SduHandler<impl WriteHeader> {
    let out = out
        .set_header("#!/bin/bash\nset -e\n\n. /usr/share/debconf/confmodule\n\ndeclare -A CONFIG\n\n")
        .finalize();

    SduHandler {
        out: out,
        var_written: false,
        var_depth: 0,
    }
}

impl<H: WriteHeader> SduHandler<H> {
    fn write_fetch_var(&mut self, package_name: &str, var_name: &str) -> io::Result<()> {
        writeln!(self.out, "db_get {}/{}", package_name, var_name)?;
        writeln!(self.out, "CONFIG[\"{}/{}\"]=\"$RET\"", package_name, var_name)
    }

    fn write_var_plain(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "cat << EOF >> \"{}\"", config.file_name)?;
        writeln!(self.out, "{}=$RET", name)?;
        writeln!(self.out, "EOF")
    }

    fn write_var_space_separated(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "cat << EOF >> \"{}\"", config.file_name)?;
        writeln!(self.out, "{} $RET", name)?;
        writeln!(self.out, "EOF")
    }

    fn write_stringly_toml(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "echo -n \"{}=\\\"\" >> \"{}\"", name, config.file_name)?;
        writeln!(self.out, "if [ $(cat << EOF | wc -c")?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, ") -gt 1 ]; then")?;
        writeln!(self.out, "cat << EOF | perl -pe 'chomp if eof' | sed -e 's/\\\\/\\\\\\\\/' -e 's/\"/\\\\\"/' | awk 1 ORS='\\n' | sed 's/$/\"/' >> \"{}\"", config.file_name)?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, "else")?;
        writeln!(self.out, "echo '\"' >> \"{}\"", config.file_name)?;
        writeln!(self.out, "fi")
    }

    fn write_unquoted_toml(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "cat << EOF >> \"{}\"", config.file_name)?;
        writeln!(self.out, "{} = $RET", name)?;
        writeln!(self.out, "EOF")
    }

    fn write_stringly_yaml(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "echo -n \"{}{}: \\\"\" >> \"{}\"", Indent(4*self.var_depth), name, config.file_name)?;
        writeln!(self.out, "if [ $(cat << EOF | wc -c")?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, ") -gt 1 ]; then")?;
        writeln!(self.out, "cat << EOF | perl -pe 'chomp if eof' | sed -e 's/\\\\/\\\\\\\\/' -e 's/\"/\\\\\"/' | awk 1 ORS='\\n' | sed 's/$/\"/' >> \"{}\"", config.file_name)?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, "else")?;
        writeln!(self.out, "echo '\"' >> \"{}\"", config.file_name)?;
        writeln!(self.out, "fi")
    }

    fn write_unquoted_yaml(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "cat << EOF >> \"{}\"", config.file_name)?;
        writeln!(self.out, "{}{}: $RET", Indent(4*self.var_depth), name)?;
        writeln!(self.out, "EOF")
    }

    fn write_stringly_json(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "echo -n \"\\\"{}\\\": \\\"\" >> \"{}\"", name, config.file_name)?;
        writeln!(self.out, "if [ $(cat << EOF | wc -c")?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, ") -gt 1 ]; then")?;
        writeln!(self.out, "cat << EOF | perl -pe 'chomp if eof' | sed -e 's/\\\\/\\\\\\\\/' -e 's/\"/\\\\\"/' | awk 1 ORS='\\n' | sed 's/$/\"/' >> \"{}\"", config.file_name)?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, "else")?;
        writeln!(self.out, "echo '\"' >> \"{}\"", config.file_name)?;
        writeln!(self.out, "fi")
    }

    fn write_unquoted_json(&mut self, config: &Config, name: &str) -> io::Result<()> {
        writeln!(self.out, "cat << EOF >> \"{}\"", config.file_name)?;
        writeln!(self.out, "\"{}\": $RET", name)?;
        writeln!(self.out, "EOF")
    }

    fn write_maybe_empty_var(&mut self, config: &Config, name: &str, ty: &VarType) -> io::Result<()> {
        match (config.format, ty) {
            (ConfFormat::Toml, VarType::String) |
            (ConfFormat::Toml, VarType::BindHost) |
            (ConfFormat::Toml, VarType::Path { .. }) => self.write_stringly_toml(config, name),
            (ConfFormat::Toml, _) => self.write_unquoted_toml(config, name),
            (ConfFormat::Yaml, VarType::String) |
            (ConfFormat::Yaml, VarType::BindHost) |
            (ConfFormat::Yaml, VarType::Path { .. }) => self.write_stringly_yaml(config, name),
            (ConfFormat::Yaml, _) => self.write_unquoted_yaml(config, name),
            (ConfFormat::Json, VarType::String) |
            (ConfFormat::Json, VarType::BindHost) |
            (ConfFormat::Json, VarType::Path { .. }) => self.write_stringly_json(config, name),
            (ConfFormat::Json, _) => self.write_unquoted_json(config, name),
            (ConfFormat::Plain, _) => self.write_var_plain(config, name),
            (ConfFormat::SpaceSeparated, _) => self.write_var_space_separated(config, name),
        }
    }

    fn write_nonempty_var(&mut self, config: &Config, name: &str, ty: &VarType) -> io::Result<()> {
        writeln!(self.out, "opts=$-")?;
        writeln!(self.out, "set +e")?;
        writeln!(self.out, "grep -q '^..*$' << EOF")?;
        writeln!(self.out, "$RET")?;
        writeln!(self.out, "EOF")?;
        writeln!(self.out, "if [ $? -eq 0 ]; then")?;
        writeln!(self.out, "if [[ $opts =~ e ]]; then set -e; fi")?;
        match (config.format, ty) {
            (ConfFormat::Toml, VarType::String) |
            (ConfFormat::Toml, VarType::BindHost) |
            (ConfFormat::Toml, VarType::Path { .. }) => self.write_stringly_toml(config, name),
            (ConfFormat::Toml, _) => self.write_unquoted_toml(config, name),
            (ConfFormat::Yaml, VarType::String) |
            (ConfFormat::Yaml, VarType::BindHost) |
            (ConfFormat::Yaml, VarType::Path { .. }) => self.write_stringly_yaml(config, name),
            (ConfFormat::Yaml, _) => self.write_unquoted_yaml(config, name),
            /*
            (ConfFormat::Json, VarType::String) |
            (ConfFormat::Json, VarType::BindHost) |
            (ConfFormat::Json, VarType::Path { .. }) => self.write_stringly_json(config, name),
            (ConfFormat::Json, _) => self.write_unquoted_json(config, name),
            */
            (ConfFormat::Json, _) => unimplemented!("Unimplemented because of commas"),
            (ConfFormat::Plain, _) => self.write_var_plain(config, name),
            (ConfFormat::SpaceSeparated, _) => self.write_var_space_separated(config, name),
        }?;
        writeln!(self.out, "else")?;
        writeln!(self.out, "if [[ $opts =~ e ]]; then set -e; fi")?;
        writeln!(self.out, "fi")
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

    fn prepare_database(&mut self, request: CreateDbRequest) -> Result<(), Self::Error> {
        writeln!(self.out, "mkdir -p `dirname {}`", request.config_path)?;
        writeln!(self.out, "dbc_generate_include=template:{}", request.config_path)?;
        // There doesn't seem to be a standardized path for templates, so we made up one
        writeln!(self.out, "dbc_generate_include_args=\"-o template_infile=/usr/share/{}/dbconfig-common/template\"", request.pkg_name)?;
        writeln!(self.out, "dbc_generate_include_owner={}:{}", request.config_owner, request.config_group)?;
        writeln!(self.out, "dbc_generate_include_perms={}", request.config_mode)?;
        writeln!(self.out, ". /usr/share/dbconfig-common/dpkg/postinst.{}", request.db_type.lib_name())?;
        writeln!(self.out, "dbc_go {} \"$@\"", request.pkg_name)?;
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

        if let Some(group) = &config.change_group {
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

    fn fetch_var(&mut self, _config: &Config, package: &str, name: &str) -> Result<(), Self::Error> {
        self.write_fetch_var(package, name)
    }

    fn generate_const_var(&mut self, _config: &Config, package: &str, name: &str, _ty: &VarType, val: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "RET=\"{}\"", val)?;
        writeln!(self.out, "CONFIG[\"{}/{}\"]=\"$RET\"", package, name)
    }

    fn generate_var_using_script(&mut self, _config: &Config, package: &str, name: &str, _ty: &VarType, script: &str) -> Result<(), Self::Error> {
        writeln!(self.out, "RET=\"$({})\"", script)?;
        writeln!(self.out, "CONFIG[\"{}/{}\"]=\"$RET\"", package, name)
    }

    fn generate_var_using_template(&mut self, _config: &Config, package: &str, name: &str, _ty: &VarType, template: &str, constants: ConstantsByVariant<'_>) -> Result<(), Self::Error> {
        use debcrafter::template::{Component, Query};

        write!(self.out, "RET=\"")?;
        for component in debcrafter::template::parse(template) {
            match component {
                Component::Constant(val) => write!(self.out, "{}", val)?,
                Component::Variable(var) if var.starts_with('/') => write!(self.out, "${{CONFIG[{}{}]}}", package, var)?,
                Component::Variable(var) if var.contains('/') => {
                    let pos = var.find('/').expect("unreachable");
                    let pkg_name = VPackageName::try_from(var[..pos].to_owned()).expect("invalid package name");
                    let var_name = &var[(pos + 1)..];
                    write!(self.out, "${{CONFIG[{}/{}]}}", pkg_name.expand_to_cow(constants.get_variant()), var_name)?;
                },
                Component::Variable(var) => write!(self.out, "{}", constants.get(var).unwrap_or_else(|| panic!("constant {} not found for variant", var)))?,
            }
        }
        writeln!(self.out, "\"")?;
        writeln!(self.out, "CONFIG[\"{}/{}\"]=\"$RET\"", package, name)
    }

    fn sub_object_begin(&mut self, config: &Config, name: &str) -> Result<(), Self::Error> {
        match config.format {
            ConfFormat::Plain => panic!("Plain format doesn't support structured configuration"),
            ConfFormat::Toml => unimplemented!("Structured configuration not implemented for toml"),
            ConfFormat::Yaml => {
                writeln!(self.out, "echo \"{}{}:\" >> \"{}\"", Indent(4*self.var_depth), name, config.file_name)?;
                self.var_depth += 1;
            },
            ConfFormat::Json => {
                if self.var_written {
                    writeln!(self.out, "echo ',' >> \"{}\"", config.file_name)?;
                } else {
                    writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
                }
                writeln!(self.out, "echo -n '\"{}\": {{' >> \"{}\"", name, config.file_name)?;
                self.var_written = false;
            },
            ConfFormat::SpaceSeparated => panic!("Space separated format doesn't support structured configuration"),
        }
        Ok(())
    }

    fn sub_object_end(&mut self, config: &Config, _name: &str) -> Result<(), Self::Error> {
        match &config.format {
            ConfFormat::Plain => panic!("Plain format doesn't support structured configuration"),
            ConfFormat::Toml => unimplemented!("Structured configuration not implemented for toml"),
            ConfFormat::Yaml => self.var_depth -= 1,
            ConfFormat::Json => {
                if self.var_written {
                    writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
                }
                writeln!(self.out, "echo -n '}}' >> \"{}\"", config.file_name)?;
                self.var_written = true;
            },
            ConfFormat::SpaceSeparated => panic!("Space separated format doesn't support structured configuration"),
        }
        Ok(())
    }

    fn condition_begin<'a>(&mut self, instance: &impl PackageOps<'a>, conditions: &[InternalVarCondition]) -> Result<(), Self::Error> {
        fmt2io::write(&mut self.out, |writer| write_ivar_conditions(writer, instance, conditions))
    }

    fn condition_end(&mut self) -> Result<(), Self::Error> {
        writeln!(self.out, "fi")
    }

    fn write_var<'a, I>(&mut self, config: &Config, package: &str, name: &str, ty: &VarType, structure: I, ignore_empty: bool) -> Result<(), Self::Error> where I: IntoIterator<Item=&'a str> {
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
            self.write_nonempty_var(config, out_var, ty)?;
        } else {
            self.write_maybe_empty_var(config, out_var, ty)?;
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
            (ConfFormat::Yaml, FileVar::Dir {repr: DirRepr::Array, path, .. }) => {
                let mut out_var = structure.next().expect("Empty structure");
                for var in structure {
                    out_var = var;
                }
                let out_file = config.file_name;
                let in_dir = format!("/etc/{}/{}/", subdir, path);
                writeln!(self.out, "echo >> \"{}\"", config.file_name)?;
                writeln!(self.out, "echo \"{}{}:\" >> \"{}\"", Indent(4*self.var_depth), out_var, out_file)?;
                writeln!(self.out, "if [ -d \"{}\" ] && [ `ls \"{}\" | wc -l` -gt 0 ];", in_dir, in_dir)?;
                writeln!(self.out, "then")?;
                writeln!(self.out, "\tfor file in \"{}\"/*", in_dir)?;
                writeln!(self.out, "\tdo")?;
                writeln!(self.out, "\t\tsed -e '1,1s/^/{}    - /' -e '2,$s/^/{}      /' \"$file\" >> \"{}\"", Indent(4*self.var_depth), Indent(4*self.var_depth), out_file)?;
                writeln!(self.out, "\tdone")?;
                writeln!(self.out, "fi")?;
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
        writeln!(self.out, "mkdir -m 750 -p \"{}\"", path)
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
    fn register_alternatives<A, B, I>(&mut self, alternatives: I) -> Result<(), Self::Error> where I: IntoIterator<Item=(A, B)>, A: AsRef<str>, B: std::borrow::Borrow<debcrafter::im_repr::Alternative> {
        let mut written = false;
        for (provider, alternative) in alternatives {
            if !written {
                // This intentionally does **not** skip if there's a version string present as
                // running it twice is harmless but not running it is harmful.
                writeln!(self.out, "if [ \"$1\" = configure ];")?;
                writeln!(self.out, "then")?;
                written = true;
            }

            let alternative = alternative.borrow();

            writeln!(self.out, "update-alternatives --install \"{}\" \"{}\" \"{}\" {}", alternative.dest, alternative.name, provider.as_ref(), alternative.priority)?;
        }

        if written {
            writeln!(self.out, "fi")?;
        }

        Ok(())
    }

    fn patch_files<A, B, I>(&mut self, pkg_name: &str, patches: I) -> Result<(), Self::Error> where I: IntoIterator<Item=(A, B)>, A: AsRef<str>, B: AsRef<str> {
        for (dest, patch) in patches {
            let dest = dest.as_ref();
            let patch = patch.as_ref();

            writeln!(self.out, "was_diverted=\"`dpkg-divert --list \"{}\" | wc -l`\"", dest)?;
            writeln!(self.out, "if [ \"$was_diverted\" -eq 0 ];")?;
            writeln!(self.out, "then")?;
            writeln!(self.out, "\tdpkg-divert --add --rename --package \"{}\" \"{}\"", pkg_name, dest)?;
            writeln!(self.out, "fi")?;
            writeln!(self.out, "orig_file=\"`dpkg-divert --truename \"{}\"`\"", dest)?;
            writeln!(self.out, "test -r \"$orig_file\"")?;
            writeln!(self.out, "patch -o \"{}\" \"$orig_file\" \"{}\"", dest, patch)?;
            writeln!(self.out, "chown --reference=\"$orig_file\" \"{}\"", dest)?;
            writeln!(self.out, "chmod --reference=\"$orig_file\" \"{}\"", dest)?;
        }

        Ok(())
    }

    fn reload_apparmor(&mut self) -> Result<(), Self::Error> {
        writeln!(self.out, "if aa-enabled &> /dev/null && systemctl is-active apparmor;")?;
        writeln!(self.out, "then")?;
        writeln!(self.out, "\tsystemctl reload apparmor")?;
        writeln!(self.out, "fi")?;

        Ok(())
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

    fn run_command<I>(&mut self, command: I, env: &CommandEnv<'_>) -> Result<(), Self::Error> where I: IntoIterator, I::Item: fmt::Display {
        let mut iter = command.into_iter();
        write!(self.out, "MAINTSCRIPT_ACTION=\"$1\" MAINTSCRIPT_VERSION=\"$2\" ")?;
        let (user, group, allow_new_privs) = if let Some(restrictions) = &env.restrict_privileges {
            (restrictions.user, restrictions.group, restrictions.allow_new_privileges)
        } else {
            ("root", "root", true)
        };
        let program = iter.next().expect("Can't run command: missing program name").to_string();
        fmt2io::write(&mut self.out, |writer|
            crate::codegen::bash::SecureCommand::new(&program, iter, user, group)
                .allow_new_privileges(allow_new_privs)
                .keep_env(true)
                .generate_script(writer)
        )?;
        writeln!(self.out)
    }

    fn finalize_migrations(&mut self, migrations: &Map<MigrationVersion, Migration>, constatnts: ConstantsByVariant<'_>) -> Result<(), Self::Error> {
        writeln!(self.out, "if [ \"$1\" = \"configure\" ] && dpkg --validate-version \"$2\" &>/dev/null;")?;
        writeln!(self.out, "then")?;
        for (version, migration) in migrations {
            if let Some(migration) = &migration.postinst_finish {
                writeln!(self.out, "\tif dpkg --compare-versions \"$2\" lt '{}';", version.version())?;
                writeln!(self.out, "\tthen")?;
                let migration = migration.expand_to_cow(&constatnts);
                for line in migration.trim().split('\n') {
                    if line.is_empty() {
                        writeln!(self.out)?;
                    } else {
                        writeln!(self.out, "\t\t{}", line)?;
                    }
                }
                writeln!(self.out, "\tfi")?;
                writeln!(self.out)?;
            }
        }
        writeln!(self.out, "fi")?;
        Ok(())
    }

    fn finish(mut self) -> Result<(), Self::Error> {
        writeln!(self.out, "#DEBHELPER#\n")?;
        writeln!(self.out, "exit 0")
    }
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let handler = new_sdu_handler(out);
    debcrafter::postinst::handle_instance(handler, instance)
}

struct Indent(usize);

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for _ in 0..self.0 {
            f.write_str(" ")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    macro_rules! test_case {
        ($name:ident, $input:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(super::DisplayEscaped($input).to_string(), $expected);
            }
        }
    }

    test_case!(escape_empty, "", "''");
    test_case!(escape_single_char, "x", "'x'");
    test_case!(escape_few_chars, "xydfd", "'xydfd'");
    test_case!(escape_single_quote, "'", "''\\'''");
    test_case!(escape_to_quotes, "''", "''\\'''\\'''");
    test_case!(escape_letter_quote_letter, "a'b", "'a'\\''b'");
}
