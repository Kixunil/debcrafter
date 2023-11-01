use super::require_fields;

use std::convert::TryFrom;
use super::{PackageError, TemplateString, DebconfPriority, FileType, check_unknown_fields};
use crate::types::{VarName, NonEmptyVec};

pub struct InternalVar {
    pub ty: VarType,
    pub summary: TemplateString,
    pub long_doc: Option<TemplateString>,
    pub default: Option<TemplateString>,
    pub try_overwrite_default: Option<TemplateString>,
    pub priority: DebconfPriority,
    pub store: bool,
    pub ignore_empty: bool,
    pub structure: Option<Vec<String>>,
    pub conditions: Vec<InternalVarCondition>,
}

impl TryFrom<crate::input::InternalVar> for InternalVar {
    type Error = PackageError;

    fn try_from(value: crate::input::InternalVar) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, ty, summary, priority);

        let ty = match &**ty.get_ref() {
            "string" => VarType::String,
            "uint" => VarType::Uint,
            "bool" => VarType::Bool,
            "bind_host" => VarType::BindHost,
            "bind_port" => VarType::BindPort,
            "path" => VarType::Path { file_type: value.file_type, create: value.create.map(TryFrom::try_from).transpose()? },
            _ => return Err(PackageError::UnknownVarType(ty)),
        };

        let conditions = value.conditions
            .unwrap_or_default()
            .into_iter()
            .map(TryFrom::try_from)
            .collect::<Result<_, _>>()?;

        Ok(InternalVar {
            ty,
            summary,
            long_doc: value.long_doc,
            default: value.default,
            try_overwrite_default: value.try_overwrite_default,
            priority,
            store: value.store.unwrap_or(true),
            ignore_empty: value.ignore_empty.unwrap_or_default(),
            structure: value.structure,
            conditions,
        })
    }
}

#[derive(Debug)]
pub enum VarType {
    String,
    Uint,
    Bool,
    BindHost,
    BindPort,
    Path { file_type: Option<FileType>, create: Option<CreateFsObj>, },
}

pub enum InternalVarCondition {
    Var { name: VarName<'static>, value: TemplateString, },
    Command { run: NonEmptyVec<TemplateString>, user: TemplateString, group: TemplateString, invert: bool, },
}

impl TryFrom<crate::input::InternalVarCondition> for InternalVarCondition {
    type Error = PackageError;

    fn try_from(value: crate::input::InternalVarCondition) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        match (value.var, value.command) {
            (Some(var), None) => {
                check_unknown_fields(var.unknown)?;

                require_fields!(var, name, value);
                Ok(InternalVarCondition::Var {
                    name,
                    value,
                })
            },
            (None, Some(command)) => {
                check_unknown_fields(command.unknown)?;

                require_fields!(command, run, user, group);
                Ok(InternalVarCondition::Command {
                    run,
                    user,
                    group,
                    invert: command.invert.unwrap_or_default(),
                })
            },
            (None, None) => Err(PackageError::MissingFieldsOneOf(value.span, &[&["var"], &["command"]])),
            (Some(_), Some(_)) => Err(PackageError::Ambiguous(value.span, "variable condition")),
        }
    }
}

pub struct ExternalVar {
    pub name: Option<String>,
    pub store: bool,
    pub ignore_empty: bool,
    pub structure: Option<Vec<String>>,
}

impl TryFrom<crate::input::ExternalVar> for ExternalVar {
    type Error = PackageError;

    fn try_from(value: crate::input::ExternalVar) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        Ok(ExternalVar {
            name: value.name,
            store: value.store.unwrap_or(true),
            ignore_empty: value.ignore_empty.unwrap_or_default(),
            structure: value.structure,
        })
    }
}

pub struct HiddenVar {
    pub ty: VarType,
    pub ignore_empty: bool,
    pub store: bool,
    pub val: HiddenVarVal,
    pub structure: Option<Vec<String>>,
}

impl TryFrom<crate::input::HiddenVar> for HiddenVar {
    type Error = PackageError;

    fn try_from(value: crate::input::HiddenVar) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, ty);
        let ty = match &**ty.get_ref() {
            "string" => VarType::String,
            "uint" => VarType::Uint,
            "bool" => VarType::Bool,
            "bind_host" => VarType::BindHost,
            "bind_port" => VarType::BindPort,
            "path" => VarType::Path { file_type: value.file_type, create: value.create.map(TryFrom::try_from).transpose()? },
            _ => return Err(PackageError::UnknownVarType(ty)),
        };

        let val = match (value.constant, value.script, value.template) {
            (Some(constant), None, None) => HiddenVarVal::Constant(constant),
            (None, Some(script), None) => HiddenVarVal::Script(script),
            (None, None, Some(template)) => HiddenVarVal::Template(template),
            (None, None, None) => return Err(PackageError::MissingFieldsOneOf(value.span, &[&["constant"], &["script"], &["template"]])),
            _ => return Err(PackageError::Ambiguous(value.span, "value of hidden variable")),
        };

        Ok(HiddenVar {
            ty,
            ignore_empty: value.ignore_empty.unwrap_or_default(),
            store: value.store.unwrap_or(true),
            val,
            structure: value.structure,
        })
    }
}

pub enum HiddenVarVal {
    Constant(String),
    Script(TemplateString),
    Template(String),
}

#[derive(Clone, Debug)]
pub struct CreateFsObj {
    // TODO: use better type
    pub mode: u16,
    pub owner: TemplateString,
    pub group: TemplateString,
    pub only_parent: bool,
}

impl TryFrom<crate::input::CreateFsObj> for CreateFsObj {
    type Error = PackageError;


    fn try_from(value: crate::input::CreateFsObj) -> Result<Self, Self::Error> {
        check_unknown_fields(value.unknown)?;

        require_fields!(value, mode, owner, group);
        Ok(CreateFsObj {
            mode,
            owner,
            group,
            only_parent: value.only_parent.unwrap_or_default(),
        })
    }
}

