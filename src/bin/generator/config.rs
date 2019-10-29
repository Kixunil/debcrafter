use std::io::{self, Write};
use debcrafter::{PackageInstance, PackageConfig, ConfType, DebconfPriority};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let header = "#!/bin/bash

. /usr/share/debconf/confmodule

";
    let mut out = out.set_header(header).finalize();

    //TODO: data validation
    for (_, config) in instance.config() {
        match &config.conf_type {
            ConfType::Static { .. } => (),
            ConfType::Dynamic { ivars, .. } =>  {
                for (var_name, var) in ivars {
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

    Ok(())
}
