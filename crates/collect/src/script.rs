//! Rendering of the remote collection script.

use serde::{Deserialize, Serialize};

const TEMPLATE: &str = include_str!("remote.sh");

/// Groups of sections that can be requested in one execution. Groups run at
/// different intervals; the server decides which ones are due on each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Metrics,
    Basics,
    Medium,
    Inventory,
    Auth,
    Updates,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Metrics,
        Group::Basics,
        Group::Medium,
        Group::Inventory,
        Group::Auth,
        Group::Updates,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Group::Metrics => "metrics",
            Group::Basics => "basics",
            Group::Medium => "medium",
            Group::Inventory => "inventory",
            Group::Auth => "auth",
            Group::Updates => "updates",
        }
    }
}

/// Optional behaviour of the script.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptOptions {
    /// Run dnf/yum to list pending updates. Off by default because dnf
    /// always writes its own log files on the host.
    pub dnf_updates: bool,
}

/// Parameters for one execution of the script.
#[derive(Debug, Clone)]
pub struct ScriptRequest {
    /// Random hex nonce embedded in section markers.
    pub nonce: String,
    pub groups: Vec<Group>,
    /// Only report authentication failures after this unix timestamp.
    pub auth_since: i64,
    pub options: ScriptOptions,
}

impl ScriptRequest {
    /// Creates a request with a fresh random nonce.
    pub fn new(groups: Vec<Group>, auth_since: i64, random: [u8; 16]) -> Self {
        let nonce = random.iter().map(|b| format!("{b:02x}")).collect();
        Self {
            nonce,
            groups,
            auth_since,
            options: ScriptOptions::default(),
        }
    }

    /// Renders the script text. Only server-generated values are inserted:
    /// the nonce is validated to be hexadecimal, groups come from a fixed
    /// enum and the timestamp is an integer.
    pub fn render(&self) -> String {
        let nonce: String = self
            .nonce
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        let groups = self
            .groups
            .iter()
            .map(|g| g.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let opts = if self.options.dnf_updates { "dnf" } else { "" };
        TEMPLATE
            .replace("__NONCE__", &nonce)
            .replace("__GROUPS__", &groups)
            .replace("__SINCE__", &self.auth_since.max(0).to_string())
            .replace("__OPTS__", opts)
    }

    pub fn marker(&self) -> String {
        format!("@@HS-{}@@", self.nonce)
    }
}

/// The raw template, for documentation and review purposes.
pub fn template() -> &'static str {
    TEMPLATE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_all_placeholders() {
        let req = ScriptRequest::new(vec![Group::Metrics, Group::Auth], 1234, [0xab; 16]);
        let s = req.render();
        assert!(!s.contains("__NONCE__"));
        assert!(!s.contains("__GROUPS__"));
        assert!(!s.contains("__SINCE__"));
        assert!(s.contains("@@HS-abababababababababababababababab@@"));
        assert!(s.contains("HS_GROUPS=' metrics auth '"));
        assert!(s.contains("SINCE='1234'"));
    }

    #[test]
    fn render_rejects_non_hex_nonce_characters() {
        let req = ScriptRequest {
            nonce: "ab'; rm -rf /; '".into(),
            groups: vec![],
            auth_since: -5,
            options: ScriptOptions { dnf_updates: true },
        };
        let s = req.render();
        assert!(s.contains("M='@@HS-abf@@'"));
        assert!(s.contains("SINCE='0'"));
        assert!(s.contains("HS_OPTS=' dnf '"));
        assert!(!s.contains("__OPTS__"));
    }

    #[test]
    fn script_never_writes_files() {
        // Every output redirection must target /dev/null or another fd.
        for line in TEMPLATE.lines() {
            let l = line.trim();
            if l.starts_with('#') {
                continue;
            }
            let unquoted = strip_single_quoted(l);
            let mut rest = unquoted.as_str();
            while let Some(i) = rest.find('>') {
                let after = rest[i + 1..].trim_start_matches('>').trim_start();
                assert!(
                    after.starts_with("/dev/null") || after.starts_with('&'),
                    "suspicious redirection in remote script: {line}"
                );
                rest = &rest[i + 1..];
            }
        }
    }

    fn strip_single_quoted(s: &str) -> String {
        let mut out = String::new();
        let mut quoted = false;
        for c in s.chars() {
            if c == '\'' {
                quoted = !quoted;
            } else if !quoted {
                out.push(c);
            }
        }
        out
    }
}
