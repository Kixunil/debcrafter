// BS lints
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]


pub mod postinst;
pub mod template;
pub mod types;
pub mod input;
pub mod im_repr;
pub mod error_report;

pub type Map<K, V> = std::collections::BTreeMap<K, V>;
pub type Set<T> = std::collections::BTreeSet<T>;
