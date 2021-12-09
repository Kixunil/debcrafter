use std::io::{self, Write};
use debcrafter::im_repr::{PackageOps, PackageInstance, PackageConfig, ConfType, DebconfPriority};
use crate::codegen::{LazyCreateBuilder};
use std::borrow::Cow;

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let header = "#!/bin/bash

. /usr/share/debconf/confmodule

";
    let mut out = out.set_header(header).finalize();

    if instance.migrations.values().any(|migration| migration.config.is_some()) {
        writeln!(out, "if [ \"$1\" = \"configure\" ] && dpkg --validate-version \"$2\" &>/dev/null;")?;
        writeln!(out, "then")?;
        for (version, migration) in instance.migrations {
            if let Some(config) = &migration.config {
                writeln!(out, "\tif dpkg --compare-versions \"$2\" lt '{}';", version.version())?;
                writeln!(out, "\tthen")?;
                let config = config.expand_to_cow(instance.constants_by_variant());
                for line in config.trim().split('\n') {
                    if line.is_empty() {
                        writeln!(out)?;
                    } else {
                        writeln!(out, "\t\t{}", line)?;
                    }
                }
                writeln!(out, "\tfi")?;
                writeln!(out)?;
            }
        }
        writeln!(out, "fi")?;
    }

    //TODO: data validation
    for (_, config) in instance.config() {
        match &config.conf_type {
            ConfType::Static { .. } => (),
            ConfType::Dynamic { ivars, .. } =>  {
                for (var_name, var) in ivars {
                    if let Some(try_overwrite_default) = &var.try_overwrite_default {
                        writeln!(out, "db_fget {}/{} seen", instance.name, var_name)?;
                        writeln!(out, "if [ \"$RET\" '!=' 'true' ];")?;
                        writeln!(out, "then")?;
                        write!(out, "\tif default_value=\"$(")?;
                        fmt2io::write(&mut out, |writer|
                            crate::codegen::bash::SecureCommand::new("bash", &[Cow::Borrowed("-c"), try_overwrite_default.expand_to_cow(instance.constants_by_variant())], "nobody", "nogroup")
                                .generate_script(writer)
                        )?;
                        writeln!(out, ")\";")?;
                        writeln!(out, "\tthen")?;
                        writeln!(out, "\t\tdb_set {}/{} \"$default_value\"", instance.name, var_name)?;
                        writeln!(out, "\t\tdb_fset {}/{} seen false", instance.name, var_name)?;
                        writeln!(out, "\tfi")?;
                        writeln!(out, "fi")?;
                    }

                    let priority = match &var.priority {
                        DebconfPriority::Low => "PRIORITY=low",
                        DebconfPriority::Medium => "PRIORITY=medium",
                        DebconfPriority::High => "PRIORITY=high",
                        DebconfPriority::Critical => "PRIORITY=critical",
                        DebconfPriority::Dynamic { script } => &script,
                    };

                    writeln!(out, "{}\ndb_input $PRIORITY {}/{}", priority, instance.name, var_name)?;
                }
            },
        }
    }
    if let Some(file) = out.created() {
        writeln!(file, "db_go")?;
    }

    if let Some((db_type, _)) = instance.databases().iter().next() {
        writeln!(out, "if [ -f /usr/share/dbconfig-common/dpkg/config.{} ];", db_type.lib_name())?;
        writeln!(out, "then")?;
        writeln!(out, "\tdbc_dbtypes={}", db_type.dbconfig_db_type())?;
        writeln!(out, "\tdbc_prio_high=medium")?;
        writeln!(out, "\tdbc_prio_medium=low")?;
        writeln!(out, "\t. /usr/share/dbconfig-common/dpkg/config.{}", db_type.lib_name())?;
        writeln!(out, "\tdbc_go {} \"$@\"", instance.name)?;
        writeln!(out, "fi")?;
    }

    Ok(())
}
