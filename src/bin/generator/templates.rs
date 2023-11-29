use std::io::{self, Write};
use debcrafter::im_repr::{PackageInstance, PackageConfig, PackageOps, ConfType, VarType};
use crate::codegen::{LazyCreateBuilder};

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    for config in instance.config().values() {
        if let ConfType::Dynamic { ivars, .. } = &config.conf_type {
            for (var, var_spec) in ivars {
                out.separator("\n")?;

                writeln!(out, "Template: {}/{}", instance.name, var)?;

                let template_type = if let VarType::Bool = var_spec.ty {
                    "boolean"
                } else {
                    "string"
                };
                writeln!(out, "Type: {}", template_type)?;

                if let Some(default) = &var_spec.default {
                    writeln!(out, "Default: {}", default.expand(instance.constants_by_variant()))?;
                }
                writeln!(out, "Description: {}", var_spec.summary.expand(instance.constants_by_variant()))?;
                if let Some(long_doc) = &var_spec.long_doc {
                    crate::codegen::paragraph(&mut out, &long_doc.expand_to_cow(instance.constants_by_variant()))?;
                }
            }
        }
    }
    Ok(())
}
