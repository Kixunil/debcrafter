use std::fmt;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFile;

pub trait IntoDiagnostic: Sized {
    fn into_diagnostic<FileId: Copy>(self, file_id: FileId) -> Diagnostic<FileId>;
    fn report<Name: fmt::Display + Clone, Source: AsRef<str>>(self, name: Name, source: Source) -> ! {
        use codespan_reporting::term::termcolor::{StandardStream, ColorChoice};

        let diagnostic = self.into_diagnostic(());
        let file = SimpleFile::new(name, source);
        let mut out = StandardStream::stderr(ColorChoice::Auto);
        codespan_reporting::term::emit(&mut out, &Default::default(), &file, &diagnostic).unwrap();
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
        }
    }
}
