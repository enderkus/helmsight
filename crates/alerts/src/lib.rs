//! Alert rules and notifications.
//!
//! [`samples`] turns collected data into rule inputs, [`Evaluator`] decides
//! which conditions are firing (honouring `for` durations and the built-in
//! rules), and [`notify`] delivers notifications.

pub mod notify;
mod samples;

pub use samples::{Sample, samples};

use common::config::{AlertsConfig, Severity};
use common::rules::{Expr, metric};
use common::selector::{Selectable, Selector};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Built-in rule ids.
pub const HOST_UNREACHABLE: &str = "host_unreachable";
pub const HOST_KEY_CHANGED: &str = "host_key_changed";
pub const HOST_KEY_UNKNOWN: &str = "host_key_unknown";
pub const FAILED_UNITS: &str = "failed_units";
pub const CERT_EXPIRY: &str = "cert_expiry";

/// Connection state of a host, as far as alerting is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reachability {
    /// No collection attempted yet.
    Pending,
    Ok,
    Unreachable {
        since: i64,
        message: String,
    },
    AuthFailed {
        since: i64,
        message: String,
    },
    HostKeyUnknown {
        fingerprint: String,
    },
    HostKeyChanged {
        fingerprint: String,
    },
}

/// Everything the evaluator needs to know about one host.
#[derive(Debug, Clone)]
pub struct HostInput {
    pub name: String,
    pub groups: Vec<String>,
    pub tags: Vec<String>,
    pub reachability: Reachability,
    pub samples: Vec<Sample>,
    pub failed_units: Vec<String>,
}

impl Selectable for HostInput {
    fn name(&self) -> &str {
        &self.name
    }
    fn groups(&self) -> &[String] {
        &self.groups
    }
    fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// Result of a TLS certificate check.
#[derive(Debug, Clone)]
pub struct CertInput {
    pub endpoint: String,
    pub days_left: Option<f64>,
    pub error: Option<String>,
}

/// A condition that is currently firing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Firing {
    pub fingerprint: String,
    pub rule_id: String,
    pub host: Option<String>,
    pub instance: Option<String>,
    pub severity: Severity,
    pub summary: String,
    pub value: Option<f64>,
}

#[derive(Debug, Clone)]
struct Rule {
    id: String,
    expr: Expr,
    severity: Severity,
    summary: Option<String>,
    scope: Selector,
}

/// Stateful rule evaluator. Call [`Evaluator::evaluate`] periodically with
/// the latest data for all hosts.
#[derive(Debug)]
pub struct Evaluator {
    rules: Vec<Rule>,
    cfg: AlertsConfig,
    /// fingerprint -> first time the condition was seen true
    pending: HashMap<String, i64>,
    /// Last firing set, reused for hosts whose data is currently unavailable.
    last: HashMap<String, Firing>,
}

fn fingerprint(rule: &str, host: Option<&str>, instance: Option<&str>) -> String {
    format!("{rule}|{}|{}", host.unwrap_or(""), instance.unwrap_or(""))
}

/// Formats a value with its metric unit for summaries.
pub fn format_value(metric_name: &str, v: f64) -> String {
    let unit = metric(metric_name).map(|m| m.unit).unwrap_or("");
    match unit {
        "%" => format!("{v:.1}%"),
        "B/s" => format!("{}/s", human_bytes(v)),
        "s" => format!("{v:.0}s"),
        _ if v.fract() == 0.0 => format!("{v:.0}"),
        _ => format!("{v:.2}"),
    }
}

fn human_bytes(v: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = v;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < units.len() {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", units.get(i).unwrap_or(&"B"))
}

impl Evaluator {
    /// Builds an evaluator from the alert configuration. Rules have been
    /// validated by the config loader; invalid ones are skipped.
    pub fn new(cfg: &AlertsConfig) -> Self {
        let rules = cfg
            .rules
            .iter()
            .filter_map(|r| {
                Some(Rule {
                    id: r.id.clone(),
                    expr: Expr::parse(&r.expr).ok()?,
                    severity: r.severity,
                    summary: r.summary.clone(),
                    scope: r.scope.clone(),
                })
            })
            .collect();
        Self {
            rules,
            cfg: cfg.clone(),
            pending: HashMap::new(),
            last: HashMap::new(),
        }
    }

    /// True when the condition behind `fingerprint` held during the last
    /// evaluation, even if its `for` duration has not elapsed yet.
    pub fn is_active(&self, fingerprint: &str) -> bool {
        self.pending.contains_key(fingerprint) || self.last.contains_key(fingerprint)
    }

    /// Returns all conditions that are firing now.
    pub fn evaluate(&mut self, now: i64, hosts: &[HostInput], certs: &[CertInput]) -> Vec<Firing> {
        let mut out: Vec<Firing> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for host in hosts {
            let name = Some(host.name.as_str());
            match &host.reachability {
                Reachability::Unreachable { since, message } | Reachability::AuthFailed { since, message } => {
                    let auth = matches!(host.reachability, Reachability::AuthFailed { .. });
                    let down_for = now - since;
                    if down_for >= i64::try_from(self.cfg.unreachable_after.as_secs()).unwrap_or(i64::MAX) {
                        out.push(Firing {
                            fingerprint: fingerprint(HOST_UNREACHABLE, name, None),
                            rule_id: HOST_UNREACHABLE.into(),
                            host: Some(host.name.clone()),
                            instance: None,
                            severity: Severity::Critical,
                            summary: if auth {
                                format!("SSH authentication failed: {message}")
                            } else {
                                format!("Host unreachable: {message}")
                            },
                            value: Some(down_for as f64),
                        });
                    }
                }
                Reachability::HostKeyChanged { fingerprint: fp } => out.push(Firing {
                    fingerprint: fingerprint(HOST_KEY_CHANGED, name, None),
                    rule_id: HOST_KEY_CHANGED.into(),
                    host: Some(host.name.clone()),
                    instance: None,
                    severity: Severity::Critical,
                    summary: format!(
                        "SSH host key changed (now {fp}); collection stopped until an admin reviews it"
                    ),
                    value: None,
                }),
                Reachability::HostKeyUnknown { fingerprint: fp } => out.push(Firing {
                    fingerprint: fingerprint(HOST_KEY_UNKNOWN, name, None),
                    rule_id: HOST_KEY_UNKNOWN.into(),
                    host: Some(host.name.clone()),
                    instance: None,
                    severity: Severity::Warning,
                    summary: format!("SSH host key {fp} is not trusted yet; approve it to start collection"),
                    value: None,
                }),
                Reachability::Ok | Reachability::Pending => {}
            }

            if host.reachability != Reachability::Ok {
                // Keep metric-based alerts as they were while data is missing.
                for f in self.last.values() {
                    if f.host.as_deref() == name && is_data_rule(&f.rule_id) {
                        seen.insert(f.fingerprint.clone());
                        out.push(f.clone());
                    }
                }
                continue;
            }

            if self.cfg.failed_units {
                for unit in &host.failed_units {
                    out.push(Firing {
                        fingerprint: fingerprint(FAILED_UNITS, name, Some(unit)),
                        rule_id: FAILED_UNITS.into(),
                        host: Some(host.name.clone()),
                        instance: Some(unit.clone()),
                        severity: Severity::Warning,
                        summary: format!("Service {unit} has failed"),
                        value: Some(1.0),
                    });
                }
            }

            for rule in &self.rules {
                if !rule.scope.matches_or_all(host) {
                    continue;
                }
                for s in host.samples.iter().filter(|s| s.metric == rule.expr.metric) {
                    let fp = fingerprint(&rule.id, name, s.instance.as_deref());
                    if !rule.expr.op.eval(s.value, rule.expr.threshold) {
                        continue;
                    }
                    seen.insert(fp.clone());
                    let since = *self.pending.entry(fp.clone()).or_insert(now);
                    let needed = i64::try_from(rule.expr.duration.as_secs()).unwrap_or(i64::MAX);
                    if now - since < needed {
                        continue;
                    }
                    let what = match &s.instance {
                        Some(i) => format!("{} on {i}", rule.expr.metric),
                        None => rule.expr.metric.clone(),
                    };
                    let auto = format!(
                        "{what} is {} ({} {})",
                        format_value(&rule.expr.metric, s.value),
                        rule.expr.op.as_str(),
                        format_value(&rule.expr.metric, rule.expr.threshold)
                    );
                    out.push(Firing {
                        fingerprint: fp,
                        rule_id: rule.id.clone(),
                        host: Some(host.name.clone()),
                        instance: s.instance.clone(),
                        severity: rule.severity,
                        summary: match &rule.summary {
                            Some(text) => format!("{text}: {auto}"),
                            None => auto,
                        },
                        value: Some(s.value),
                    });
                }
            }
        }

        for c in certs {
            let fp = fingerprint(CERT_EXPIRY, None, Some(&c.endpoint));
            let firing = |severity, summary: String, value| Firing {
                fingerprint: fp.clone(),
                rule_id: CERT_EXPIRY.into(),
                host: None,
                instance: Some(c.endpoint.clone()),
                severity,
                summary,
                value,
            };
            if let Some(e) = &c.error {
                out.push(firing(
                    Severity::Warning,
                    format!("TLS check of {} failed: {e}", c.endpoint),
                    None,
                ));
            } else if let Some(days) = c.days_left {
                let sev = if days < f64::from(self.cfg.cert_critical_days) {
                    Some(Severity::Critical)
                } else if days < f64::from(self.cfg.cert_warning_days) {
                    Some(Severity::Warning)
                } else {
                    None
                };
                if let Some(sev) = sev {
                    let text = if days < 0.0 {
                        format!(
                            "Certificate for {} expired {:.0} days ago",
                            c.endpoint, -days
                        )
                    } else {
                        format!(
                            "Certificate for {} expires in {:.0} days",
                            c.endpoint,
                            days.floor()
                        )
                    };
                    out.push(firing(sev, text, Some(days)));
                }
            }
        }

        self.pending.retain(|fp, _| seen.contains(fp));
        self.last = out
            .iter()
            .map(|f| (f.fingerprint.clone(), f.clone()))
            .collect();
        out
    }
}

fn is_data_rule(rule_id: &str) -> bool {
    ![
        HOST_UNREACHABLE,
        HOST_KEY_CHANGED,
        HOST_KEY_UNKNOWN,
        CERT_EXPIRY,
    ]
    .contains(&rule_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::config::RuleConfig;
    use common::duration::Dur;

    fn cfg(rules: Vec<RuleConfig>) -> AlertsConfig {
        AlertsConfig {
            rules,
            ..AlertsConfig::default()
        }
    }

    fn rule(id: &str, expr: &str) -> RuleConfig {
        RuleConfig {
            id: id.into(),
            expr: expr.into(),
            severity: Severity::Critical,
            summary: None,
            scope: Selector::default(),
        }
    }

    fn host(disk: f64) -> HostInput {
        HostInput {
            name: "web-1".into(),
            groups: vec!["web".into()],
            tags: vec!["env:prod".into()],
            reachability: Reachability::Ok,
            samples: vec![Sample {
                metric: "disk_used_pct",
                instance: Some("/".into()),
                value: disk,
            }],
            failed_units: vec![],
        }
    }

    #[test]
    fn for_duration_is_honoured() {
        let mut e = Evaluator::new(&cfg(vec![rule("disk", "disk_used_pct > 90 for 10m")]));
        assert!(e.evaluate(0, &[host(95.0)], &[]).is_empty());
        assert!(e.evaluate(599, &[host(95.0)], &[]).is_empty());
        let f = e.evaluate(600, &[host(95.0)], &[]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].fingerprint, "disk|web-1|/");
        assert_eq!(f[0].summary, "disk_used_pct on / is 95.0% (> 90.0%)");
        // Condition clears, timer restarts.
        assert!(e.evaluate(601, &[host(50.0)], &[]).is_empty());
        assert!(e.evaluate(602, &[host(95.0)], &[]).is_empty());
    }

    #[test]
    fn scope_limits_hosts() {
        let mut r = rule("disk", "disk_used_pct > 90");
        r.scope.tags = vec!["env:staging".into()];
        let mut e = Evaluator::new(&cfg(vec![r]));
        assert!(e.evaluate(0, &[host(99.0)], &[]).is_empty());
    }

    #[test]
    fn unreachable_keeps_data_alerts_and_fires_after_delay() {
        let mut e = Evaluator::new(&cfg(vec![rule("disk", "disk_used_pct > 90")]));
        assert_eq!(e.evaluate(0, &[host(95.0)], &[]).len(), 1);
        let mut down = host(0.0);
        down.samples.clear();
        down.reachability = Reachability::Unreachable {
            since: 10,
            message: "timeout".into(),
        };
        let f = e.evaluate(20, &[down.clone()], &[]);
        assert_eq!(f.len(), 1, "disk alert kept, unreachable not yet");
        let f = e.evaluate(10 + 120, &[down], &[]);
        assert_eq!(f.len(), 2);
        assert!(
            f.iter()
                .any(|x| x.rule_id == HOST_UNREACHABLE && x.severity == Severity::Critical)
        );
    }

    #[test]
    fn builtins() {
        let mut c = cfg(vec![]);
        c.unreachable_after = Dur::secs(0);
        let mut e = Evaluator::new(&c);
        let mut h = host(0.0);
        h.failed_units = vec!["nginx.service".into()];
        let f = e.evaluate(0, &[h.clone()], &[]);
        assert_eq!(f[0].rule_id, FAILED_UNITS);
        h.reachability = Reachability::HostKeyChanged {
            fingerprint: "SHA256:x".into(),
        };
        assert_eq!(e.evaluate(1, &[h], &[])[0].rule_id, HOST_KEY_CHANGED);
        let certs = [
            CertInput {
                endpoint: "a:443".into(),
                days_left: Some(3.0),
                error: None,
            },
            CertInput {
                endpoint: "b:443".into(),
                days_left: Some(15.0),
                error: None,
            },
            CertInput {
                endpoint: "c:443".into(),
                days_left: Some(90.0),
                error: None,
            },
            CertInput {
                endpoint: "d:443".into(),
                days_left: None,
                error: Some("refused".into()),
            },
        ];
        let f = e.evaluate(2, &[], &certs);
        assert_eq!(f.len(), 3);
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[1].severity, Severity::Warning);
        assert!(f[2].summary.contains("refused"));
    }
}
