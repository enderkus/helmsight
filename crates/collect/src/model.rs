//! Data types produced by the parsers. All of them serialize to the JSON
//! shapes used by the HTTP API.

use serde::{Deserialize, Serialize};

/// Result of a probe that may be unavailable on a given host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Probe<T> {
    /// The data was collected. `source` names the tool or file it came from.
    Ok { source: String, data: T },
    /// The data could not be collected; `reason` explains why in one line.
    Na { reason: String },
}

impl<T> Probe<T> {
    pub fn ok(source: impl Into<String>, data: T) -> Self {
        Probe::Ok {
            source: source.into(),
            data,
        }
    }

    pub fn na(reason: impl Into<String>) -> Self {
        Probe::Na {
            reason: reason.into(),
        }
    }

    pub fn data(&self) -> Option<&T> {
        match self {
            Probe::Ok { data, .. } => Some(data),
            Probe::Na { .. } => None,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Probe::Ok { .. })
    }
}

/// CPU time split, in percent of total capacity (all cores = 100).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CpuBreakdown {
    pub user: f64,
    pub nice: f64,
    pub system: f64,
    pub iowait: f64,
    pub irq: f64,
    pub softirq: f64,
    pub steal: f64,
    pub idle: f64,
    /// Everything except idle and iowait.
    pub busy: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CpuMetrics {
    pub cores: u32,
    pub total: CpuBreakdown,
    /// Busy percentage per core, indexed by core number.
    pub per_core: Vec<f64>,
}

/// Memory figures in bytes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub used: u64,
    pub buffers: u64,
    pub cached: u64,
    pub used_pct: f64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub swap_used_pct: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LoadAvg {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
    pub runnable: u32,
    pub entities: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Filesystem {
    pub device: String,
    pub mount: String,
    pub fstype: Option<String>,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub used_pct: f64,
    pub inodes_total: Option<u64>,
    pub inodes_used: Option<u64>,
    pub inodes_used_pct: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiskIo {
    pub device: String,
    pub read_bps: f64,
    pub write_bps: f64,
    pub read_iops: f64,
    pub write_iops: f64,
    pub util_pct: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NetIface {
    pub name: String,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_pps: f64,
    pub tx_pps: f64,
    pub rx_errors: f64,
    pub tx_errors: f64,
    pub rx_drops: f64,
    pub tx_drops: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TcpSummary {
    pub established: u64,
    pub listen: u64,
    pub time_wait: u64,
    pub close_wait: u64,
    pub other: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub user: Option<String>,
    pub command: Option<String>,
    pub state: String,
    pub threads: u32,
    pub rss_bytes: u64,
    /// Percent of one core (like `top`); `None` until two samples exist.
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProcSummary {
    pub total: u32,
    pub running: u32,
    pub blocked: u32,
    pub top_cpu: Vec<Process>,
    pub top_mem: Vec<Process>,
}

/// Fast-changing metrics collected on every tick.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Metrics {
    pub remote_time: Option<i64>,
    pub cpu: Option<CpuMetrics>,
    pub mem: Option<Memory>,
    pub load: Option<LoadAvg>,
    pub uptime_secs: Option<f64>,
    pub boot_time: Option<i64>,
    pub filesystems: Vec<Filesystem>,
    pub disks: Vec<DiskIo>,
    pub net: Vec<NetIface>,
    pub tcp: Option<TcpSummary>,
    pub procs: Option<ProcSummary>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
pub struct ListenSocket {
    pub proto: String,
    pub address: String,
    pub port: u16,
    pub process: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Service {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

impl Service {
    pub fn is_failed(&self) -> bool {
        self.active == "failed" || self.sub == "failed" || self.sub == "crashed"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub cpu_pct: Option<f64>,
    pub mem_bytes: Option<u64>,
    pub mem_limit: Option<u64>,
    pub mem_pct: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Session {
    pub user: String,
    pub line: String,
    pub from: Option<String>,
    pub login: String,
}

/// Medium-rate data (default every 60 s).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Medium {
    pub listening: Probe<Vec<ListenSocket>>,
    pub services: Probe<Vec<Service>>,
    pub containers: Probe<Vec<Container>>,
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OsRelease {
    pub id: Option<String>,
    pub id_like: Option<String>,
    pub name: Option<String>,
    pub version_id: Option<String>,
    pub pretty_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Kernel {
    pub name: Option<String>,
    pub release: Option<String>,
    pub version: Option<String>,
    pub machine: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RebootStatus {
    /// `None` when no detection mechanism is available on the host.
    pub required: Option<bool>,
    pub reasons: Vec<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub arch: Option<String>,
}

/// Slow-changing inventory (default every 15 minutes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Inventory {
    pub os: OsRelease,
    pub kernel: Kernel,
    pub cpu_model: Option<String>,
    pub virtualization: Option<String>,
    pub reboot: RebootStatus,
    pub packages: Probe<Vec<Package>>,
    pub enabled_units: Probe<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AuthFailure {
    /// Unix seconds.
    pub ts: i64,
    pub user: String,
    pub ip: Option<String>,
    pub invalid_user: bool,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PendingUpdate {
    pub name: String,
    pub current: Option<String>,
    pub available: String,
    pub security: bool,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateReport {
    pub total: u32,
    /// `None` when the package manager cannot classify security updates.
    pub security: Option<u32>,
    pub packages: Vec<PendingUpdate>,
    pub notes: Vec<String>,
}

/// Static facts about the SSH session, refreshed with the inventory.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Basics {
    pub page_size: Option<u64>,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub uid: Option<u32>,
}
