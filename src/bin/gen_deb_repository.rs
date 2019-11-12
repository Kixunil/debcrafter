use std::{io, fs};
use std::io::Write;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use codegen::{LazyCreateBuilder};
use debcrafter::{Package, PackageInstance};
use serde_derive::Deserialize;

mod generator;
mod codegen;

#[derive(Deserialize)]
pub struct Repository {
    pub maintainer: String,
    pub sources: HashMap<String, Source>,
}

#[derive(Deserialize)]
pub struct Source {
    // TODO: enum with validation instead?
    pub version: String,
    pub section: String,
    #[serde(default)]
    pub build_depends: Vec<String>,
    #[serde(default, rename = "with")]
    pub with_components: HashSet<String>,
    pub packages: HashSet<String>,
}

static FILE_GENERATORS: &[(&str, fn(&PackageInstance, LazyCreateBuilder) -> io::Result<()>)] = &[
    ("config", crate::generator::config::generate),
    ("install", crate::generator::install::generate),
    ("links", crate::generator::links::generate),
    ("postinst", crate::generator::postinst::generate),
    ("postrm", crate::generator::postrm::generate),
    ("templates", crate::generator::templates::generate),
    ("triggers", crate::generator::triggers::generate),
];

fn gen_rules<I>(deb_dir: &Path, source: &Source, systemd_services: I) -> io::Result<()> where I: IntoIterator, <I as IntoIterator>::IntoIter: ExactSizeIterator, <I as IntoIterator>::Item: std::fmt::Display {
    let systemd_services = systemd_services.into_iter();
    let mut out = fs::File::create(deb_dir.join("rules")).expect("Failed to create control file");

    writeln!(out, "#!/usr/bin/make -f")?;
    writeln!(out)?;
    writeln!(out, "%:")?;
    write!(out, "\tdh $@")?;
    for component in &source.with_components {
        write!(out, " --with {}", component)?;
    }
    writeln!(out)?;

    if systemd_services.len() > 0 {
        writeln!(out)?;
        writeln!(out, "override_dh_systemd_enable:")?;
        for service in systemd_services {
            writeln!(out, "\tdh_systemd_enable --name={}", service)?;
        }
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

fn load_package(source_dir: &Path, package: &str) -> Package {
    let mut filename = source_dir.join(package);
    filename.set_extension("sps");
    Package::load(&filename)
}

fn create_lazy_builder(dest_dir: &Path, name: &str, extension: &str, append: bool) -> LazyCreateBuilder {
    let mut file_name = dest_dir.join(name);
    file_name.set_extension(extension);
    LazyCreateBuilder::new(file_name, append)
}

fn gen_source(dest: &Path, source_dir: &Path, name: &str, source: &mut Source, maintainer: &str) {
    let dir = dest.join(format!("{}-{}", name, source.version));
    let deb_dir = dir.join("debian");
    fs::create_dir_all(&deb_dir).expect("Failed to create debian directory");
    copy_changelog(&deb_dir, source_dir, name);

    // TODO: calculate dh-systemd dep instead
    gen_control(&deb_dir, name, source, maintainer, true).expect("Failed to generate control");
    std::fs::write(deb_dir.join("compat"), "10\n").expect("Failed to write debian/compat");

    let services = source.packages
        .iter()
        .map(|package| load_package(source_dir, &package))
        .map(|package| {
            let includes = package.load_includes(source_dir);
            (package, includes)
        })
        .filter_map(|(package, includes)| {
            use debcrafter::postinst::Package as PostinstPackage;

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

            instance.service_name().map(String::from)
        })
        .collect::<Vec<_>>();

    if services.len() > 0 {
        source.with_components.insert("systemd".to_owned());
    }
    gen_rules(&deb_dir, source, &services).expect("Failed to generate rules");
}

fn main() {
    let (spec_file, dest, _) = codegen::get_args();

    let repo = debcrafter::load_file::<Repository, _>(&spec_file);
    
    for (name, mut source) in repo.sources {
        gen_source(&dest, spec_file.parent().unwrap_or(".".as_ref()), &name, &mut source, &repo.maintainer)
    }
}
