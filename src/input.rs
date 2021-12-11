use std::fmt;
use serde_derive::Deserialize;
use std::path::{Path, PathBuf};
use std::convert::TryFrom;
use crate::template::TemplateString;
use linked_hash_map::LinkedHashMap;
use crate::types::{VPackageName, Variant, NonEmptyVec, VarName};

use super::{Map, Set};

fn create_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct Plug {
    pub run_as_user: TemplateString,
    #[serde(default)]
    pub run_as_group: Option<TemplateString>,
    pub register_cmd: Vec<TemplateString>,
    pub unregister_cmd: Vec<TemplateString>,
}

#[derive(Deserialize)]
pub struct Package {
    pub name: VPackageName,
    #[serde(default)]
    pub map_variants: Map<String, Map<Variant, String>>,
    #[serde(flatten)]
    pub spec: PackageSpec,
    #[serde(default)]
    pub depends: Set<TemplateString>,
    #[serde(default)]
    pub provides: Set<TemplateString>,
    #[serde(default)]
    pub recommends: Set<TemplateString>,
    #[serde(default)]
    pub suggests: Set<TemplateString>,
    #[serde(default)]
    pub conflicts: Set<TemplateString>,
    #[serde(default)]
    pub extended_by: Set<TemplateString>,
    #[serde(default)]
    pub extra_triggers: Set<TemplateString>,
    #[serde(default)]
    pub migrations: Map<MigrationVersion, Migration>,
    #[serde(default)]
    pub plug: Vec<Plug>,
}

pub type FileDeps<'a> = Option<&'a mut Set<PathBuf>>;

fn load_include<'a>(dir: &'a Path, name: &'a VPackageName, mut deps: FileDeps<'a>) -> impl 'a + FnMut() -> Package {
    move || {
        let file = name.sps_path(dir);
        let package = Package::load(&file).expect("Failed to load include");
        deps.as_mut().map(|deps| deps.insert(file));
        package
    }
}

#[derive(Debug, thiserror::Error)]
enum LoadTomlErrorSource {
    #[error("Failed to read")]
    Read(#[from] std::io::Error),
    #[error("Failed to parse")]
    Parse(#[from] toml::de::Error)
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to load Toml file {path}")]
pub struct LoadTomlError {
    path: PathBuf,
    inner: LoadTomlErrorSource,
}

impl LoadTomlError {
    fn with_path<E: Into<LoadTomlErrorSource>, P: Into<PathBuf>>(path: P) -> impl FnOnce(E) -> Self {
        |error| LoadTomlError {
            path: path.into(),
            inner: error.into(),
        }
    }
}

pub fn load_toml<T: for<'a> serde::Deserialize<'a>, P: AsRef<Path> + Into<PathBuf>>(file: P) -> Result<T, LoadTomlError> {
    let file = file.as_ref();
    let spec = std::fs::read(&file).map_err(LoadTomlError::with_path(file))?;
    toml::from_slice(&spec).map_err(LoadTomlError::with_path(file))
}

impl Package {
    pub fn load<P: AsRef<Path> + Into<PathBuf>>(file: P) -> Result<Self, LoadTomlError> {
        load_toml(file)
    }

    pub fn load_includes<P: AsRef<Path>>(&self, dir: P, mut deps: Option<&mut Set<PathBuf>>) -> Map<VPackageName, Package> {
        let mut result = Map::new();
        let config = match &self.spec {
            PackageSpec::Service(spec) => &spec.config,
            PackageSpec::ConfExt(spec) => &spec.config,
            PackageSpec::Base(spec) => &spec.config,
        };
        for (_, conf) in config {
            if let ConfType::Dynamic { evars, .. } = &conf.conf_type {
                for (pkg, _) in evars {
                    let deps = deps.as_mut().map(|deps| &mut **deps);
                    result.entry(pkg.to_owned()).or_insert_with(load_include(dir.as_ref(), pkg, deps));
                }
            }
        }

        if let PackageSpec::ConfExt(ConfExtPackageSpec { extends, external: false, .. }) = &self.spec {
            result.entry(extends.clone()).or_insert_with(load_include(dir.as_ref(), &extends, deps));
        }

        result
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
    pub fn summary(&self) -> &Option<TemplateString> {
        match self {
            PackageSpec::Base(base) => &base.summary,
            PackageSpec::Service(service) => &service.summary,
            PackageSpec::ConfExt(confext) => &confext.summary,
        }
    }

    pub fn long_doc(&self) -> &Option<TemplateString> {
        match self {
            PackageSpec::Base(base) => &base.long_doc,
            PackageSpec::Service(service) => &service.long_doc,
            PackageSpec::ConfExt(confext) => &confext.long_doc,
        }
    }
}

#[derive(Deserialize)]
pub struct Migration {
    pub config: Option<TemplateString>,
    pub postinst_finish: Option<TemplateString>,
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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize)]
pub enum Database {
    #[serde(rename = "pgsql")]
    Postgres,
    #[serde(rename = "mysql")]
    MySQL,
}

impl Database {
    pub fn dependency(&self) -> &'static str {
        match self {
            Database::Postgres => "postgresql",
            Database::MySQL => "default-mysql-server",
        }
    }

    pub fn dbconfig_dependency(&self) -> &'static str {
        match self {
            Database::Postgres => "pgsql",
            Database::MySQL => "mysql",
        }
    }

    pub fn lib_name(&self) -> &'static str {
        match self {
            Database::Postgres => "pgsql",
            Database::MySQL => "mysql",
        }
    }

    pub fn dbconfig_db_type(&self) -> &'static str {
        match self {
            Database::Postgres => "pgsql",
            Database::MySQL => "mysql",
        }
    }
}

#[derive(Deserialize)]
pub struct DbConfig {
    pub template: String,
    #[serde(default)]
    pub config_file_owner: Option<String>,
    #[serde(default)]
    pub config_file_group: Option<String>,
}

#[derive(Deserialize)]
pub struct ExtraGroup {
    pub create: bool,
}

#[derive(Deserialize)]
pub enum Architecture {
    #[serde(rename = "any")]
    Any,
    #[serde(rename = "all")]
    All,
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Architecture::Any => write!(f, "any"),
            Architecture::All => write!(f, "all"),
        }
    }
}


#[derive(Deserialize)]
pub struct BasePackageSpec {
    pub architecture: Architecture,
    #[serde(default)]
    pub config: Map<TemplateString, Config>,
    #[serde(default)]
    pub summary: Option<TemplateString>,
    #[serde(default)]
    pub long_doc: Option<TemplateString>,
    #[serde(default)]
    pub databases: Map<Database, DbConfig>,
    #[serde(default)]
    pub add_files: Vec<TemplateString>,
    #[serde(default)]
    pub add_dirs: Vec<TemplateString>,
    #[serde(default)]
    pub add_links: Vec<TemplateString>,
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
    pub min_patch: Option<String>,
    pub binary: String,
    #[serde(default)]
    pub bare_conf_param: bool,
    #[serde(default)]
    pub conf_param: Option<String>,
    #[serde(default)]
    pub conf_d: Option<ConfDir>,
    pub user: UserSpec,
    #[serde(default)]
    pub config: Map<TemplateString, Config>,
    #[serde(default)]
    pub condition_path_exists: Option<TemplateString>,
    #[serde(default)]
    pub service_type: Option<String>,
    #[serde(default)]
    pub exec_stop: Option<String>,
    #[serde(default)]
    pub after: Option<TemplateString>,
    #[serde(default)]
    pub before: Option<TemplateString>,
    #[serde(default)]
    pub wants: Option<TemplateString>,
    #[serde(default)]
    pub requires: Option<TemplateString>,
    #[serde(default)]
    pub binds_to: Option<TemplateString>,
    #[serde(default)]
    pub part_of: Option<TemplateString>,
    #[serde(default)]
    pub wanted_by: Option<TemplateString>,
    #[serde(default)]
    pub refuse_manual_start: bool,
    #[serde(default)]
    pub refuse_manual_stop: bool,
    #[serde(default)]
    pub runtime_dir: Option<RuntimeDir>,
    #[serde(default)]
    pub extra_service_config: Option<TemplateString>,
    #[serde(default)]
    pub summary: Option<TemplateString>,
    #[serde(default)]
    pub long_doc: Option<TemplateString>,
    #[serde(default)]
    pub databases: Map<Database, DbConfig>,
    #[serde(default)]
    pub extra_groups: Map<TemplateString, ExtraGroup>,
    #[serde(default)]
    pub add_files: Vec<TemplateString>,
    #[serde(default)]
    pub add_dirs: Vec<TemplateString>,
    #[serde(default)]
    pub add_links: Vec<TemplateString>,
    #[serde(default)]
    pub add_manpages: Vec<String>,
    #[serde(default)]
    pub alternatives: Map<String, Alternative>,
    #[serde(default)]
    pub patch_foreign: Map<String, String>,
    #[serde(default)]
    pub allow_suid_sgid: bool,
}

#[derive(Deserialize)]
pub struct RuntimeDir {
    pub mode: String,
}

pub enum BoolOrVecTemplateString {
    Bool(bool),
    VecString(Vec<TemplateString>),
}

impl Default for BoolOrVecTemplateString {
    fn default() -> Self {
        BoolOrVecTemplateString::Bool(false)
    }
}

impl<'de> serde::Deserialize<'de> for BoolOrVecTemplateString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = BoolOrVecTemplateString;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "bool or a sequence of strings")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(BoolOrVecTemplateString::Bool(v))
            }

            fn visit_seq<A>(self, mut v: A) -> Result<Self::Value, A::Error> where A: serde::de::SeqAccess<'de> {
                let mut vec = v.size_hint().map(Vec::with_capacity).unwrap_or_else(Vec::new);
                while let Some(item) = v.next_element()? {
                    vec.push(item);
                }
                Ok(BoolOrVecTemplateString::VecString(vec))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Deserialize)]
pub struct ConfExtPackageSpec {
    pub extends: VPackageName,
    #[serde(default)]
    pub replaces: BoolOrVecTemplateString,
    #[serde(default)]
    pub depends_on_extended: bool,
    pub min_patch: Option<String>,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub summary: Option<TemplateString>,
    #[serde(default)]
    pub long_doc: Option<TemplateString>,
    #[serde(default)]
    pub databases: Map<Database, DbConfig>,
    #[serde(default)]
    pub config: Map<TemplateString, Config>,
    #[serde(default)]
    pub add_files: Vec<TemplateString>,
    #[serde(default)]
    pub add_dirs: Vec<TemplateString>,
    #[serde(default)]
    pub add_links: Vec<TemplateString>,
    #[serde(default)]
    pub add_manpages: Vec<String>,
    #[serde(default)]
    pub alternatives: Map<String, Alternative>,
    #[serde(default)]
    pub patch_foreign: Map<String, String>,
    #[serde(default)]
    pub extra_groups: Map<TemplateString, ExtraGroup>,
}

#[derive(Deserialize)]
pub struct ConfDir {
    pub param: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct UserSpec {
    #[serde(default)]
    pub name: Option<TemplateString>,
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
    #[serde(default)]
    pub external: bool,
    #[serde(flatten)]
    pub conf_type: ConfType,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ConfType {
    Static { content: String, #[serde(default)] internal: bool, },
    Dynamic {
        format: ConfFormat,
        insert_header: Option<TemplateString>,
        #[serde(default)]
        with_header: bool,
        #[serde(default)]
        ivars: LinkedHashMap<String, InternalVar>,
        #[serde(default)]
        evars: Map<VPackageName, Map<String, ExternalVar>>,
        #[serde(default)]
        hvars: LinkedHashMap<String, HiddenVar>,
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
    pub command: Vec<TemplateString>,
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
    File(TemplateString),
    Dir(TemplateString),
}

#[derive(Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfFormat {
    Plain,
    Toml,
    Yaml,
    Json,
    SpaceSeparated,
}

impl fmt::Display for ConfFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfFormat::Plain => write!(f, "plain"),
            ConfFormat::Toml => write!(f, "toml"),
            ConfFormat::Yaml => write!(f, "yaml"),
            ConfFormat::Json => write!(f, "json"),
            ConfFormat::SpaceSeparated => write!(f, "space_separated"),
        }
    }
}

#[derive(Deserialize)]
pub struct InternalVar {
    #[serde(flatten)]
    pub ty: VarType,
    pub summary: TemplateString,
    #[serde(default)]
    pub long_doc: Option<TemplateString>,
    #[serde(default)]
    pub default: Option<TemplateString>,
    #[serde(default)]
    pub try_overwrite_default: Option<TemplateString>,
    pub priority: DebconfPriority,
    #[serde(default = "create_true")]
    pub store: bool,
    #[serde(default)]
    pub ignore_empty: bool,
    #[serde(default)]
    pub structure: Option<Vec<String>>,
    #[serde(default)]
    pub conditions: Vec<InternalVarCondition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternalVarCondition {
    Var { name: VarName<'static>, value: TemplateString, },
    Command { run: NonEmptyVec<TemplateString>, user: TemplateString, group: TemplateString, #[serde(default)] invert: bool, },
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
    Script(TemplateString),
    Template(String),
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
    pub owner: TemplateString,
    pub group: TemplateString,
    #[serde(default)]
    pub only_parent: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Alternative {
    pub name: String,
    pub dest: String,
    pub priority: u32,
}
