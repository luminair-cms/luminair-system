use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A validated, pre-compiled regular expression pattern value object.
/// Equality and hashing are based on the pattern string.
#[derive(Clone)]
pub struct RegexPattern {
    raw: String,
    compiled: Arc<regex::Regex>,
}

impl RegexPattern {
    /// Creates a new `RegexPattern` by compiling the pattern string.
    pub fn try_new(raw: impl Into<String>) -> Result<Self, regex::Error> {
        let raw = raw.into();
        let compiled = Arc::new(regex::Regex::new(&raw)?);
        Ok(Self { raw, compiled })
    }

    /// Checks if the given text matches the regular expression.
    pub fn is_match(&self, text: &str) -> bool {
        self.compiled.is_match(text)
    }

    /// Returns the raw pattern string.
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Debug for RegexPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RegexPattern({:?})", self.raw)
    }
}

impl fmt::Display for RegexPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl AsRef<str> for RegexPattern {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

impl std::ops::Deref for RegexPattern {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl PartialEq for RegexPattern {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl Eq for RegexPattern {}

impl Hash for RegexPattern {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
    }
}

impl Serialize for RegexPattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RegexPattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        RegexPattern::try_new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_regex_pattern() {
        let pattern = RegexPattern::try_new(r"^[a-z]+$").expect("valid regex");
        assert!(pattern.is_match("hello"));
        assert!(!pattern.is_match("Hello"));
        assert!(!pattern.is_match("123"));
        assert_eq!(pattern.as_str(), r"^[a-z]+$");
        assert_eq!(format!("{pattern}"), r"^[a-z]+$");
    }

    #[test]
    fn test_invalid_regex_pattern() {
        let res = RegexPattern::try_new(r"[a-z");
        assert!(res.is_err());
    }

    #[test]
    fn test_equality_and_hash() {
        use std::collections::HashSet;

        let p1 = RegexPattern::try_new(r"^[a-z]+$").unwrap();
        let p2 = RegexPattern::try_new(r"^[a-z]+$").unwrap();
        let p3 = RegexPattern::try_new(r"^[0-9]+$").unwrap();

        assert_eq!(p1, p2);
        assert_ne!(p1, p3);

        let mut set = HashSet::new();
        set.insert(p1);
        assert!(set.contains(&p2));
        assert!(!set.contains(&p3));
    }

    #[test]
    fn test_serde_roundtrip() {
        let pattern = RegexPattern::try_new(r"^[a-z]+$").unwrap();
        let json = serde_json::to_string(&pattern).unwrap();
        assert_eq!(json, r#""^[a-z]+$""#);

        let deserialized: RegexPattern = serde_json::from_str(&json).unwrap();
        assert_eq!(pattern, deserialized);
        assert!(deserialized.is_match("test"));
    }
}
