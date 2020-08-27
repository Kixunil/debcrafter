use std::{io, fs};
use std::io::Write;
use std::path::{Path, PathBuf};
use codegen::{LazyCreateBuilder};
use debcrafter::{Package, PackageInstance, ServiceInstance, Map, Set};
use serde_derive::Deserialize;
use std::borrow::Borrow;

mod generator;
mod codegen;

#[derive(Deserialize)]
pub struct Repository {
    pub maintainer: String,
    pub sources: Map<String, Source>,
}

#[derive(Deserialize)]
pub struct Source {
    // TODO: enum with validation instead?
    pub version: String,
    pub section: String,
    #[serde(default)]
    pub build_depends: Vec<String>,
    #[serde(default, rename = "with")]
    pub with_components: Set<String>,
    #[serde(default)]
    pub buildsystem: Option<String>,
    pub packages: Set<String>,
    #[serde(default)]
    pub skip_debug_symbols: bool,
}

#[derive(Deserialize)]
pub struct SingleSource {
    pub name: String,
    pub maintainer: Option<String>,
    #[serde(flatten)]
    pub source: Source,
}

struct ServiceRule {
    unit_name: String,
    refuse_manual_start: bool,
    refuse_manual_stop: bool,
}

static FILE_GENERATORS: &[(&str, fn(&PackageInstance, LazyCreateBuilder) -> io::Result<()>)] = &[
    ("config", crate::generator::config::generate),
    ("install", crate::generator::install::generate),
    ("dirs", crate::generator::dirs::generate),
    ("links", crate::generator::links::generate),
    ("manpages", crate::generator::manpages::generate),
    ("preinst", crate::generator::preinst::generate),
    ("postinst", crate::generator::postinst::generate),
    ("prerm", crate::generator::prerm::generate),
    ("postrm", crate::generator::postrm::generate),
    ("templates", crate::generator::templates::generate),
    ("triggers", crate::generator::triggers::generate),
];

fn gen_rules<I>(deb_dir: &Path, source: &Source, systemd_services: I) -> io::Result<()> where I: IntoIterator, <I as IntoIterator>::IntoIter: ExactSizeIterator, <I as IntoIterator>::Item: Borrow<ServiceRule> {
    let systemd_services = systemd_services.into_iter();
    let mut out = fs::File::create(deb_dir.join("rules")).expect("Failed to create control file");

    writeln!(out, "#!/usr/bin/make -f")?;
    writeln!(out)?;
    writeln!(out, "%:")?;
    write!(out, "\tdh $@")?;
    for component in &source.with_components {
        write!(out, " --with {}", component)?;
    }
    if let Some(buildsystem) = &source.buildsystem {
        write!(out, " --buildsystem {}", buildsystem)?;
    }
    writeln!(out)?;

    if systemd_services.len() > 0 {
        writeln!(out)?;
        writeln!(out, "override_dh_installsystemd:")?;
        for service in systemd_services {
            let service = service.borrow();

            write!(out, "\tdh_installsystemd --name={}", service.unit_name)?;
            if service.refuse_manual_start {
                write!(out, " --no-start")?;
            }
            if service.refuse_manual_stop {
                write!(out, " --no-stop-on-upgrade --no-restart-after-upgrade")?;
            }
            writeln!(out)?;
        }
    }
    if source.skip_debug_symbols {
        writeln!(out)?;
        writeln!(out, "override_dh_dwz:")?;
    }
    Ok(())
}

fn gen_control(deb_dir: &Path, name: &str, source: &Source, maintainer: &str, needs_dh_systemd: bool) -> io::Result<()> {
    let mut out = fs::File::create(deb_dir.join("control")).expect("Failed to create control file");

    writeln!(out, "Source: {}", name)?;
    writeln!(out, "Section: {}", source.section)?;
    writeln!(out, "Priority: optional")?;
    writeln!(out, "Maintainer: {}", maintainer)?;
    write!(out, "Build-Depends: debhelper (>= 9)")?;
    if needs_dh_systemd {
        write!(out, ",\n               dh-systemd (>= 1.15.5)")?;
    }
    for build_dep in &source.build_depends {
        write!(out, ",\n               {}", build_dep)?;
    }
    writeln!(out)
}

fn copy_changelog(deb_dir: &Path, source_dir: &Path, name: &str) {
    let mut source = source_dir.join(name);
    source.set_extension("changelog");
    let dest = deb_dir.join("changelog");

    match fs::copy(&source, &dest) {
        Ok(_) => (),
        Err(ref err) if err.kind() == std::io::ErrorKind::NotFound => (),
        Err(err) => panic!("Failed to copy changelog of package {} from {} to {}: {}", name, source.display(), dest.display(), err),
    }
}

fn load_package(source_dir: &Path, package: &str) -> (Package, PathBuf) {
    let mut filename = source_dir.join(package);
    filename.set_extension("sps");
    let package = Package::load(&filename);
    (package, filename)
}

fn create_lazy_builder(dest_dir: &Path, name: &str, extension: &str, append: bool) -> LazyCreateBuilder {
    let mut file_name = dest_dir.join(name);
    file_name.set_extension(extension);
    LazyCreateBuilder::new(file_name, append)
}

fn gen_source(dest: &Path, source_dir: &Path, name: &str, source: &mut Source, maintainer: &str, mut dep_file: Option<&mut fs::File>) {
    let dir = dest.join(format!("{}-{}", name, source.version));
    let deb_dir = dir.join("debian");
    fs::create_dir_all(&deb_dir).expect("Failed to create debian directory");
    copy_changelog(&deb_dir, source_dir, name);

    let mut deps = Set::new();
    let mut deps_opt = dep_file.as_mut().map(|_| { &mut deps });

    // TODO: calculate dh-systemd dep instead
    gen_control(&deb_dir, name, source, maintainer, true).expect("Failed to generate control");
    std::fs::write(deb_dir.join("compat"), "12\n").expect("Failed to write debian/compat");

    let services = source.packages
        .iter()
        .map(|package| load_package(source_dir, &package))
        .filter_map(|(package, filename)| {
            use debcrafter::postinst::Package as PostinstPackage;
            let deps_opt = deps_opt.as_mut().map(|deps| { deps.insert(filename); &mut **deps});
            let includes = package.load_includes(source_dir, deps_opt);
            let instance = package.instantiate(None, Some(&includes)).expect("Invalid variant");

            for &(extension, generator) in FILE_GENERATORS {
                let out = create_lazy_builder(&deb_dir, &package.name, extension, false);
                generator(&instance, out).expect("Failed to generate file");
            }

            if let Some(service_name) = instance.service_name() {
                let out = create_lazy_builder(&deb_dir, &package.name, &format!("{}.service", service_name), false);
                crate::generator::service::generate(&instance, out).expect("Failed to generate file");
            }

            let out = create_lazy_builder(&deb_dir, "control", "", true);
            generator::control::generate(&instance, out).expect("Failed to generate file");
            generator::static_files::generate(&instance, &dir).expect("Failed to generate static files");

            instance.as_service().map(|service| ServiceRule {
                unit_name: ServiceInstance::service_name(&service).to_owned(),
                refuse_manual_start: service.spec.refuse_manual_start,
                refuse_manual_stop: service.spec.refuse_manual_stop,
            })
        })
        .collect::<Vec<_>>();

    if let Some(dep_file) = dep_file {
        (|| -> Result<(), io::Error> {
            write!(dep_file, "{}/debcrafter-{}.stamp:", dest.display(), name)?;
            for dep in &deps {
                write!(dep_file, " {}", dep.display())?;
            }
            writeln!(dep_file, "\n")?;
            Ok(())
        })().expect("Failed to write into dependency file")
    }

    gen_rules(&deb_dir, source, &services).expect("Failed to generate rules");
}

fn main() {
    let mut args = std::env::args_os();
    args.next().expect("Not even zeroth argument given");
    let spec_file = std::path::PathBuf::from(args.next().expect("Source not specified."));
    let dest = std::path::PathBuf::from(args.next().expect("Dest not specified."));
    let mut split_source = false;
    let mut write_deps = None;

    while let Some(arg) = args.next() {
        if arg == "--split-source" {
            split_source = true;
        }

        if arg == "--write-deps" {
            let file = args.next().expect("missing argument for --write-deps");
            write_deps = Some(file.into_string().expect("Invalid UTF econding"));
        }
    }

    let mut dep_file = write_deps.map(|dep_file| fs::File::create(dep_file).expect("failed to open dependency file"));

    if split_source {
        let mut source = debcrafter::load_file::<SingleSource, _>(&spec_file);
        let maintainer = source.maintainer.or_else(|| std::env::var("DEBEMAIL").ok()).expect("missing maintainer");
        
        gen_source(&dest, spec_file.parent().unwrap_or(".".as_ref()), &source.name, &mut source.source, &maintainer, dep_file.as_mut())
    } else {
        let repo = debcrafter::load_file::<Repository, _>(&spec_file);
        
        for (name, mut source) in repo.sources {
            gen_source(&dest, spec_file.parent().unwrap_or(".".as_ref()), &name, &mut source, &repo.maintainer, dep_file.as_mut())
        }
    }
}
