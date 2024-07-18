use std::fmt;
use serde_derive::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use crate::template::TemplateString;
use indexmap::IndexMap as OrderedHashMap;
use crate::types::{VPackageName, Variant, NonEmptyVec, VarName};
use toml::Spanned;
use crate::im_repr::Span;

use super::{Map, Set};

macro_rules! field_name {
    ($name:ident,) => { stringify!($name) };
    ($name:ident, $rename:tt) => { $rename };
}

macro_rules! serde_struct {
    ($struct_vis:vis struct $struct_name:ident { $unknown_vis:vis $unknown:ident, $span_vis:vis $span:ident $(, $(#[serde(rename = $rename:tt)])? $field_vis:vis $field_name:ident: $field_ty:ty)* $(,)? }) => {
        #[derive(Debug)]
        $struct_vis struct $struct_name {
            $($field_vis $field_name: Option<$field_ty>,)*
            $unknown_vis $unknown: Vec<Spanned<String>>,
            $span_vis $span: Span,
        }

        impl<'de> serde::de::Deserialize<'de> for $struct_name {
            fn deserialize<D: serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;

                impl<'de2> serde::de::Visitor<'de2> for Visitor {
                    type Value = Wrapper;

                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(f, "a map")
                    }

                    fn visit_map<A: serde::de::MapAccess<'de2>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                        let mut result = $struct_name {
                            $(
                                $field_name: None,
                            )*
                            $unknown: Vec::new(),
                            $span: Span { begin: 0, end: 0 },
                        };
                        while let Some(key) = map.next_key::<Spanned<String>>()? {
                            match &**key.get_ref() {
                                $(
                                    field_name!($field_name, $($rename)?) => {
                                        result.$field_name = Some(map.next_value()?);
                                    },
                                )*
                                _ => {
                                    result.$unknown.push(key);
                                    map.next_value::<serde::de::IgnoredAny>()?;
                                },
                            }
                        }
                        Ok(Wrapper(result))
                    }
                }

                struct Wrapper($struct_name);

                impl<'de2> serde::de::Deserialize<'de2> for Wrapper {
                    fn deserialize<D: serde::de::Deserializer<'de2>>(deserializer: D) -> Result<Self, D::Error> {
                        deserializer.deserialize_map(Visitor)
                    }
                }

                Spanned::<Wrapper>::deserialize(deserializer)
                    .map(|wrapper| {
                        let span = wrapper.span();
                        let mut inner = wrapper.into_inner().0;
                        inner.span = Span { begin: span.0, end: span.1 };
                        inner
                    })
            }
        }
    }
}

serde_struct! {
pub(crate) struct Plug {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) run_as_user: TemplateString,
    pub(crate) run_as_group: TemplateString,
    pub(crate) register_cmd: Vec<TemplateString>,
    pub(crate) unregister_cmd: Vec<TemplateString>,
    pub(crate) read_only_root: bool,
}
}

serde_struct! {
pub struct Package {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) name: VPackageName,
    pub(crate) map_variants: Map<String, Map<Variant, String>>,
    pub(crate) architecture: Spanned<Architecture>,
    pub(crate) bin_package: String,
    pub(crate) min_patch: String,
    pub(crate) binary: String,
    pub(crate) bare_conf_param: bool,
    pub(crate) conf_param: String,
    pub(crate) conf_d: ConfDir,
    pub(crate) user: UserSpec,
    pub(crate) config: Map<TemplateString, Config>,
    pub(crate) condition_path_exists: TemplateString,
    pub(crate) service_type: String,
    pub(crate) exec_stop: String,
    pub(crate) after: TemplateString,
    pub(crate) before: TemplateString,
    pub(crate) wants: TemplateString,
    pub(crate) requires: TemplateString,
    pub(crate) binds_to: TemplateString,
    pub(crate) part_of: TemplateString,
    pub(crate) wanted_by: TemplateString,
    pub(crate) refuse_manual_start: bool,
    pub(crate) refuse_manual_stop: bool,
    pub(crate) runtime_dir: RuntimeDir,
    pub(crate) extra_service_config: TemplateString,
    pub(crate) extends: Spanned<VPackageName>,
    pub(crate) replaces: BoolOrVecTemplateString,
    pub(crate) depends_on_extended: bool,
    pub(crate) external: bool,
    pub(crate) summary: TemplateString,
    pub(crate) long_doc: TemplateString,
    pub(crate) databases: Map<Database, DbConfig>,
    pub(crate) add_files: Vec<TemplateString>,
    pub(crate) import_files: Vec<[TemplateString; 2]>,
    pub(crate) add_dirs: Vec<TemplateString>,
    pub(crate) add_links: Vec<TemplateString>,
    pub(crate) add_manpages: Vec<String>,
    pub(crate) alternatives: Map<String, Alternative>,
    pub(crate) patch_foreign: Map<String, String>,
    pub(crate) extra_groups: Map<TemplateString, ExtraGroup>,
    pub(crate) allow_suid_sgid: bool,
    pub(crate) depends: Set<TemplateString>,
    pub(crate) provides: Set<TemplateString>,
    pub(crate) recommends: Set<TemplateString>,
    pub(crate) suggests: Set<TemplateString>,
    pub(crate) conflicts: Set<TemplateString>,
    pub(crate) extended_by: Set<TemplateString>,
    pub(crate) extra_triggers: Set<TemplateString>,
    pub(crate) migrations: Map<Spanned<String>, Migration>,
    pub(crate) plug: Vec<Plug>,
    pub(crate) custom_postrm_script: TemplateString,
}
}

pub type FileDeps<'a> = Option<&'a mut Set<PathBuf>>;

#[derive(Debug, thiserror::Error)]
enum LoadTomlErrorSource {
    #[error("Failed to read")]
    Read(#[from] std::io::Error),
    #[error("Failed to parse")]
    Parse(toml::de::Error),
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
    let spec = std::fs::read(file).map_err(LoadTomlError::with_path(file))?;
    toml::from_slice(&spec)
        .map_err(LoadTomlErrorSource::Parse)
        .map_err(LoadTomlError::with_path(file))
}

impl Package {
    pub fn load<P: AsRef<Path> + Into<PathBuf>>(file: P) -> Result<Self, LoadTomlError> {
        load_toml(file)
    }
}

serde_struct! {
pub(crate) struct Migration {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) config: TemplateString,
    pub(crate) postinst_finish: TemplateString,
}
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Database {
    Postgres,
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

impl<'de> serde::Deserialize<'de> for Database {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de2> serde::de::Visitor<'de2> for Visitor {
            type Value = Database;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
                write!(f, "a database name - pgsql or mysql")
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                match s {
                    "pgsql" => Ok(Database::Postgres),
                    "mysql" => Ok(Database::MySQL),
                    unknown => Err(E::unknown_variant(unknown, &["pgsql", "mysql"])),
                }
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

serde_struct! {
pub(crate) struct DbConfig {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) template: String,
    pub(crate) min_version: String,
    pub(crate) since: Spanned<String>,
    pub(crate) config_file_owner: String,
    pub(crate) config_file_group: String,
}
}

serde_struct! {
pub(crate) struct ExtraGroup {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) create: bool,
}
}

#[derive(Serialize, Deserialize)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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

serde_struct! {
pub(crate) struct RuntimeDir {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) mode: String,
}
}

#[derive(Debug)]
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

serde_struct! {
pub(crate) struct ConfDir {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) param: String,
    pub(crate) name: String,
}
}

serde_struct! {
pub(crate) struct UserSpec {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) name: TemplateString,
    pub(crate) group: bool,
    pub(crate) create: CreateUser,
}
}

serde_struct! {
pub(crate) struct CreateUser {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) home: bool,
}
}

serde_struct! {
pub(crate) struct Config {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) public: bool,
    pub(crate) external: bool,
    pub(crate) content: String,
    pub(crate) internal: bool,
    pub(crate) format: ConfFormat,
    pub(crate) insert_header: TemplateString,
    pub(crate) with_header: bool,
    pub(crate) ivars: OrderedHashMap<Spanned<String>, InternalVar>,
    pub(crate) evars: Map<Spanned<VPackageName>, Map<Spanned<String>, ExternalVar>>,
    pub(crate) hvars: OrderedHashMap<Spanned<String>, HiddenVar>,
    pub(crate) fvars: Map<String, FileVar>,
    pub(crate) cat_dir: String,
    pub(crate) cat_files: Set<String>,
    pub(crate) comment: String,
    // Command to run after creating whole config file
    pub(crate) postprocess: PostProcess,
}
}

serde_struct! {
pub(crate) struct PostProcess {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) command: Vec<TemplateString>,
    pub(crate) generates: Vec<GeneratedFile>,
    pub(crate) stop_service: bool,
}
}

serde_struct! {
pub(crate) struct GeneratedFile {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) file: TemplateString,
    pub(crate) dir: TemplateString,
    pub(crate) internal: bool,
}
}

#[derive(Debug)]
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

serde_struct! {
pub(crate) struct InternalVar {
    pub(crate) unknown,
    pub(crate) span,

    #[serde(rename = "type")]
    pub(crate) ty: Spanned<String>,
    pub(crate) summary: TemplateString,
    pub(crate) long_doc: TemplateString,
    pub(crate) default: Spanned<TemplateString>,
    pub(crate) try_overwrite_default: TemplateString,
    pub(crate) priority: DebconfPriority,
    pub(crate) store: bool,
    pub(crate) ignore_empty: bool,
    pub(crate) structure: Vec<String>,
    pub(crate) conditions: Vec<InternalVarCondition>,
    pub(crate) file_type: FileType,
    pub(crate) create: Spanned<CreateFsObj>,
}
}

serde_struct! {
pub(crate) struct InternalVarCondition {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) var: InternalVarConditionVar,
    pub(crate) command: InternalVarConditionCommand,
}
}

serde_struct! {
pub(crate) struct InternalVarConditionVar {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) name: Spanned<VarName<'static>>,
    pub(crate) value: TemplateString,
}
}

serde_struct! {
pub(crate) struct InternalVarConditionCommand {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) run: NonEmptyVec<TemplateString>,
    pub(crate) user: TemplateString,
    pub(crate) group: TemplateString,
    pub(crate) invert: bool,
}
}

#[derive(Debug)]
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebconfPriority {
    Low,
    Medium,
    High,
    Critical,
    Dynamic { script: String },
}

serde_struct! {
pub(crate) struct ExternalVar {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) name: String,
    pub(crate) store: bool,
    pub(crate) ignore_empty: bool,
    pub(crate) structure: Vec<String>,
}
}

serde_struct! {
pub(crate) struct HiddenVar {
    pub(crate) unknown,
    pub(crate) span,
    #[serde(rename = "type")]
    pub(crate) ty: Spanned<String>,
    pub(crate) ignore_empty: bool,
    pub(crate) store: bool,
    pub(crate) constant: String,
    pub(crate) script: TemplateString,
    pub(crate) template: Spanned<String>,
    pub(crate) structure: Vec<String>,
    pub(crate) file_type: FileType,
    pub(crate) create: Spanned<CreateFsObj>,
}
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
#[serde(rename_all = "snake_case")]
pub enum FileType {
    Regular,
    Dir,
}

serde_struct! {
pub(crate) struct CreateFsObj {
    pub(crate) unknown,
    pub(crate) span,

    // TODO: use better type
    pub(crate) mode: u16,
    pub(crate) owner: TemplateString,
    pub(crate) group: TemplateString,
    pub(crate) only_parent: bool,
}
}

serde_struct! {
pub(crate) struct Alternative {
    pub(crate) unknown,
    pub(crate) span,

    pub(crate) name: String,
    pub(crate) dest: String,
    pub(crate) priority: u32,
}
}
