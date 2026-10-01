//! Alert rule expressions such as `disk_used_pct > 90 for 10m`.

use crate::duration::Dur;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A metric that alert rules can reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricDef {
    pub name: &'static str,
    pub unit: &'static str,
    pub description: &'static str,
    /// Label identifying the instance for per-instance metrics (mount,
    /// device, interface ...), or `None` for host-level metrics.
    pub instance: Option<&'static str>,
}

const fn m(
    name: &'static str,
    unit: &'static str,
    instance: Option<&'static str>,
    description: &'static str,
) -> MetricDef {
    MetricDef {
        name,
        unit,
        description,
        instance,
    }
}

/// All metrics available to alert rules.
pub const METRICS: &[MetricDef] = &[
    m("cpu_pct", "%", None, "CPU busy time, all cores"),
    m("cpu_iowait_pct", "%", None, "CPU time waiting for I/O"),
    m(
        "cpu_steal_pct",
        "%",
        None,
        "CPU time stolen by the hypervisor",
    ),
    m(
        "mem_used_pct",
        "%",
        None,
        "Memory in use (excluding reclaimable cache)",
    ),
    m("swap_used_pct", "%", None, "Swap in use"),
    m("load1", "", None, "1-minute load average"),
    m("load5", "", None, "5-minute load average"),
    m("load15", "", None, "15-minute load average"),
    m(
        "load1_per_core",
        "",
        None,
        "1-minute load average divided by core count",
    ),
    m("disk_used_pct", "%", Some("mount"), "Filesystem space used"),
    m(
        "disk_inodes_used_pct",
        "%",
        Some("mount"),
        "Filesystem inodes used",
    ),
    m(
        "disk_util_pct",
        "%",
        Some("device"),
        "Block device utilisation",
    ),
    m(
        "net_rx_bytes",
        "B/s",
        Some("interface"),
        "Bytes received per second",
    ),
    m(
        "net_tx_bytes",
        "B/s",
        Some("interface"),
        "Bytes sent per second",
    ),
    m(
        "net_errors",
        "1/s",
        Some("interface"),
        "Receive and transmit errors per second",
    ),
    m("tcp_established", "", None, "Established TCP connections"),
    m("process_count", "", None, "Number of processes"),
    m(
        "failed_units",
        "",
        None,
        "Failed systemd units or crashed OpenRC services",
    ),
    m("pending_updates", "", None, "Pending package updates"),
    m(
        "pending_security_updates",
        "",
        None,
        "Pending security updates",
    ),
    m(
        "reboot_required",
        "",
        None,
        "1 when a reboot is required, else 0",
    ),
    m(
        "ssh_failed_logins_1h",
        "",
        None,
        "Failed SSH logins in the last hour",
    ),
    m("uptime_secs", "s", None, "Seconds since boot"),
    m(
        "clock_skew_secs",
        "s",
        None,
        "Absolute difference between host and server clocks",
    ),
    m(
        "containers_not_running",
        "",
        None,
        "Containers that exist but are not running",
    ),
];

pub fn metric(name: &str) -> Option<&'static MetricDef> {
    METRICS.iter().find(|m| m.name == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
}

impl Op {
    pub fn eval(self, value: f64, threshold: f64) -> bool {
        match self {
            Op::Gt => value > threshold,
            Op::Ge => value >= threshold,
            Op::Lt => value < threshold,
            Op::Le => value <= threshold,
            Op::Eq => (value - threshold).abs() < f64::EPSILON,
            Op::Ne => (value - threshold).abs() >= f64::EPSILON,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Eq => "==",
            Op::Ne => "!=",
        }
    }
}

/// A parsed rule expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub metric: String,
    pub op: Op,
    pub threshold: f64,
    /// How long the condition must hold before the alert fires.
    pub duration: Dur,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.metric, self.op.as_str(), self.threshold)?;
        if self.duration.0.as_secs() > 0 {
            write!(f, " for {}", self.duration)?;
        }
        Ok(())
    }
}

impl Expr {
    /// Parses `<metric> <op> <number>[K|M|G|Ki|Mi|Gi] [for <duration>]`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        let (metric, op, value, rest) = match toks.as_slice() {
            [metric, op, value, rest @ ..] => (*metric, *op, *value, rest),
            _ => {
                return Err(format!(
                    "expected `<metric> <operator> <threshold> [for <duration>]`, got `{s}`"
                ));
            }
        };
        if self::metric(metric).is_none() {
            let names: Vec<&str> = METRICS.iter().map(|m| m.name).collect();
            return Err(format!(
                "unknown metric `{metric}`; available: {}",
                names.join(", ")
            ));
        }
        let op = match op {
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "<" => Op::Lt,
            "<=" => Op::Le,
            "==" => Op::Eq,
            "!=" => Op::Ne,
            other => return Err(format!("unknown operator `{other}` (use > >= < <= == !=)")),
        };
        let threshold = parse_number(value)?;
        let duration = match rest {
            [] => Dur::default(),
            ["for", d] => Dur::parse(d)?,
            _ => return Err(format!("unexpected `{}` after threshold", rest.join(" "))),
        };
        Ok(Expr {
            metric: metric.to_string(),
            op,
            threshold,
            duration,
        })
    }
}

fn parse_number(s: &str) -> Result<f64, String> {
    let split = s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len());
    let (num, suffix) = s.split_at(split);
    let n: f64 = num
        .parse()
        .map_err(|_| format!("invalid threshold `{s}`"))?;
    let mult = match suffix {
        "" => 1.0,
        "K" | "k" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "Ki" => 1024.0,
        "Mi" => 1024.0 * 1024.0,
        "Gi" => 1024.0 * 1024.0 * 1024.0,
        "Ti" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        other => return Err(format!("unknown threshold suffix `{other}`")),
    };
    let v = n * mult;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("invalid threshold `{s}`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expressions() {
        let e = Expr::parse("disk_used_pct > 90 for 10m").unwrap();
        assert_eq!(e.metric, "disk_used_pct");
        assert_eq!(e.op, Op::Gt);
        assert_eq!(e.threshold, 90.0);
        assert_eq!(e.duration, Dur::secs(600));
        assert_eq!(e.to_string(), "disk_used_pct > 90 for 10m");
        let e = Expr::parse("net_rx_bytes >= 100Mi").unwrap();
        assert_eq!(e.threshold, 104_857_600.0);
        assert_eq!(e.duration, Dur::default());
    }

    #[test]
    fn every_metric_is_documented() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/configuration.md");
        let doc = std::fs::read_to_string(path).unwrap();
        for m in METRICS {
            assert!(
                doc.contains(&format!("`{}`", m.name)),
                "{} missing from docs/configuration.md",
                m.name
            );
        }
    }

    #[test]
    fn rejects_bad_expressions() {
        assert!(
            Expr::parse("disk_used > 90")
                .unwrap_err()
                .contains("unknown metric")
        );
        assert!(Expr::parse("cpu_pct => 90").is_err());
        assert!(Expr::parse("cpu_pct > ninety").is_err());
        assert!(Expr::parse("cpu_pct > 90 during 5m").is_err());
        assert!(Expr::parse("cpu_pct >").is_err());
    }
}
