//! Selecting hosts by name, group or tag.

use serde::{Deserialize, Serialize};

/// Anything that can be matched by a [`Selector`].
pub trait Selectable {
    fn name(&self) -> &str;
    fn groups(&self) -> &[String];
    fn tags(&self) -> &[String];
}

/// Matches hosts listed by name, or belonging to any listed group, or
/// carrying any listed tag.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Selector {
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty() && self.groups.is_empty() && self.tags.is_empty()
    }

    /// True when the host matches. An empty selector matches nothing; use
    /// [`Selector::matches_or_all`] where "empty means everything" applies.
    pub fn matches<H: Selectable + ?Sized>(&self, host: &H) -> bool {
        self.hosts.iter().any(|h| h == host.name())
            || self.groups.iter().any(|g| host.groups().contains(g))
            || self.tags.iter().any(|t| host.tags().contains(t))
    }

    pub fn matches_or_all<H: Selectable + ?Sized>(&self, host: &H) -> bool {
        self.is_empty() || self.matches(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct H(&'static str, Vec<String>, Vec<String>);
    impl Selectable for H {
        fn name(&self) -> &str {
            self.0
        }
        fn groups(&self) -> &[String] {
            &self.1
        }
        fn tags(&self) -> &[String] {
            &self.2
        }
    }

    #[test]
    fn matching() {
        let h = H("web-1", vec!["web".into()], vec!["env:prod".into()]);
        let empty = Selector::default();
        assert!(!empty.matches(&h));
        assert!(empty.matches_or_all(&h));
        let by_tag = Selector {
            tags: vec!["env:prod".into()],
            ..Default::default()
        };
        assert!(by_tag.matches(&h));
        let other = Selector {
            hosts: vec!["db-1".into()],
            groups: vec!["db".into()],
            ..Default::default()
        };
        assert!(!other.matches(&h));
    }
}
