use crate::types::VPackageName;

pub mod postinst;
pub mod template;
pub mod types;
pub mod input;

pub type Map<K, V> = std::collections::BTreeMap<K, V>;
pub type Set<T> = std::collections::BTreeSet<T>;

pub use input::*;
