use std::borrow::Borrow;
use std::convert::TryFrom;

#[derive(Debug)]
#[derive(serde_derive::Deserialize)]
#[serde(try_from = "Vec<T>")]
pub struct NonEmptyVec<T>(Vec<T>);

#[derive(Debug, thiserror::Error)]
#[error("The vec is empty")]
#[non_exhaustive]
pub struct EmptyVecError;

impl<T> TryFrom<Vec<T>> for NonEmptyVec<T> {
    type Error = EmptyVecError;

    fn try_from(vec: Vec<T>) -> Result<Self, EmptyVecError> {
        if vec.is_empty() {
            Err(EmptyVecError)
        } else {
            Ok(NonEmptyVec(vec))
        }
    }
}

impl<T> NonEmptyVec<T> {
    pub fn split_first(&self) -> (&T, &[T]) {
        self.0.split_first().unwrap()
    }
}

impl<T> std::ops::Deref for NonEmptyVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        self.0.borrow()
    }
}
impl<T> From<NonEmptyVec<T>> for Vec<T> {
    fn from(value: NonEmptyVec<T>) -> Self {
        value.0
    }
}
