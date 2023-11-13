use std::fmt;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFile;

pub trait IntoDiagnostic: Sized {
    fn into_diagnostic<FileId: Copy>(self, file_id: FileId) -> Diagnostic<FileId>;
}

pub trait Report {
    fn report<Name: fmt::Display + Clone, Source: AsRef<str>>(self, name: Name, source: Source) -> !;
}

impl<T: IntoDiagnostic> Report for T {
    fn report<Name: fmt::Display + Clone, Source: AsRef<str>>(self, name: Name, source: Source) -> ! {
        use codespan_reporting::term::termcolor::{StandardStream, ColorChoice};

        let diagnostic = self.into_diagnostic(());
        let file = SimpleFile::new(name, source);
        let mut out = StandardStream::stderr(ColorChoice::Auto);
        codespan_reporting::term::emit(&mut out, &Default::default(), &file, &diagnostic).unwrap();
        std::process::exit(1);
    }
}

impl<T: IntoDiagnostic> Report for Vec<T> {
    fn report<Name: fmt::Display + Clone, Source: AsRef<str>>(self, name: Name, source: Source) -> ! {
        use codespan_reporting::term::termcolor::{StandardStream, ColorChoice};

        let file = SimpleFile::new(name, source);
        let mut out = StandardStream::stderr(ColorChoice::Auto);
        for item in self {
            let diagnostic = item.into_diagnostic(());
            codespan_reporting::term::emit(&mut out, &Default::default(), &file, &diagnostic).unwrap();
        }
        std::process::exit(1);
    }
}

impl IntoDiagnostic for crate::im_repr::PackageError {
    fn into_diagnostic<FileId: Copy>(self, file_id: FileId) -> Diagnostic<FileId> {
        use crate::im_repr::{PackageError, MigrationVersionErrorInner};

        match self {
            PackageError::Ambiguous(span, what) => {
                Diagnostic::error()
                    .with_message(format!("ambiguous {}", what))
                    .with_labels(vec![Label::primary(file_id, span).with_message(format!("this {} is ambiguous", what))])
            },
            PackageError::MissingFields(span, fields) => {
                let (missing_fields_text, is_are) = if fields.len() == 1 {
                    ("missing field", "is")
                } else {
                    ("missing fields", "are")
                };
                Diagnostic::error()
                    .with_message(missing_fields_text)
                    .with_labels(vec![Label::primary(file_id, span).with_message(format!("{} {} missing", fields.join(", "), is_are))])
            },
            PackageError::MissingFieldsOneOf(span, fields) => {
                let missing_fields_text = if fields.len() == 1 {
                    "missing field"
                } else {
                    "missing fields"
                };
                let mut label = format!("Either ");
                let mut is_first_group = true;
                for group in fields {
                    if !is_first_group {
                        label.push_str(" or ");
                    } else {
                        is_first_group = false;
                    }
                    let mut is_first_field = true;
                    for field in *group {
                        if !is_first_field {
                            label.push_str(", ");
                        } else {
                            is_first_field = false;
                        }
                        label.push_str(field);
                    }
                    label.push_str(" must be present");
                }
                Diagnostic::error()
                    .with_message(missing_fields_text)
                    .with_labels(vec![Label::primary(file_id, span).with_message(label)])
            },
            PackageError::UnknownFields(fields) => {
                let unknown_fields_text = if fields.len() == 1 {
                    "unknown field"
                } else {
                    "unknown fields"
                };
                let labels = fields.iter().map(|field| {
                    let span = field.span();
                    Label::primary(file_id, span.0..span.1)
                        .with_message("unknown field")
                })
                .collect();
                Diagnostic::error()
                    .with_message(format!("{} {}", unknown_fields_text, fields.join(", ")))
                    .with_labels(labels)
            },
            PackageError::UnknownVarType(unknown) => {
                let span = unknown.span();
                Diagnostic::error()
                    .with_message(format!("unknown variable type {}", unknown.get_ref()))
                    .with_labels(vec![Label::primary(file_id, span.0..span.1).with_message("unknown type")])
            },
            PackageError::Migration(migration) => {
                match migration.error {
                    MigrationVersionErrorInner::BadPrefix(_version) => {
                        Diagnostic::error()
                            .with_message("invalid migration version prefix")
                            .with_labels(vec![Label::primary(file_id, migration.span).with_message("must start with <<")])
                    },
                    MigrationVersionErrorInner::Invalid(message) => {
                        Diagnostic::error()
                            .with_message("invalid migration version")
                            .with_labels(vec![Label::primary(file_id, migration.span).with_message(message)])
                    },
                }
            },
            PackageError::InvalidPackageName(error) => {
                let mut invalid_chars = error.value.invalid_chars
                    .iter()
                    .map(|char_pos| *char_pos + error.span_start).peekable();
                let mut last_start;
                let mut last_end;
                let mut labels = Vec::new();
                while invalid_chars.peek().is_some() {
                    last_start = invalid_chars.next().expect("non-empty");
                    last_end = last_start;
                    while invalid_chars.peek() == Some(&(last_end + 1)) {
                        last_end += 1;
                        invalid_chars.next();
                    }
                    let label = Label::primary(file_id, last_start..(last_end + 1));
                    let label = match (labels.is_empty(), last_end - last_start) {
                        (true, 0) => label.with_message("This char is invalid"),
                        (true, _) => label.with_message("These chars are invalid"),
                        (false, 0) => label.with_message("... and this char"),
                        (false, _) => label.with_message("... and these chars"),
                    };
                    labels.push(label);
                }
                if error.value.invalid_chars.len() > 1 {
                    Diagnostic::error()
                        .with_message("invalid characters in package name")
                        .with_labels(labels)
                } else {
                    Diagnostic::error()
                        .with_message("invalid character in package name")
                        .with_labels(labels)
                }
            },
            PackageError::CreatePathWithoutType(range) => {
                Diagnostic::error()
                    .with_message("Cannot create a path without knowing the file type")
                    .with_labels(vec![Label::primary(file_id, range).with_message("Path creation requested here.")])
            },
            PackageError::EVarNotFound(var) => {
                Diagnostic::error()
                    .with_message(format!("Variable {} not found", var.value))
                    .with_labels(vec![Label::primary(file_id, var.span_range()).with_message("This variable was not imported")])
            },
            PackageError::EVarNotInPackage(package, var) => {
                Diagnostic::error()
                    .with_message(format!("No variable {} in package {}", var, package.as_raw()))
                    .with_labels(vec![
                                 Label::secondary(file_id, package.span_range()).with_message("This package"),
                                 Label::primary(file_id, var.span_range()).with_message("doesn't contain this variable"),
                    ])
            },
            PackageError::IVarNotFound(var, later) => {
                let mut labels = vec![Label::primary(file_id, var.span_range()).with_message("This internal variable was not defined")];
                if let Some(later) = later {
                    labels.push(Label::secondary(file_id, later).with_message("The variable is defined here which is after being used."));
                }
                Diagnostic::error()
                    .with_message(format!("Internal variable {} not found", var.value))
                    .with_labels(labels)
            },
            PackageError::VarNotFound(var, later) => {
                let mut labels = vec![Label::primary(file_id, var.span_range()).with_message("This variable was not defined")];
                if let Some(later) = later {
                    labels.push(Label::secondary(file_id, later).with_message("The variable is defined here which is after being used."));
                }
                Diagnostic::error()
                    .with_message(format!("Variable {} not found", var.value))
                    .with_labels(labels)
            },
            PackageError::ConstantNotFound(var) => {
                Diagnostic::error()
                    .with_message(format!("Constant {} not found", var.value))
                    .with_labels(vec![Label::primary(file_id, var.span_range()).with_message("This constant was not defined")])
            },
            PackageError::UntemplatedBindPort(var, default) => {
                let label = match default {
                    Some(default) => Label::primary(file_id, default).with_message("This has no template variables."),
                    None => Label::primary(file_id, var.span_range()).with_message("This variable doesn't have a templated default.")
                };
                Diagnostic::error()
                    .with_message(format!("Bind port of variable {} is not templated", var.value))
                    .with_labels(vec![label])
            },
            PackageError::ConstCond(range) => {
                Diagnostic::error()
                    .with_message("Using a constant to skip a variable would be useless")
                    .with_labels(vec![Label::primary(file_id, range).with_message("This constant is not useful")])
            },
            PackageError::PackageNotFound(package) => {
                Diagnostic::error()
                    .with_message(format!("Package {} not found", package.as_raw()))
                    .with_labels(vec![Label::primary(file_id, package.span_range()).with_message("This package doesn't exist")])
            },
        }
    }
}
