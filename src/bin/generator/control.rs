use crate::codegen::{LazyCreateBuilder};
use debcrafter::{PackageInstance, PackageSpec, ConfType, Set};
use std::io::{self, Write};
use debcrafter::postinst::Package;

fn calculate_dependencies<'a>(instance: &'a PackageInstance, upstream_version: &str) -> impl 'a + IntoIterator<Item=impl 'a + std::fmt::Display> {
    use std::borrow::Cow;

    const PREFIX: &str = "dbconfig-";
    const DELIMITER: &str = " | ";
    const NO_THANKS: &str = "dbconfig-no-thanks";

    let db_deps = if instance.databases().len() > 0 {
        let mut databases = String::new();
        let sum = instance.databases().iter().map(|(db, _)| db.dbconfig_dependency().len()).sum::<usize>();
        let mut dbconfig = String::with_capacity(sum + instance.databases().len() * (PREFIX.len() + DELIMITER.len()) + NO_THANKS.len());
        for (db, _) in instance.databases() {
            dbconfig.push_str(PREFIX);
            dbconfig.push_str(db.dbconfig_dependency());
            dbconfig.push_str(DELIMITER);

            let db_dep = db.dependency();

            if databases.len() > 0 {
                databases.push_str(DELIMITER);
            }
            databases.push_str(db_dep);
        }
        dbconfig.push_str(NO_THANKS);
        Some(std::iter::once(dbconfig.into()).chain(std::iter::once(Cow::Owned(databases))))
    } else {
        None
    };

    let (main_dep, config, is_service, patch, external) = match &instance.spec {
        PackageSpec::Base(base) => (None, &base.config, false, None, false),
        PackageSpec::Service(service) => {
            (Some(Cow::Borrowed(&*service.bin_package)), &service.config, true, service.min_patch.as_ref(), false)
        },
        PackageSpec::ConfExt(confext) => if confext.depends_on_extended {
            (Some(confext.extends.expand_to_cow(instance.variant())), &confext.config, false, confext.min_patch.as_ref(), confext.external)
        } else {
            (None, &confext.config, false, None, confext.external)
        },
    };
    let has_patches = !match &instance.spec {
        PackageSpec::Base(base) => &base.patch_foreign,
        PackageSpec::Service(service) => &service.patch_foreign,
        PackageSpec::ConfExt(confext) => &confext.patch_foreign,
    }.is_empty();

    let cond_to_opt = |present, dependency: &'static str| if present {
        Some(dependency.into())
    } else {
        None
    };

    let patch_deps = cond_to_opt(has_patches, "patch");
    let systemd_deps = cond_to_opt(is_service, "procps");

    config
        .iter()
        .flat_map(|(_, conf)| if let ConfType::Dynamic { evars, ..} = &conf.conf_type {
            Some(evars.keys().map(|pkg_name| pkg_name.expand_to_cow(instance.variant())))
        } else {
            None
        })
        .flatten()
        .chain(instance.depends.iter().map(|dependency| dependency.expand_to_cow(instance.constants_by_variant())))
        .chain(main_dep.map(|main_dep| if external {
            main_dep
        } else {
            Cow::Owned(patch.map(|patch| format!("{} (>= {}-{})", main_dep, upstream_version, patch)).unwrap_or_else(|| format!("{} (>= {})", main_dep, upstream_version)))
        }))
        .chain(db_deps.into_iter().flatten())
        .chain(patch_deps)
        .chain(systemd_deps)
        // This avoids duplicates
        .collect::<Set<Cow<'_, _>>>()
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

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder, upstream_version: &str, buildsystem: Option<&str>) -> io::Result<()> {
    use debcrafter::BoolOrVecTemplateString;

    let mut out = out.finalize();

    writeln!(out)?;
    writeln!(out, "Package: {}", instance.name)?;
    writeln!(out, "Priority: optional")?;
    let architecture = match &instance.spec {
        PackageSpec::Base(base) => &base.architecture,
        PackageSpec::Service(_) | PackageSpec::ConfExt(_) => &debcrafter::Architecture::All,
    };
    writeln!(out, "Architecture: {}", architecture)?;
    write!(out, "Depends: ")?;
    for dep in calculate_dependencies(instance, upstream_version) {
        write!(out, "{},\n         ", dep)?;
    }
    write!(out, "${{misc:Depends}} ${{shlibs:Depends}}")?;
    if let Some("pybuild") = buildsystem {
        write!(out, "${{python3:Depends}}")?;
    }

    writeln!(out)?;

    write_deps(&mut out, "Suggests", instance.suggests.iter().chain(instance.extended_by).map(|suggested| suggested.expand(instance.constants_by_variant())))?;
    write_deps(&mut out, "Provides", instance.provides.iter().map(|suggested| suggested.expand(instance.constants_by_variant())))?;
    write_deps(&mut out, "Conflicts", instance.conflicts.iter().map(|suggested| suggested.expand(instance.constants_by_variant())))?;

    if let PackageSpec::ConfExt(confext) = &instance.spec {
        if confext.depends_on_extended {
            write_deps(&mut out, "Recommends", instance.recommends.iter().map(|suggested| suggested.expand(instance.constants_by_variant())))?;
        } else {
            write_deps(&mut out, "Recommends", std::iter::once(confext.extends.expand_to_cow(instance.variant())).chain(instance.recommends.iter().map(|suggested| suggested.expand_to_cow(instance.constants_by_variant()))))?;
        }
        writeln!(out, "Enhances: {}", confext.extends.expand_to_cow(instance.variant()))?;
        match &confext.replaces {
            BoolOrVecTemplateString::Bool(false) => (),
            BoolOrVecTemplateString::Bool(true) => writeln!(out, "Replaces: {}", confext.extends.expand_to_cow(instance.variant()))?,
            BoolOrVecTemplateString::VecString(replaces) => write_deps(&mut out, "Replaces", replaces.iter().map(|replace| replace.expand(instance.constants_by_variant())))?,
        }
    } else {
        write_deps(&mut out, "Recommends", instance.recommends.iter().map(|suggested| suggested.expand(instance.constants_by_variant())))?;
    }
    if let Some(summary) = instance.spec.summary() {
        writeln!(out, "Description: {}", summary.expand(instance.constants_by_variant()))?;
        if let Some(long) = instance.spec.long_doc() {
            crate::codegen::paragraph(&mut out, &long.expand_to_cow(instance.constants_by_variant()))?;
        }
    }
    Ok(())
}
