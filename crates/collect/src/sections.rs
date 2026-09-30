//! Splitting raw script output into named sections.

use std::collections::BTreeMap;

/// Output of one script execution, split by section marker.
#[derive(Debug, Default, Clone)]
pub struct Sections {
    map: BTreeMap<String, String>,
    /// The `begin` marker was seen.
    pub began: bool,
    /// The `end` marker was seen, so the script ran to completion.
    pub complete: bool,
}

impl Sections {
    /// Splits `raw` on lines equal to `"<marker> <name>"`. Content before the
    /// first marker (for example shell start-up noise) is ignored. Duplicate
    /// section names keep the first occurrence.
    pub fn split(raw: &str, marker: &str) -> Self {
        let mut out = Sections::default();
        let mut current: Option<String> = None;
        let mut buf = String::new();
        let prefix = format!("{marker} ");
        for line in raw.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if let Some(name) = line.strip_prefix(&prefix)
                && is_section_name(name)
            {
                if let Some(prev) = current.take() {
                    out.insert(prev, std::mem::take(&mut buf));
                }
                buf.clear();
                match name {
                    "begin" => out.began = true,
                    "end" => out.complete = true,
                    _ => {}
                }
                current = Some(name.to_string());
                continue;
            }
            if current.is_some() {
                buf.push_str(line);
                buf.push('\n');
            }
        }
        if let Some(prev) = current.take() {
            out.insert(prev, buf);
        }
        out
    }

    fn insert(&mut self, name: String, mut content: String) {
        // Each marker is preceded by an empty line; drop the trailing blank.
        while content.ends_with('\n') {
            content.pop();
        }
        self.map.entry(name).or_insert(content);
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }

    pub fn has(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

fn is_section_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_sections() {
        let raw = "motd noise\n\n@@HS-x@@ begin\nv=1\n\n@@HS-x@@ stat\ncpu 1 2 3\n\n@@HS-x@@ end\n";
        let s = Sections::split(raw, "@@HS-x@@");
        assert!(s.began && s.complete);
        assert_eq!(s.get("begin"), Some("v=1"));
        assert_eq!(s.get("stat"), Some("cpu 1 2 3"));
        assert_eq!(s.get("missing"), None);
    }

    #[test]
    fn ignores_foreign_markers_and_duplicates() {
        let raw = "@@HS-x@@ stat\na\n@@HS-y@@ meminfo\nb\n@@HS-x@@ stat\nc\n@@HS-x@@ Bad Name\n";
        let s = Sections::split(raw, "@@HS-x@@");
        assert_eq!(s.get("stat"), Some("a\n@@HS-y@@ meminfo\nb"));
        assert!(!s.complete);
    }

    #[test]
    fn truncated_output_is_incomplete() {
        let raw = "@@HS-x@@ begin\n\n@@HS-x@@ stat\ncpu 1";
        let s = Sections::split(raw, "@@HS-x@@");
        assert!(s.began && !s.complete);
        assert_eq!(s.get("stat"), Some("cpu 1"));
    }
}
