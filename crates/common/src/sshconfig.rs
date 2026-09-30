//! Importing host definitions from an OpenSSH client config file.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SshHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}

#[derive(Debug, Default)]
struct Block {
    patterns: Vec<String>,
    options: Vec<(String, String)>,
}

/// Parses an ssh config and returns every concrete (non-wildcard) host
/// alias with its effective options. Like OpenSSH, the first value found
/// for an option wins. `Match` blocks and `Include` are not evaluated.
pub fn parse(text: &str) -> (Vec<SshHost>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut blocks = vec![Block {
        patterns: vec!["*".into()],
        options: Vec::new(),
    }];
    let mut in_match = false;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (
                k.to_ascii_lowercase(),
                v.trim_start_matches([' ', '\t', '=']).trim(),
            ),
            None => (line.to_ascii_lowercase(), ""),
        };
        let value = value.trim_matches('"').to_string();
        match key.as_str() {
            "host" => {
                in_match = false;
                blocks.push(Block {
                    patterns: value.split_whitespace().map(str::to_string).collect(),
                    options: Vec::new(),
                });
            }
            "match" => {
                in_match = true;
                warnings.push(format!("line {}: `Match` blocks are ignored", n + 1));
            }
            "include" => warnings.push(format!("line {}: `Include` is not followed", n + 1)),
            _ if in_match => {}
            _ => {
                if let Some(b) = blocks.last_mut() {
                    b.options.push((key, value));
                }
            }
        }
    }
    let mut aliases: Vec<String> = Vec::new();
    for b in &blocks {
        for p in &b.patterns {
            if !p.contains(['*', '?', '!']) && !aliases.contains(p) {
                aliases.push(p.clone());
            }
        }
    }
    let hosts = aliases
        .into_iter()
        .map(|alias| {
            let mut h = SshHost {
                alias: alias.clone(),
                ..SshHost::default()
            };
            for b in blocks.iter().filter(|b| block_matches(&b.patterns, &alias)) {
                for (k, v) in &b.options {
                    match k.as_str() {
                        "hostname" if h.hostname.is_none() => h.hostname = Some(v.clone()),
                        "user" if h.user.is_none() => h.user = Some(v.clone()),
                        "port" if h.port.is_none() => h.port = v.parse().ok(),
                        "identityfile" if h.identity_file.is_none() => {
                            h.identity_file = Some(v.clone())
                        }
                        "proxyjump" if h.proxy_jump.is_none() => h.proxy_jump = Some(v.clone()),
                        _ => {}
                    }
                }
            }
            h
        })
        .collect();
    (hosts, warnings)
}

fn block_matches(patterns: &[String], alias: &str) -> bool {
    let mut matched = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if glob(neg, alias) {
                return false;
            }
        } else if glob(p, alias) {
            matched = true;
        }
    }
    matched
}

/// Glob matching with `*` and `?`.
pub fn glob(pattern: &str, s: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = s.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        match p.get(pi) {
            Some('?') => {
                pi += 1;
                ti += 1;
            }
            Some('*') => {
                star = Some(pi);
                mark = ti;
                pi += 1;
            }
            Some(c) if Some(c) == t.get(ti) => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                Some(s) => {
                    pi = s + 1;
                    mark += 1;
                    ti = mark;
                }
                None => return false,
            },
        }
    }
    while p.get(pi) == Some(&'*') {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks_with_first_value_wins() {
        let (hosts, warnings) = parse(
            "# comment\nHost web-1 web-2\n  HostName 10.0.0.1\n  User deploy\n\
             Host web-2\n  HostName 10.0.0.2\n  Port 2222\n\
             Host db\n  HostName=db.internal\n  ProxyJump bastion\n\
             Host * !db\n  User monitor\n  IdentityFile ~/.ssh/id_ed25519\n\
             Match host x\n  User nope\nInclude other\n",
        );
        assert_eq!(warnings.len(), 2);
        assert_eq!(hosts.len(), 3);
        let w2 = hosts.iter().find(|h| h.alias == "web-2").unwrap();
        assert_eq!(w2.hostname.as_deref(), Some("10.0.0.1"));
        assert_eq!(w2.user.as_deref(), Some("deploy"));
        assert_eq!(w2.port, Some(2222));
        let db = hosts.iter().find(|h| h.alias == "db").unwrap();
        assert_eq!(db.user, None);
        assert_eq!(db.proxy_jump.as_deref(), Some("bastion"));
    }

    #[test]
    fn globbing() {
        assert!(glob("web-*", "web-12"));
        assert!(glob("w?b", "web"));
        assert!(!glob("web-*", "db-1"));
        assert!(glob("*", ""));
        assert!(glob("a*b*c", "axxbyyc"));
    }
}
