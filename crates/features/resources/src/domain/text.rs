//! Shared immutable strings for row storage.
//!
//! Namespaces, statuses, owner kinds and many cell texts repeat across
//! thousands of rows. [`Text`] is a reference counted `str` and
//! [`Interner`] makes equal short strings share one allocation, so a table of
//! 10k rows holds a handful of copies of `Running` instead of 10k.

use std::collections::HashSet;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

/// A cheaply clonable immutable string.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Text(Arc<str>);

impl Text {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The shared allocation, for zero-copy conversion to UI string types.
    pub fn as_arc(&self) -> &Arc<str> {
        &self.0
    }

    /// Whether both values share one allocation.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for Text {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Text {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Self(Arc::from(value))
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}

impl From<&String> for Text {
    fn from(value: &String) -> Self {
        Self(Arc::from(value.as_str()))
    }
}

impl From<Arc<str>> for Text {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

impl PartialEq<str> for Text {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

impl PartialEq<String> for Text {
    fn eq(&self, other: &String) -> bool {
        &*self.0 == other.as_str()
    }
}

impl PartialEq<Text> for &str {
    fn eq(&self, other: &Text) -> bool {
        *self == &*other.0
    }
}

impl PartialEq<Text> for String {
    fn eq(&self, other: &Text) -> bool {
        self.as_str() == &*other.0
    }
}

/// Longest string worth sharing. Longer texts (messages, label lists) are
/// rarely repeated and would only pin memory.
pub const INTERN_MAX_LEN: usize = 48;
/// Most distinct strings an [`Interner`] keeps. When full it starts over:
/// rows already holding a shared string keep it alive on their own.
pub const INTERN_MAX_ENTRIES: usize = 4096;

/// A bounded pool of shared strings.
#[derive(Debug, Default)]
pub struct Interner {
    pool: HashSet<Text>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of strings currently pooled.
    pub fn len(&self) -> usize {
        self.pool.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pool.is_empty()
    }

    /// Returns the pooled copy of `text`, adding it when new. Strings longer
    /// than [`INTERN_MAX_LEN`] are returned as they are, unpooled.
    pub fn intern(&mut self, text: &Text) -> Text {
        if text.len() > INTERN_MAX_LEN {
            return text.clone();
        }
        if let Some(shared) = self.pool.get(text.as_str()) {
            return shared.clone();
        }
        if self.pool.len() >= INTERN_MAX_ENTRIES {
            self.pool.clear();
        }
        self.pool.insert(text.clone());
        text.clone()
    }

    /// Interns a borrowed string, allocating only when it is not pooled yet.
    pub fn intern_str(&mut self, text: &str) -> Text {
        if let Some(shared) = self.pool.get(text) {
            return shared.clone();
        }
        let owned = Text::from(text);
        if text.len() > INTERN_MAX_LEN {
            return owned;
        }
        if self.pool.len() >= INTERN_MAX_ENTRIES {
            self.pool.clear();
        }
        self.pool.insert(owned.clone());
        owned
    }
}

impl std::borrow::Borrow<str> for Text {
    fn borrow(&self) -> &str {
        &self.0
    }
}
