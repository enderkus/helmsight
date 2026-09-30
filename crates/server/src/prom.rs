//! Prometheus exposition of collected host metrics (`/metrics`).

use crate::app::App;
use crate::auth::ct_eq;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use collect::model::Probe;
use std::fmt::Write;
use std::sync::Arc;

fn esc(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

struct Out {
    buf: String,
    prefix: String,
    declared: std::collections::HashSet<String>,
}

impl Out {
    fn metric(&mut self, name: &str, help: &str, kind: &str, labels: &[(&str, &str)], value: f64) {
        if !value.is_finite() {
            return;
        }
        let full = format!("{}_{name}", self.prefix);
        if self.declared.insert(full.clone()) {
            let _ = writeln!(self.buf, "# HELP {full} {help}");
            let _ = writeln!(self.buf, "# TYPE {full} {kind}");
        }
        let l: Vec<String> = labels
            .iter()
            .map(|(k, v)| format!("{k}=\"{}\"", esc(v)))
            .collect();
        let _ = writeln!(self.buf, "{full}{{{}}} {value}", l.join(","));
    }
}

pub async fn metrics(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let Some(expected) = app.prometheus_token.as_ref() else {
        return (StatusCode::NOT_FOUND, "metrics endpoint is disabled").into_response();
    };
    let sent = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !ct_eq(sent, expected) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "invalid or missing bearer token",
        )
            .into_response();
    }
    let mut o = Out {
        buf: String::with_capacity(64 * 1024),
        prefix: common::PRODUCT_NAME.replace(['-', '.'], "_"),
        declared: Default::default(),
    };
    let hosts: Vec<_> = app
        .fleet
        .with(|m| m.values().cloned().collect())
        .unwrap_or_default();
    for h in &hosts {
        let host = h.cfg.name.as_str();
        let up = matches!(h.conn, crate::state::Conn::Ok);
        o.metric(
            "host_up",
            "1 if the last collection succeeded",
            "gauge",
            &[("host", host)],
            if up { 1.0 } else { 0.0 },
        );
        o.metric(
            "host_alerts_firing",
            "Firing alerts by severity",
            "gauge",
            &[("host", host), ("severity", "critical")],
            f64::from(h.alerts.critical),
        );
        o.metric(
            "host_alerts_firing",
            "Firing alerts by severity",
            "gauge",
            &[("host", host), ("severity", "warning")],
            f64::from(h.alerts.warning),
        );
        if let Some(t) = h.last_success {
            o.metric(
                "host_last_success_timestamp_seconds",
                "Time of the last successful collection",
                "gauge",
                &[("host", host)],
                t as f64,
            );
        }
        let Some(m) = h.metrics.as_ref().map(|t| &t.data) else {
            continue;
        };
        if let Some(c) = &m.cpu {
            for (mode, v) in [
                ("user", c.total.user + c.total.nice),
                ("system", c.total.system),
                ("iowait", c.total.iowait),
                ("steal", c.total.steal),
                ("irq", c.total.irq + c.total.softirq),
                ("idle", c.total.idle),
            ] {
                o.metric(
                    "cpu_percent",
                    "CPU time by mode, percent of all cores",
                    "gauge",
                    &[("host", host), ("mode", mode)],
                    v,
                );
            }
            o.metric(
                "cpu_cores",
                "Number of CPU cores",
                "gauge",
                &[("host", host)],
                f64::from(c.cores),
            );
        }
        if let Some(mem) = &m.mem {
            o.metric(
                "memory_total_bytes",
                "Total memory",
                "gauge",
                &[("host", host)],
                mem.total as f64,
            );
            o.metric(
                "memory_used_bytes",
                "Memory in use (total minus available)",
                "gauge",
                &[("host", host)],
                mem.used as f64,
            );
            o.metric(
                "memory_available_bytes",
                "Memory available",
                "gauge",
                &[("host", host)],
                mem.available as f64,
            );
            o.metric(
                "swap_used_bytes",
                "Swap in use",
                "gauge",
                &[("host", host)],
                mem.swap_used as f64,
            );
        }
        if let Some(l) = &m.load {
            o.metric(
                "load1",
                "1-minute load average",
                "gauge",
                &[("host", host)],
                l.one,
            );
            o.metric(
                "load5",
                "5-minute load average",
                "gauge",
                &[("host", host)],
                l.five,
            );
            o.metric(
                "load15",
                "15-minute load average",
                "gauge",
                &[("host", host)],
                l.fifteen,
            );
        }
        if let Some(u) = m.uptime_secs {
            o.metric(
                "uptime_seconds",
                "Seconds since boot",
                "gauge",
                &[("host", host)],
                u,
            );
        }
        for f in &m.filesystems {
            let l = [
                ("host", host),
                ("mount", f.mount.as_str()),
                ("device", f.device.as_str()),
            ];
            o.metric(
                "filesystem_size_bytes",
                "Filesystem size",
                "gauge",
                &l,
                f.total as f64,
            );
            o.metric(
                "filesystem_used_bytes",
                "Filesystem space used",
                "gauge",
                &l,
                f.used as f64,
            );
            o.metric(
                "filesystem_avail_bytes",
                "Filesystem space available",
                "gauge",
                &l,
                f.avail as f64,
            );
        }
        for d in &m.disks {
            let l = [("host", host), ("device", d.device.as_str())];
            o.metric(
                "disk_read_bytes_per_second",
                "Disk read throughput",
                "gauge",
                &l,
                d.read_bps,
            );
            o.metric(
                "disk_written_bytes_per_second",
                "Disk write throughput",
                "gauge",
                &l,
                d.write_bps,
            );
            o.metric(
                "disk_utilization_percent",
                "Time the device was busy",
                "gauge",
                &l,
                d.util_pct,
            );
        }
        for n in &m.net {
            let l = [("host", host), ("interface", n.name.as_str())];
            o.metric(
                "network_receive_bytes_per_second",
                "Bytes received per second",
                "gauge",
                &l,
                n.rx_bps,
            );
            o.metric(
                "network_transmit_bytes_per_second",
                "Bytes sent per second",
                "gauge",
                &l,
                n.tx_bps,
            );
        }
        if let Some(t) = &m.tcp {
            o.metric(
                "tcp_connections",
                "TCP connections by state",
                "gauge",
                &[("host", host), ("state", "established")],
                t.established as f64,
            );
            o.metric(
                "tcp_connections",
                "TCP connections by state",
                "gauge",
                &[("host", host), ("state", "time_wait")],
                t.time_wait as f64,
            );
        }
        if let Some(Probe::Ok { data, .. }) = h.updates.as_ref().map(|u| &u.data) {
            o.metric(
                "pending_updates",
                "Pending package updates",
                "gauge",
                &[("host", host)],
                f64::from(data.total),
            );
            if let Some(s) = data.security {
                o.metric(
                    "pending_security_updates",
                    "Pending security updates",
                    "gauge",
                    &[("host", host)],
                    f64::from(s),
                );
            }
        }
        if let Some(r) = h.inventory.as_ref().and_then(|i| i.data.reboot.required) {
            o.metric(
                "reboot_required",
                "1 if a reboot is required",
                "gauge",
                &[("host", host)],
                if r { 1.0 } else { 0.0 },
            );
        }
    }
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        o.buf,
    )
        .into_response()
}
