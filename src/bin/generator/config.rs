use std::io::{self, Write};
use debcrafter::im_repr::{PackageOps, PackageInstance, PackageConfig, ConfType, DebconfPriority, InternalVarCondition};
use crate::codegen::{LazyCreateBuilder};
use std::borrow::Cow;
use crate::codegen::bash::write_ivar_conditions;
use debcrafter::types::DynVarName;

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let header = "#!/bin/bash

. /usr/share/debconf/confmodule

declare -A CONFIG
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
    for config in instance.config().values() {
        match &config.conf_type {
            ConfType::Static { .. } => (),
            ConfType::Dynamic { ivars, .. } => {
                for (var_name, var) in ivars {
                    if !var.conditions.is_empty() {
                        let mut has_our_var = false;
                        for cond in &var.conditions {

                            match cond {
                                InternalVarCondition::Var { name, value: _, } => {
                                    // TODO: we could actually allow this to show certain options
                                    // only for specific variants but the logic is quite different.
                                    let needs_db_go = match &**name {
                                        DynVarName::Internal(_) => true,
                                        DynVarName::Absolute(package, _) => package.expand_to_cow(instance.variant()) == instance.name,
                                    };
                                    if needs_db_go && !has_our_var {
                                        writeln!(out, "db_go")?;
                                        has_our_var = true;
                                    }
                                    let name = name
                                        .expand(&instance.name, instance.variant());
                                    writeln!(out, "db_get {}", name)?;
                                    writeln!(out, "CONFIG[\"{}\"]=\"$RET\"", name)?;
                                },
                                InternalVarCondition::Command { .. } => (),
                            }
                        }
                        if !var.conditions.is_empty() {
                            fmt2io::write(&mut out, |out| write_ivar_conditions(out, instance, &var.conditions))?;
                        }
                    }
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
                        DebconfPriority::Dynamic { script } => script,
                    };

                    writeln!(out, "{}\ndb_input $PRIORITY {}/{}", priority, instance.name, var_name)?;
                    if !var.conditions.is_empty() {
                        writeln!(out, "fi")?;
                    }
                }
            },
        }
    }
    if let Some(file) = out.created() {
        writeln!(file, "db_go")?;
    }

    if let Some((db_type, db_config)) = instance.databases().iter().next() {
        writeln!(out, "if [ -f /usr/share/dbconfig-common/dpkg/config.{} ];", db_type.lib_name())?;
        writeln!(out, "then")?;
        writeln!(out, "\tdbc_dbtypes={}", db_type.dbconfig_db_type())?;
        writeln!(out, "\tdbc_prio_high=medium")?;
        writeln!(out, "\tdbc_prio_medium=low")?;
        writeln!(out, "\t. /usr/share/dbconfig-common/dpkg/config.{}", db_type.lib_name())?;
        if let Some(since) = &db_config.since {
            // dbconfig supports migration from non-dbconfig-managed databases and fresh
            // installations but it doesn't have a clean way to handle the case when an upgrading
            // package previously didn't have any database at all. Thus we need to manually fool it
            // into thinking this is the first installation.
            //
            // Note that we could in principle support migration to debcrafter and retain
            // dbconfig's semantics of `dbc_first_version` but that's a lot more work that no known
            // package needs now.
            writeln!(out, "\tif [ \"$1\" = configure ] && dpkg --validate-version \"$2\" &>/dev/null && dpkg --compare-versions \"{}\" gt \"$2\"", since)?;
            writeln!(out, "\tthen")?;
            writeln!(out, "\t\tdbc_go {} configure", instance.name)?;
            writeln!(out, "\telse")?;
            writeln!(out, "\t\tdbc_go {} \"$@\"", instance.name)?;
            writeln!(out, "\tfi")?;
        } else {
            writeln!(out, "\tdbc_go {} \"$@\"", instance.name)?;
        }
        writeln!(out, "fi")?;
    }

    Ok(())
}
