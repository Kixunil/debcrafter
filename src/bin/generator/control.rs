use crate::codegen::{LazyCreateBuilder};
use debcrafter::{PackageInstance, PackageSpec, ConfType};
use std::io::{self, Write};
use std::collections::HashSet;

fn calculate_dependencies<'a>(instance: &'a PackageInstance) -> impl 'a + IntoIterator<Item=impl 'a + std::fmt::Display> {
    use std::borrow::Cow;

    const PREFIX: &str = "dbconfig-";
    const DELIMITER: &str = " | ";
    const NO_THANKS: &str = "dbconfig-no-thanks";

    let (main_dep, config, extra) = match &instance.spec {
        PackageSpec::Base(base) => (None, &base.config, None),
        PackageSpec::Service(service) => {
            let extra = if service.databases.len() > 0 {
                let mut databases = String::new();
                let sum = service.databases.iter().map(|(db, _)| db.len()).sum::<usize>();
                let mut dbconfig = String::with_capacity(sum + service.databases.len() * (PREFIX.len() + DELIMITER.len()) + NO_THANKS.len());
                for (db_name, _) in &service.databases {
                    dbconfig.push_str(PREFIX);
                    dbconfig.push_str(db_name);
                    dbconfig.push_str(DELIMITER);

                    let db_dep = match &**db_name {
                        "pgsql" => "postgresql",
                        "mysql" => "default-mysql-server",
                        x => panic!("Unsupported database: {}", x),
                    };

                    if databases.len() > 0 {
                        databases.push_str(DELIMITER);
                    }
                    databases.push_str(db_dep);
                }
                dbconfig.push_str(NO_THANKS);
                Some(std::iter::once(dbconfig.into()).chain(std::iter::once(databases.into())))
            } else {
                None
            };
            (Some(&service.bin_package), &service.config, extra)
        },
        PackageSpec::ConfExt(confext) => (Some(&confext.extends), &confext.config, None),
    };
    config
        .iter()
        .flat_map(|(_, conf)| if let ConfType::Dynamic { evars, ..} = &conf.conf_type { Some(evars) } else { None })
        .flatten()
        .map(|(pkg, _)| pkg.as_str())
        .chain(main_dep.map(String::as_str))
        .chain(instance.depends.iter().map(AsRef::as_ref))
        .map(Into::into)
        .chain(extra.into_iter().flatten())
        // This avoids duplicates
        .collect::<HashSet<Cow<'_, _>>>()
}

fn write_deps<W, I>(mut out: W, name: &str, deps: I) -> io::Result<()> where W: io::Write, I: IntoIterator, <I as IntoIterator>::Item: std::fmt::Display {
    let mut iter = deps.into_iter();
    if let Some(first) = iter.next() {
        write!(out, "{}: {}", name, first)?;
        for item in iter {
            write!(out, ",\n         {}", item)?;
        }
        writeln!(out)?;
    }
    Ok(())
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

    write_deps(&mut out, "Recommends", instance.recommends)?;
    write_deps(&mut out, "Suggests", instance.suggests)?;
    write_deps(&mut out, "Provides", instance.provides)?;
    write_deps(&mut out, "Conflicts", instance.conflicts)?;

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
