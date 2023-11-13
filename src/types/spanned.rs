use std::fmt;

#[derive(Debug, Copy, Clone)]
pub struct Spanned<T> {
    pub value: T,
    pub span_start: usize,
    pub span_end: usize,
}

impl<T> Spanned<T> {
    pub fn span_range(&self) -> core::ops::Range<usize> {
        self.span_start..self.span_end
    }
}

impl<T> From<toml::Spanned<T>> for Spanned<T> {
    fn from(value: toml::Spanned<T>) -> Self {
        let (span_start, span_end) = value.span();
        Spanned {
            value: value.into_inner(),
            span_start,
            span_end,
        }
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::borrow::Borrow<T> for Spanned<T> {
    fn borrow(&self) -> &T {
        &self.value
    }
}

impl std::borrow::Borrow<str> for Spanned<String> {
    fn borrow(&self) -> &str {
        &self.value
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

impl<T: PartialOrd> PartialOrd for Spanned<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Ord> Ord for Spanned<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(other)
    }
}

impl<T: std::hash::Hash> std::hash::Hash for Spanned<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: fmt::Display> fmt::Display for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}
