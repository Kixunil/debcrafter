use std::fmt;
use serde_derive::Deserialize;
use std::path::{Path, PathBuf};
use std::borrow::Cow;
use std::convert::TryFrom;

pub mod postinst;

pub type Map<K, V> = std::collections::BTreeMap<K, V>;
pub type Set<T> = std::collections::BTreeSet<T>;

fn create_true() -> bool {
    true
}

pub trait PackageConfig {
    fn config(&self) -> &Map<String, Config>;
}

impl<'a, T> PackageConfig for &'a T where T: PackageConfig {
    fn config(&self) -> &Map<String, Config> {
        (*self).config()
    }
}

impl<'a> PackageConfig for PackageInstance<'a> {
    fn config(&self) -> &Map<String, Config> {
        &self.spec.config()
    }
}

impl<'a> PackageConfig for ServiceInstance<'a> {
    fn config(&self) -> &Map<String, Config> {
        &self.spec.config()
    }
}

impl PackageConfig for ServicePackageSpec {
    fn config(&self) -> &Map<String, Config> {
        &self.config
    }
}

#[derive(Deserialize)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    pub variants: Set<String>,
    #[serde(flatten)]
    pub spec: PackageSpec,
    #[serde(default)]
    pub depends: Set<String>,
    #[serde(default)]
    pub provides: Set<String>,
    #[serde(default)]
    pub recommends: Set<String>,
    #[serde(default)]
    pub suggests: Set<String>,
    #[serde(default)]
    pub conflicts: Set<String>,
    #[serde(default)]
    pub extended_by: Set<String>,
    #[serde(default)]
    pub extra_triggers: Set<String>,
    #[serde(default)]
    pub migrations: Map<MigrationVersion, Migration>,
}

pub type FileDeps<'a> = Option<&'a mut Set<PathBuf>>;

fn load_include<'a>(dir: &'a Path, name: &'a str, mut deps: FileDeps<'a>) -> impl 'a + FnMut() -> Package {
    move || {
        let mut file = dir.join(name);
        file.set_extension("sps");
        let package = Package::load(&file);
        deps.as_mut().map(|deps| deps.insert(file));
        package
    }
}

pub fn load_file<T: for<'a> serde::Deserialize<'a>, P: AsRef<Path>>(file: P) -> T {
    let file = file.as_ref();
    let spec = std::fs::read(file).unwrap_or_else(|err| panic!("Failed to read {}: {}", file.display(), err));
    toml::from_slice(&spec).unwrap_or_else(|err| panic!("Failed to parse {}: {}", file.display(), err))
}

impl Package {
    pub fn load<P: AsRef<Path>>(file: P) -> Self {
        load_file(file)
    }

    pub fn load_includes<P: AsRef<Path>>(&self, dir: P, mut deps: Option<&mut Set<PathBuf>>) -> Map<String, Package> {
        let mut result = Map::new();
        for (_, conf) in self.config() {
            if let ConfType::Dynamic { evars, .. } = &conf.conf_type {
                for (pkg, _) in evars {
                    let mut deps = deps.as_mut().map(|deps| &mut **deps);
                    result.entry(pkg.to_owned()).or_insert_with(load_include(dir.as_ref(), pkg, deps));
                }
            }
        }

        if let PackageSpec::ConfExt(ConfExtPackageSpec { extends, external: false, .. }) = &self.spec {
            result.entry(extends.clone()).or_insert_with(load_include(dir.as_ref(), &extends, deps));
        }

        result
    }

    pub fn instantiate<'a>(&'a self, variant: Option<&'a str>, includes: Option<&'a Map<String, Package>>) -> Option<PackageInstance<'a>> {
        let name = if let Some(variant) = variant {
            // Sanity check
            if !self.variants.contains(variant) {
                return None;
            }

            (&[&self.name.as_str(), variant]).join("-").into()
        } else {
            if self.variants.len() > 0 {
                return None;
            }
            (&self.name).into()
        };

        Some(PackageInstance {
            name,
            variant,
            spec: &self.spec,
            includes,
            depends: &self.depends,
            provides: &self.provides,
            recommends: &self.recommends,
            suggests: &self.suggests,
            conflicts: &self.conflicts,
            extended_by: &self.extended_by,
            extra_triggers: &self.extra_triggers,
            migrations: &self.migrations,
        })
    }
}

impl PackageConfig for Package {
    fn config(&self) -> &Map<String, Config> {
        self.spec.config()
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum PackageSpec {
    Service(ServicePackageSpec),
    ConfExt(ConfExtPackageSpec),
    Base(BasePackageSpec),
}

impl PackageSpec {
    pub fn summary(&self) -> &Option<String> {
        match self {
            PackageSpec::Base(base) => &base.summary,
            PackageSpec::Service(service) => &service.summary,
            PackageSpec::ConfExt(confext) => &confext.summary,
        }
    }

    pub fn long_doc(&self) -> &Option<String> {
        match self {
            PackageSpec::Base(base) => &base.long_doc,
            PackageSpec::Service(service) => &service.long_doc,
            PackageSpec::ConfExt(confext) => &confext.long_doc,
        }
    }
}

impl PackageConfig for PackageSpec {
    fn config(&self) -> &Map<String, Config> {
        match self {
            PackageSpec::Base(base) => &base.config,
            PackageSpec::Service(service) => &service.config,
            PackageSpec::ConfExt(confext) => &confext.config,
        }
    }
}

#[derive(Deserialize)]
pub struct Migration {
    pub config: String
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(try_from = "String")]
pub struct MigrationVersion(String);

impl MigrationVersion {
    pub fn version(&self) -> &str {
        &self.0[3..]
    }
}

impl std::cmp::Ord for MigrationVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self == other {
            return std::cmp::Ordering::Equal;
        }
        if std::process::Command::new("dpkg")
            .args(&["--compare-versions", self.version(), "lt", &other.version()])
            .spawn()
            .expect("Failed to compare versions")
            .wait()
            .expect("Failed to compare versions")
            .success() {
                std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    }
}

impl std::cmp::PartialOrd for MigrationVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<String> for MigrationVersion {
    type Error = MigrationVersionError;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        if !string.starts_with("<< ") {
            return Err(MigrationVersionErrorInner::BadPrefix(string).into());
        }
        let output = std::process::Command::new("dpkg")
            .args(&["--validate-version", &string[3..]])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to compare versions")
            .wait_with_output()
            .expect("Failed to compare versions");
        if output.status.success() {
            Ok(MigrationVersion(string))
        } else {
            let err_message = String::from_utf8(output.stderr).expect("Failed to decode error message");
            Err(MigrationVersionErrorInner::Invalid(err_message).into())
        }
    }
}

#[derive(Debug)]
enum MigrationVersionErrorInner {
    BadPrefix(String),
    Invalid(String),
}

#[derive(Debug)]
pub struct MigrationVersionError {
    error: MigrationVersionErrorInner
}

impl From<MigrationVersionErrorInner> for MigrationVersionError {
    fn from(value: MigrationVersionErrorInner) -> Self {
        MigrationVersionError {
            error: value,
        }
    }
}

impl fmt::Display for MigrationVersionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // strip_prefix method is in str since 1.45, we support 1.34
        let strip_prefix = "dpkg: warning: ";
        match &self.error {
            MigrationVersionErrorInner::BadPrefix(string) => write!(f, "invalid migration version '{}', the version must start with '<< '", string),
            MigrationVersionErrorInner::Invalid(string) if string.starts_with(strip_prefix) => write!(f, "{}", &string[strip_prefix.len()..]),
            MigrationVersionErrorInner::Invalid(string) => write!(f, "{}", string),
        }
    }
}


#[derive(Deserialize)]
pub struct DbConfig {
    pub template: String,
}

#[derive(Deserialize)]
pub struct ExtraGroup {
    pub create: bool,
}

#[derive(Deserialize)]
pub struct TriggerCommand {
    command: Vec<String>,
    #[serde(rename = "await")]
    #[serde(default)]
    trigger_await: bool,
}

#[derive(Deserialize)]
pub struct BasePackageSpec {
    pub architecture: String,
    #[serde(default)]
    pub config: Map<String, Config>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub long_doc: Option<String>,
    #[serde(default)]
    pub databases: Map<String, DbConfig>,
    #[serde(default)]
    pub add_files: Vec<String>,
    #[serde(default)]
    pub add_dirs: Vec<String>,
    #[serde(default)]
    pub add_links: Vec<String>,
    #[serde(default)]
    pub add_manpages: Vec<String>,
    #[serde(default)]
    pub alternatives: Map<String, Alternative>,
    #[serde(default)]
    pub patch_foreign: Map<String, String>,
}

#[derive(Deserialize)]
pub struct ServicePackageSpec {
    pub bin_package: String,
    pub binary: String,
    #[serde(default)]
    pub conf_param: Option<String>,
    #[serde(default)]
    pub conf_d: Option<ConfDir>,
    pub user: UserSpec,
    #[serde(default)]
    pub config: Map<String, Config>,
    #[serde(default)]
    pub service_type: Option<String>,
    #[serde(default)]
    pub exec_stop: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub wants: Option<String>,
    #[serde(default)]
    pub requires: Option<String>,
    #[serde(default)]
    pub binds_to: Option<String>,
    #[serde(default)]
    pub part_of: Option<String>,
    #[serde(default)]
    pub wanted_by: Option<String>,
    #[serde(default)]
    pub refuse_manual_start: bool,
    #[serde(default)]
    pub refuse_manual_stop: bool,
    #[serde(default)]
    pub extra_service_config: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub long_doc: Option<String>,
    #[serde(default)]
    pub databases: Map<String, DbConfig>,
    #[serde(default)]
    pub extra_groups: Map<String, ExtraGroup>,
    #[serde(default)]
    pub add_files: Vec<String>,
    #[serde(default)]
    pub add_dirs: Vec<String>,
    #[serde(default)]
    pub add_links: Vec<String>,
    #[serde(default)]
    pub add_manpages: Vec<String>,
    #[serde(default)]
    pub alternatives: Map<String, Alternative>,
    #[serde(default)]
    pub patch_foreign: Map<String, String>,
}

pub enum BoolOrVecString {
    Bool(bool),
    VecString(Vec<String>),
}

impl Default for BoolOrVecString {
    fn default() -> Self {
        BoolOrVecString::Bool(false)
    }
}

impl<'de> serde::Deserialize<'de> for BoolOrVecString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = BoolOrVecString;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "bool or a sequence of strings")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(BoolOrVecString::Bool(v))
            }

            fn visit_seq<A>(self, mut v: A) -> Result<Self::Value, A::Error> where A: serde::de::SeqAccess<'de> {
                let mut vec = v.size_hint().map(Vec::with_capacity).unwrap_or_else(Vec::new);
                while let Some(item) = v.next_element()? {
                    vec.push(item);
                }
                Ok(BoolOrVecString::VecString(vec))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Deserialize)]
pub struct ConfExtPackageSpec {
    pub extends: String,
    #[serde(default)]
    pub replaces: BoolOrVecString,
    #[serde(default)]
    pub depends_on_extended: bool,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub long_doc: Option<String>,
    #[serde(default)]
    pub config: Map<String, Config>,
    #[serde(default)]
    pub add_files: Vec<String>,
    #[serde(default)]
    pub add_dirs: Vec<String>,
    #[serde(default)]
    pub add_links: Vec<String>,
    #[serde(default)]
    pub add_manpages: Vec<String>,
    #[serde(default)]
    pub alternatives: Map<String, Alternative>,
    #[serde(default)]
    pub patch_foreign: Map<String, String>,
}

#[derive(Deserialize)]
pub struct ConfDir {
    pub param: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct UserSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub group: bool,
    #[serde(default)]
    pub create: Option<CreateUser>,
}

#[derive(Deserialize)]
pub struct CreateUser {
    pub home: bool,
}

#[derive(Deserialize)]
pub struct Config {
    #[serde(default)]
    pub public: bool,
    #[serde(flatten)]
    pub conf_type: ConfType,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ConfType {
    Static { content: String, #[serde(default)] internal: bool, },
    Dynamic {
        format: ConfFormat,
        insert_header: Option<String>,
        #[serde(default)]
        with_header: bool,
        #[serde(default)]
        ivars: Map<String, InternalVar>,
        #[serde(default)]
        evars: Map<String, Map<String, ExternalVar>>,
        #[serde(default)]
        hvars: Map<String, HiddenVar>,
        #[serde(default)]
        fvars: Map<String, FileVar>,
        cat_dir: Option<String>,
        #[serde(default)]
        cat_files: Set<String>,
        comment: Option<String>,
        // Command to run after creating whole config file
        postprocess: Option<PostProcess>,
    },
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PostProcess {
    pub command: Vec<String>,
    #[serde(default)]
    pub generates: Vec<GeneratedFile>,
    #[serde(default)]
    pub stop_service: bool,
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GeneratedFile {
    #[serde(flatten)]
    pub ty: GeneratedType,
    pub internal: bool,
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedType {
    File(String),
    Dir(String),
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfFormat {
    Plain,
    Toml,
    Yaml,
    Json,
}

impl fmt::Display for ConfFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfFormat::Plain => write!(f, "plain"),
            ConfFormat::Toml => write!(f, "toml"),
            ConfFormat::Yaml => write!(f, "yaml"),
            ConfFormat::Json => write!(f, "json"),
        }
    }
}

#[derive(Deserialize)]
pub struct InternalVar {
    #[serde(flatten)]
    pub ty: VarType,
    pub summary: String,
    #[serde(default)]
    pub long_doc: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    pub priority: DebconfPriority,
    #[serde(default = "create_true")]
    pub store: bool,
    #[serde(default)]
    pub ignore_empty: bool,
    pub structure: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebconfPriority {
    Low,
    Medium,
    High,
    Critical,
    Dynamic { script: String },
}

#[derive(Deserialize)]
pub struct ExternalVar {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "create_true")]
    pub store: bool,
    #[serde(default)]
    pub ignore_empty: bool,
    pub structure: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct HiddenVar {
    #[serde(flatten)]
    pub ty: VarType,
    #[serde(default)]
    pub ignore_empty: bool,
    #[serde(default = "create_true")]
    pub store: bool,
    #[serde(flatten)]
    pub val: HiddenVarVal,
    pub structure: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiddenVarVal {
    Constant(String),
    Script(String),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum FileVar {
    Dir { repr: DirRepr, path: String, structure: Option<Vec<String>>, }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum DirRepr {
    Array,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum VarType {
    String,
    Uint,
    Bool,
    BindHost,
    BindPort,
    Path { file_type: Option<FileType>, create: Option<CreateFsObj>, },
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    Regular,
    Dir,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CreateFsObj {
    // TODO: use better type
    pub mode: u16,
    pub owner: String,
    pub group: String,
    #[serde(default)]
    pub only_parent: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Alternative {
    pub name: String,
    pub dest: String,
    pub priority: u32,
}

pub struct PackageInstance<'a> {
    pub name: Cow<'a, str>,
    pub variant: Option<&'a str>,
    pub spec: &'a PackageSpec,
    pub includes: Option<&'a Map<String, Package>>,
    pub depends: &'a Set<String>,
    pub provides: &'a Set<String>,
    pub recommends: &'a Set<String>,
    pub suggests: &'a Set<String>,
    pub conflicts: &'a Set<String>,
    pub extended_by: &'a Set<String>,
    pub extra_triggers: &'a Set<String>,
    pub migrations: &'a Map<MigrationVersion, Migration>,
}

impl<'a> PackageInstance<'a> {
    pub fn as_service<'b>(&'b self) -> Option<ServiceInstance<'b>> {
        if let PackageSpec::Service(service) = &self.spec {
            Some(ServiceInstance {
                name: &self.name,
                variant: self.variant,
                spec: service,
                includes: self.includes,
            })
        } else {
            None
        }
    }
}

pub struct ServiceInstance<'a> {
    pub name: &'a Cow<'a, str>,
    pub variant: Option<&'a str>,
    pub spec: &'a ServicePackageSpec,
    pub includes: Option<&'a Map<String, Package>>,
}

impl<'a> ServiceInstance<'a> {
    pub fn user_name(&self) -> &'a str {
        self.spec.user.name.as_ref().map(AsRef::as_ref).unwrap_or(&self.name.as_ref())
    }

    pub fn service_name(&self) -> &'a str {
        &**self.name
    }

    pub fn service_group(&self) -> Option<&'a str> {
        if self.spec.user.group {
            Some(self.spec.user.name.as_ref().map(AsRef::as_ref).unwrap_or(&**self.name))
        } else {
            None
        }
    }
}
