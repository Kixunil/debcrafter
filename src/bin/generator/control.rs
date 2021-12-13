use crate::codegen::{LazyCreateBuilder};
use debcrafter::im_repr::{PackageOps, PackageInstance, PackageSpec, ConfType, Architecture};
use debcrafter::Set;
use std::io::{self, Write};
use std::borrow::Cow;

fn calculate_dependencies<'a>(instance: &'a PackageInstance, upstream_version: &str) -> impl 'a + Iterator<Item=impl 'a + std::fmt::Display + Into<Cow<'a, str>>> {

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

    let (main_dep, is_service, patch, external) = match &instance.spec {
        PackageSpec::Base(_) => (None, false, None, false),
        PackageSpec::Service(service) => {
            (Some(Cow::Borrowed(&*service.bin_package)), true, service.min_patch.as_ref(), false)
        },
        PackageSpec::ConfExt(confext) => if confext.depends_on_extended {
            (Some(confext.extends.expand_to_cow(instance.variant())), false, confext.min_patch.as_ref(), confext.external)
        } else {
            (None, false, None, confext.external)
        },
    };
    let has_patches = !instance.patch_foreign.is_empty();

    let cond_to_opt = |present, dependency: &'static str| if present {
        Some(dependency.into())
    } else {
        None
    };

    let patch_deps = cond_to_opt(has_patches, "patch");
    let systemd_deps = cond_to_opt(is_service, "procps");

    instance.config
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
        .into_iter()
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

#[derive(serde_derive::Serialize)]
#[serde(rename_all = "snake_case")]
enum Priority {
    Optional,
}

#[derive(serde_derive::Serialize)]
#[serde(rename_all = "PascalCase")]
struct Package<'a> {
    package: &'a str,
    priority: Priority,
    architecture: Architecture,
    depends: &'a [Cow<'a, str>],
    recommends: &'a [Cow<'a, str>],
    suggests: &'a [Cow<'a, str>],
    provides: &'a [Cow<'a, str>],
    conflicts: &'a [Cow<'a, str>],
    enhances: &'a [Cow<'a, str>],
    replaces: &'a [Cow<'a, str>],
    description: &'a str,
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder, upstream_version: &str, buildsystem: Option<&str>) -> io::Result<()> {
    use debcrafter::im_repr::BoolOrVecTemplateString;
    use std::fmt::Write;

    let mut out = out.finalize();

    writeln!(out)?;

    let architecture = match &instance.spec {
        PackageSpec::Base(base) => &base.architecture,
        PackageSpec::Service(_) | PackageSpec::ConfExt(_) => &Architecture::All,
    };

    let python_depends = match buildsystem {
        Some("pybuild") => Some(Cow::Borrowed("${python3:Depends}")),
        _ => None,
    };

    let fixed_depends = std::iter::once(Cow::Borrowed("${misc:Depends}"))
        .chain(std::iter::once(Cow::Borrowed("${shlibs:Depends}")))
        .chain(python_depends);

    let depends = fixed_depends.chain(calculate_dependencies(instance, upstream_version).map(Into::into)).collect::<Vec<_>>();
    let suggests = instance.suggests.iter().chain(instance.extended_by).map(|suggested| suggested.expand_to_cow(instance.constants_by_variant())).collect::<Vec<_>>();
    let provides = instance.provides.iter().map(|provided| provided.expand_to_cow(instance.constants_by_variant())).collect::<Vec<_>>();
    let conflicts = instance.conflicts.iter().map(|conflicting| conflicting.expand_to_cow(instance.constants_by_variant())).collect::<Vec<_>>();
    let (recommends, enhances, replaces) = if let PackageSpec::ConfExt(confext) = &instance.spec {
        let recommends = if confext.depends_on_extended {
            instance.recommends.iter().map(|suggested| suggested.expand_to_cow(instance.constants_by_variant())).collect::<Vec<_>>()
        } else {
            std::iter::once(confext.extends.expand_to_cow(instance.variant()))
                .chain(instance.recommends.iter().map(|suggested| suggested.expand_to_cow(instance.constants_by_variant())))
                .collect::<Vec<_>>()
        };
        let enhances = vec![confext.extends.expand_to_cow(instance.variant())];
        let replaces = match &confext.replaces {
            BoolOrVecTemplateString::Bool(false) => Vec::new(),
            BoolOrVecTemplateString::Bool(true) => vec![confext.extends.expand_to_cow(instance.variant())],
            BoolOrVecTemplateString::VecString(replaces) => replaces.iter().map(|replace| replace.expand_to_cow(instance.constants_by_variant())).collect(),
        };

        (recommends, enhances, replaces)
    } else {
        (instance.recommends.iter().map(|suggested| suggested.expand_to_cow(instance.constants_by_variant())).collect::<Vec<_>>(), Vec::new(), Vec::new())
    };

    let mut description = String::new();
    write!(description, "{}", instance.summary.expand(instance.constants_by_variant())).expect("writing to memory doesn't fail");
    if let Some(long) = instance.long_doc {
        write!(description, "\n{}", long.expand_to_cow(instance.constants_by_variant())).expect("writing to memory doesn't fail");
    }

    let package = Package {
        package: &instance.name,
        architecture: *architecture,
        priority: Priority::Optional,
        depends: &depends,
        recommends: &recommends,
        suggests: &suggests,
        provides: &provides,
        conflicts: &conflicts,
        enhances: &enhances,
        replaces: &replaces,
        description: &description,
    };

    rfc822_like::to_writer(out, &package)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;

    Ok(())
}
