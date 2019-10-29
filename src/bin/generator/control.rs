use crate::codegen::{LazyCreateBuilder};
use debcrafter::{PackageInstance, PackageSpec, ConfType};
use std::io::{self, Write};
use std::collections::HashSet;

fn calculate_dependencies<'a>(instance: &'a PackageInstance) -> impl IntoIterator<Item=&'a str> {
    let (main_dep, config) = match &instance.spec {
        PackageSpec::Service(service) => (&service.bin_package, &service.config),
        PackageSpec::ConfExt(confext) => (&confext.extends, &confext.config),
    };
    config
        .iter()
        .flat_map(|(_, conf)| if let ConfType::Dynamic { evars, ..} = &conf.conf_type { Some(evars) } else { None })
        .flatten()
        .map(|(pkg, _)| pkg.as_str())
        .chain(Some(main_dep.as_str()))
        // This avoids duplicates
        .collect::<HashSet<_>>()
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder) -> io::Result<()> {
    let mut out = out.finalize();

    writeln!(out)?;
    writeln!(out, "Package: {}", instance.name)?;
    writeln!(out, "Priority: optional")?;
    writeln!(out, "Architecture: all")?;
    write!(out, "Depends: ")?;
    for dep in calculate_dependencies(instance) {
        write!(out, "{},\n         ", dep)?;
    }
    writeln!(out, "${{misc:Depends}}")?;
    // TODO
    //writeln!(out, "Recommends: ")?;
    //writeln!(out, "Suggests: ")?;
    //writeln!(out, "Provides: ")?;
    if let PackageSpec::ConfExt(confext) = &instance.spec {
        writeln!(out, "Enhances: {}", confext.extends)?;
        if confext.replaces {
            writeln!(out, "Replaces: {}", confext.extends)?;
        }
    }
    if let Some(summary) = instance.spec.summary() {
        writeln!(out, "Description: {}", summary)?;
        if let Some(long) = instance.spec.long_doc() {
            crate::codegen::paragraph(&mut out, long)?;
        }
    }
    Ok(())
}
