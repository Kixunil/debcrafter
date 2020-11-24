use std::borrow::Borrow;
use crate::Map;

pub use package_name::VPackageName;

mod package_name;

pub struct NonEmptyMap<K, V, M: Borrow<Map<K, V>>>(M, std::marker::PhantomData<(K, V)>);

#[derive(Debug, thiserror::Error)]
#[error("The map is empty")]
#[non_exhaustive]
pub struct EmptyMapError;

impl<K, V, M> NonEmptyMap<K, V, M> where M: Borrow<Map<K, V>> {
    pub fn from_map(map: M) -> Option<Self> {
        if map.borrow().is_empty() {
            None
        } else {
            Some(NonEmptyMap(map, Default::default()))
        }
    }
}

impl<K, V, M> std::ops::Deref for NonEmptyMap<K, V, M> where M: Borrow<Map<K, V>> {
    type Target = Map<K, V>;

    fn deref(&self) -> &Self::Target {
        self.0.borrow()
    }
}
