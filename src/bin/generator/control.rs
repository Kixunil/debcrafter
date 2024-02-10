use crate::codegen::{LazyCreateBuilder};
use debcrafter::im_repr::{PackageOps, PackageInstance, PackageSpec, ConfType, Architecture};
use debcrafter::Set;
use std::io::{self, Write};
use std::borrow::Cow;

fn calculate_dependencies<'a>(instance: &'a PackageInstance, upstream_version: &str) -> impl 'a + Iterator<Item=impl 'a + std::fmt::Display + Into<Cow<'a, str>>> {

    const PREFIX: &str = "dbconfig-";
    const DELIMITER: &str = " | ";
    const NO_THANKS: &str = "dbconfig-no-thanks";

    let db_deps = if !instance.databases().is_empty() {
        let mut databases = String::new();
        let sum = instance.databases().iter().map(|(db, opts)| db.dbconfig_dependency().len() + opts.min_version.as_ref().map(|min_version| min_version.len() + 6).unwrap_or_default()).sum::<usize>();
        let mut dbconfig = String::with_capacity(sum + instance.databases().len() * (PREFIX.len() + DELIMITER.len()) + NO_THANKS.len());
        for (db, opts) in instance.databases() {
            dbconfig.push_str(PREFIX);
            dbconfig.push_str(db.dbconfig_dependency());
            dbconfig.push_str(DELIMITER);

            let db_dep = db.dependency();

            if !databases.is_empty() {
                databases.push_str(DELIMITER);
            }
            databases.push_str(db_dep);
            if let Some(min_version) = &opts.min_version {
                databases.push_str(" (>= ");
                databases.push_str(min_version);
                databases.push_str(")");
            }
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

#[derive(Eq, PartialEq, Debug, serde_derive::Serialize)]
#[serde(rename_all = "snake_case")]
enum Priority {
    Optional,
}

#[derive(serde_derive::Serialize, Debug, Eq, PartialEq)]
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

fn create_package(instance: &PackageInstance<'_>, upstream_version: &str, buildsystem: Option<&str>, cb: impl FnOnce(&Package<'_>) -> io::Result<()>) -> io::Result<()> {
    use debcrafter::im_repr::BoolOrVecTemplateString;
    use std::fmt::Write;

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

    cb(&package)
}

pub fn generate(instance: &PackageInstance, out: LazyCreateBuilder, upstream_version: &str, buildsystem: Option<&str>) -> io::Result<()> {
    let mut out = out.finalize();

    create_package(instance, upstream_version, buildsystem, move |package| {
        writeln!(out)?;

        rfc822_like::to_writer(out, &package)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))
    })
}

#[cfg(test)]
mod tests {
    use super::{Priority, create_package};
    use std::convert::TryFrom;
    use std::borrow::Cow;
    use crate::{PackageInstance, Map, Set};
    use debcrafter::template::TemplateString;
    use debcrafter::im_repr::{PackageSpec, BasePackageSpec, Architecture};
    use debcrafter::types::Variant;

    fn check_package(modify: impl FnOnce(&mut PackageInstance<'_>, &mut super::Package<'_>)) {
        let pkg_spec = BasePackageSpec {
            architecture: Architecture::All,
        };

        let mut instance = PackageInstance {
            name: Cow::Borrowed("foo"),
            variant: None,
            map_variants: &Map::new(),
            summary: &TemplateString::try_from("bar".to_owned()).unwrap(),
            long_doc: None,
            spec: &PackageSpec::Base(pkg_spec),
            config: &Map::default(),
            databases: &Map::default(),
            includes: None,
            depends: &Set::default(),
            provides: &Set::default(),
            recommends: &Set::default(),
            suggests: &Set::default(),
            conflicts: &Set::default(),
            extended_by: &Set::default(),
            add_files: &[],
            add_dirs: &[],
            add_links: &[],
            add_manpages: &[],
            alternatives: &Map::default(),
            patch_foreign: &Map::default(),
            extra_triggers: &Set::default(),
            migrations: &Map::default(),
            plug: &[],
        };

        let mut expected_package = super::Package {
            package: "foo",
            architecture: Architecture::All,
            priority: Priority::Optional,
            depends: &[Cow::Borrowed("${misc:Depends}"), Cow::Borrowed("${shlibs:Depends}")],
            recommends: &[],
            suggests: &[],
            provides: &[],
            conflicts: &[],
            enhances: &[],
            replaces: &[],
            description: "bar",
        };

        modify(&mut instance, &mut expected_package);

        create_package(&instance, "1.0.0", None, |package| {
            assert_eq!(*package, expected_package);

            Ok(())
        }).unwrap();
    }

    #[test]
    fn basic() {
        check_package(|_, _| ());
    }

    #[test]
    fn one_dep() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref DEPENDS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz".to_owned()).unwrap());
                    pkgs
                };
            }
            instance.depends = &DEPENDS;
            package.depends = &[Cow::Borrowed("${misc:Depends}"), Cow::Borrowed("${shlibs:Depends}"), Cow::Borrowed("baz")];
        });
    }

    #[test]
    fn one_recommends() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref RECOMMENDS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz".to_owned()).unwrap());
                    pkgs
                };
            }
            instance.recommends = &RECOMMENDS;
            package.recommends = &[Cow::Borrowed("baz")];
        });
    }

    #[test]
    fn one_suggests() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref SUGGESTS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz".to_owned()).unwrap());
                    pkgs
                };
            }
            instance.suggests = &SUGGESTS;
            package.suggests = &[Cow::Borrowed("baz")];
        });
    }

    #[test]
    fn one_conflicts() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref CONFLICTS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz".to_owned()).unwrap());
                    pkgs
                };
            }
            instance.conflicts = &CONFLICTS;
            package.conflicts = &[Cow::Borrowed("baz")];
        });
    }

    #[test]
    fn one_dep_variant() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref DEPENDS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz-{variant}".to_owned()).unwrap());
                    pkgs
                };

                static ref VARIANT: Variant = Variant::try_from("stuff".to_owned()).unwrap();
            }
            instance.variant = Some(&VARIANT);
            instance.depends = &DEPENDS;
            package.depends = &[Cow::Borrowed("${misc:Depends}"), Cow::Borrowed("${shlibs:Depends}"), Cow::Borrowed("baz-stuff")];
        });
    }

    #[test]
    fn one_recommends_variant() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref RECOMMENDS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz-{variant}".to_owned()).unwrap());
                    pkgs
                };

                static ref VARIANT: Variant = Variant::try_from("stuff".to_owned()).unwrap();
            }
            instance.variant = Some(&VARIANT);
            instance.recommends = &RECOMMENDS;
            package.recommends = &[Cow::Borrowed("baz-stuff")];
        });
    }

    #[test]
    fn one_suggests_variant() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref SUGGESTS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz-{variant}".to_owned()).unwrap());
                    pkgs
                };

                static ref VARIANT: Variant = Variant::try_from("stuff".to_owned()).unwrap();
            }
            instance.variant = Some(&VARIANT);
            instance.suggests = &SUGGESTS;
            package.suggests = &[Cow::Borrowed("baz-stuff")];
        });
    }

    #[test]
    fn one_conflicts_variant() {
        check_package(|instance, package| {
            lazy_static::lazy_static! {
                static ref CONFLICTS: Set<TemplateString> = {
                    let mut pkgs = Set::new();
                    pkgs.insert(TemplateString::try_from("baz-{variant}".to_owned()).unwrap());
                    pkgs
                };

                static ref VARIANT: Variant = Variant::try_from("stuff".to_owned()).unwrap();
            }
            instance.variant = Some(&VARIANT);
            instance.conflicts = &CONFLICTS;
            package.conflicts = &[Cow::Borrowed("baz-stuff")];
        });
    }
}
