//! In-memory fleet state, updated by the collection engine and read by the
//! API. Also defines the summaries sent to the UI.

use collect::model::{Basics, Inventory, Medium, Metrics, Probe, UpdateReport};
use common::config::HostConfig;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::sync::RwLock;
use transport::HostKeyInfo;
use utoipa::ToSchema;

/// Value with the unix time it was collected.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Timed<T> {
    pub ts: i64,
    pub data: T,
}

/// Connection state of a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conn {
    Pending,
    Disabled,
    Ok,
    Failed {
        kind: &'static str,
        message: String,
        since: i64,
        key: Option<HostKeyInfo>,
    },
}

impl Conn {
    pub fn as_str(&self) -> &'static str {
        match self {
            Conn::Pending => "pending",
            Conn::Disabled => "disabled",
            Conn::Ok => "ok",
            Conn::Failed { kind, .. } => kind,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct AlertCounts {
    pub critical: u32,
    pub warning: u32,
    pub info: u32,
}

/// Where failed-login data comes from, or why it is unavailable.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthSource {
    pub source: Option<String>,
    pub unavailable: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct SparkPoint {
    pub ts: i64,
    pub cpu: Option<f32>,
    pub mem: Option<f32>,
}

const SPARK_POINTS: usize = 180;

#[derive(Debug, Clone)]
pub struct HostState {
    pub cfg: HostConfig,
    pub id: i64,
    pub conn: Conn,
    pub last_attempt: Option<i64>,
    pub last_success: Option<i64>,
    pub last_duration_ms: Option<u64>,
    /// Sections missing from the last successful collection.
    pub missing: Vec<String>,
    pub truncated: bool,
    pub metrics: Option<Timed<Metrics>>,
    pub medium: Option<Timed<Medium>>,
    pub inventory: Option<Timed<Inventory>>,
    pub updates: Option<Timed<Probe<UpdateReport>>>,
    pub auth: Option<Timed<AuthSource>>,
    pub basics: Option<Basics>,
    pub clock_skew: Option<f64>,
    pub spark: VecDeque<SparkPoint>,
    pub alerts: AlertCounts,
    pub failed_logins_24h: Option<u64>,
    pub failed_logins_1h: Option<u64>,
}

impl HostState {
    pub fn new(cfg: HostConfig, id: i64) -> Self {
        let conn = if cfg.disabled {
            Conn::Disabled
        } else {
            Conn::Pending
        };
        Self {
            cfg,
            id,
            conn,
            last_attempt: None,
            last_success: None,
            last_duration_ms: None,
            missing: Vec::new(),
            truncated: false,
            metrics: None,
            medium: None,
            inventory: None,
            updates: None,
            auth: None,
            basics: None,
            clock_skew: None,
            spark: VecDeque::with_capacity(SPARK_POINTS),
            alerts: AlertCounts::default(),
            failed_logins_24h: None,
            failed_logins_1h: None,
        }
    }

    pub fn push_spark(&mut self, p: SparkPoint) {
        if self.spark.len() >= SPARK_POINTS {
            self.spark.pop_front();
        }
        self.spark.push_back(p);
    }

    pub fn failed_units(&self) -> Vec<String> {
        match self.medium.as_ref().map(|m| &m.data.services) {
            Some(Probe::Ok { data, .. }) => data
                .iter()
                .filter(|s| s.is_failed())
                .map(|s| s.unit.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Summary for the fleet overview. `stale_after` is in seconds.
    pub fn summary(&self, now: i64, stale_after: i64, with_spark: bool) -> HostSummary {
        let m = self.metrics.as_ref().map(|t| &t.data);
        let cpu = m.and_then(|m| m.cpu.as_ref());
        let mem = m.and_then(|m| m.mem.as_ref());
        let load = m.and_then(|m| m.load.as_ref());
        let worst = m.and_then(|m| {
            m.filesystems
                .iter()
                .max_by(|a, b| a.used_pct.total_cmp(&b.used_pct))
                .map(|f| DiskWorst {
                    mount: f.mount.clone(),
                    used_pct: f.used_pct,
                    avail: f.avail,
                })
        });
        let (rx, tx) = m
            .map(|m| {
                m.net
                    .iter()
                    .fold((0.0, 0.0), |(r, t), n| (r + n.rx_bps, t + n.tx_bps))
            })
            .unwrap_or((0.0, 0.0));
        let inv = self.inventory.as_ref().map(|t| &t.data);
        let updates = self.updates.as_ref().and_then(|t| t.data.data());
        let failed_units = match self.medium.as_ref().map(|m| &m.data.services) {
            Some(Probe::Ok { data, .. }) => {
                Some(u32::try_from(data.iter().filter(|s| s.is_failed()).count()).unwrap_or(0))
            }
            _ => None,
        };
        let health = if self.alerts.critical > 0 {
            "critical"
        } else if self.alerts.warning > 0 {
            "warning"
        } else {
            "ok"
        };
        let status = match &self.conn {
            Conn::Ok => health,
            other => other.as_str(),
        };
        let (error, key) = match &self.conn {
            Conn::Failed { message, key, .. } => (Some(message.clone()), key.clone()),
            _ => (None, None),
        };
        HostSummary {
            name: self.cfg.name.clone(),
            address: self.cfg.address().to_string(),
            port: self.cfg.port(),
            groups: self.cfg.groups.clone(),
            tags: self.cfg.tags.clone(),
            baseline: self.cfg.baseline.clone(),
            status: status.to_string(),
            connection: self.conn.as_str().to_string(),
            health: health.to_string(),
            error,
            host_key: key,
            partial: !self.missing.is_empty() || self.truncated,
            missing: self.missing.clone(),
            last_attempt: self.last_attempt,
            last_success: self.last_success,
            stale: self.last_success.is_some_and(|t| now - t > stale_after),
            os: inv.and_then(|i| i.os.pretty_name.clone().or_else(|| i.os.name.clone())),
            kernel: inv.and_then(|i| i.kernel.release.clone()),
            cores: cpu.map(|c| c.cores),
            cpu_pct: cpu.map(|c| c.total.busy),
            iowait_pct: cpu.map(|c| c.total.iowait),
            mem_pct: mem.map(|m| m.used_pct),
            mem_total: mem.map(|m| m.total),
            swap_pct: mem.filter(|m| m.swap_total > 0).map(|m| m.swap_used_pct),
            load1: load.map(|l| l.one),
            load5: load.map(|l| l.five),
            load15: load.map(|l| l.fifteen),
            disk: worst,
            net_rx: m.map(|_| rx),
            net_tx: m.map(|_| tx),
            uptime_secs: m.and_then(|m| m.uptime_secs),
            reboot_required: inv.and_then(|i| i.reboot.required),
            pending_updates: updates.map(|u| u.total),
            security_updates: updates.and_then(|u| u.security),
            failed_units,
            failed_logins_24h: self.failed_logins_24h,
            alerts: self.alerts,
            spark: with_spark.then(|| self.spark.iter().copied().collect()),
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiskWorst {
    pub mount: String,
    pub used_pct: f64,
    pub avail: u64,
}

/// One row of the fleet overview.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HostSummary {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub groups: Vec<String>,
    pub tags: Vec<String>,
    pub baseline: Option<String>,
    /// Effective status: `ok`, `warning`, `critical`, `pending`, `disabled`,
    /// `unreachable`, `auth_failed`, `host_key_unknown` or `host_key_changed`.
    pub status: String,
    pub connection: String,
    pub health: String,
    pub error: Option<String>,
    pub host_key: Option<HostKeyInfo>,
    /// Some requested data could not be collected.
    pub partial: bool,
    pub missing: Vec<String>,
    pub last_attempt: Option<i64>,
    pub last_success: Option<i64>,
    /// No successful collection for several intervals.
    pub stale: bool,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub cores: Option<u32>,
    pub cpu_pct: Option<f64>,
    pub iowait_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub mem_total: Option<u64>,
    pub swap_pct: Option<f64>,
    pub load1: Option<f64>,
    pub load5: Option<f64>,
    pub load15: Option<f64>,
    pub disk: Option<DiskWorst>,
    pub net_rx: Option<f64>,
    pub net_tx: Option<f64>,
    pub uptime_secs: Option<f64>,
    pub reboot_required: Option<bool>,
    pub pending_updates: Option<u32>,
    pub security_updates: Option<u32>,
    pub failed_units: Option<u32>,
    pub failed_logins_24h: Option<u64>,
    pub alerts: AlertCounts,
    pub spark: Option<Vec<SparkPoint>>,
}

/// All hosts, keyed by name.
#[derive(Debug, Default)]
pub struct Fleet {
    hosts: RwLock<BTreeMap<String, HostState>>,
}

impl Fleet {
    pub fn insert(&self, h: HostState) {
        if let Ok(mut map) = self.hosts.write() {
            map.insert(h.cfg.name.clone(), h);
        }
    }

    /// Runs `f` on a host's state, returning its result.
    pub fn update<R>(&self, name: &str, f: impl FnOnce(&mut HostState) -> R) -> Option<R> {
        let mut map = self.hosts.write().ok()?;
        map.get_mut(name).map(f)
    }

    pub fn get(&self, name: &str) -> Option<HostState> {
        self.hosts.read().ok()?.get(name).cloned()
    }

    pub fn with<R>(&self, f: impl FnOnce(&BTreeMap<String, HostState>) -> R) -> Option<R> {
        self.hosts.read().ok().map(|m| f(&m))
    }

    pub fn names(&self) -> Vec<String> {
        self.with(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn summaries(&self, now: i64, stale_after: i64, with_spark: bool) -> Vec<HostSummary> {
        self.with(|m| {
            m.values()
                .map(|h| h.summary(now, stale_after, with_spark))
                .collect()
        })
        .unwrap_or_default()
    }
}

/// Server-sent event payloads.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// A host summary changed.
    Host(Box<HostSummary>),
    /// Alerts changed; clients refetch.
    Alerts,
    /// Inventory changes were recorded for a host.
    Changes { host: String, count: usize },
    /// Pending host keys changed.
    HostKeys,
}

impl Event {
    pub fn name(&self) -> &'static str {
        match self {
            Event::Host(_) => "host",
            Event::Alerts => "alerts",
            Event::Changes { .. } => "changes",
            Event::HostKeys => "hostkeys",
        }
    }
}
