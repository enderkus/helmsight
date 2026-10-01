// Types of the JSON API (/api/v1). They mirror the Rust structs; see
// /api/v1/openapi.json for the authoritative schema.

export type Role = 'viewer' | 'operator' | 'admin';

export interface User {
  id: number;
  username: string;
  display_name: string | null;
  role: Role;
  totp_enabled: boolean;
  oidc_subject: string | null;
  created_at: number;
  last_login_at: number | null;
  disabled: boolean;
  must_change_password: boolean;
}

export interface Meta {
  product: string;
  version: string;
  setup_required: boolean;
  interval: number;
  features: {
    actions: boolean;
    oidc: string | null;
    local_login: boolean;
    local_mode: boolean;
    prometheus: boolean;
  };
}

export interface HostKeyInfo {
  endpoint: string;
  algorithm: string;
  fingerprint: string;
  previous: string | null;
}

export interface AlertCounts {
  critical: number;
  warning: number;
  info: number;
}

export interface SparkPoint {
  ts: number;
  cpu: number | null;
  mem: number | null;
}

export type HostStatus =
  | 'ok'
  | 'warning'
  | 'critical'
  | 'pending'
  | 'disabled'
  | 'unreachable'
  | 'auth_failed'
  | 'host_key_unknown'
  | 'host_key_changed';

export interface HostSummary {
  name: string;
  address: string;
  port: number;
  groups: string[];
  tags: string[];
  baseline: string | null;
  status: HostStatus;
  connection: string;
  health: string;
  error: string | null;
  host_key: HostKeyInfo | null;
  partial: boolean;
  missing: string[];
  last_attempt: number | null;
  last_success: number | null;
  stale: boolean;
  os: string | null;
  kernel: string | null;
  cores: number | null;
  cpu_pct: number | null;
  iowait_pct: number | null;
  mem_pct: number | null;
  mem_total: number | null;
  swap_pct: number | null;
  load1: number | null;
  load5: number | null;
  load15: number | null;
  disk: { mount: string; used_pct: number; avail: number } | null;
  net_rx: number | null;
  net_tx: number | null;
  uptime_secs: number | null;
  reboot_required: boolean | null;
  pending_updates: number | null;
  security_updates: number | null;
  failed_units: number | null;
  failed_logins_24h: number | null;
  alerts: AlertCounts;
  spark: SparkPoint[] | null;
}

export type Probe<T> = { status: 'ok'; source: string; data: T } | { status: 'na'; reason: string };

export interface Timed<T> {
  ts: number;
  data: T;
}

export interface CpuBreakdown {
  user: number;
  nice: number;
  system: number;
  iowait: number;
  irq: number;
  softirq: number;
  steal: number;
  idle: number;
  busy: number;
}

export interface Process {
  pid: number;
  name: string;
  user: string | null;
  command: string | null;
  state: string;
  threads: number;
  rss_bytes: number;
  cpu_pct: number | null;
  mem_pct: number | null;
}

export interface Filesystem {
  device: string;
  mount: string;
  fstype: string | null;
  total: number;
  used: number;
  avail: number;
  used_pct: number;
  inodes_total: number | null;
  inodes_used: number | null;
  inodes_used_pct: number | null;
}

export interface Metrics {
  remote_time: number | null;
  cpu: { cores: number; total: CpuBreakdown; per_core: number[] } | null;
  mem: {
    total: number;
    free: number;
    available: number;
    used: number;
    buffers: number;
    cached: number;
    used_pct: number;
    swap_total: number;
    swap_used: number;
    swap_used_pct: number;
  } | null;
  load: { one: number; five: number; fifteen: number; runnable: number; entities: number } | null;
  uptime_secs: number | null;
  boot_time: number | null;
  filesystems: Filesystem[];
  disks: { device: string; read_bps: number; write_bps: number; read_iops: number; write_iops: number; util_pct: number }[];
  net: {
    name: string;
    rx_bps: number;
    tx_bps: number;
    rx_pps: number;
    tx_pps: number;
    rx_errors: number;
    tx_errors: number;
    rx_drops: number;
    tx_drops: number;
  }[];
  tcp: { established: number; listen: number; time_wait: number; close_wait: number; other: number; total: number } | null;
  procs: { total: number; running: number; blocked: number; top_cpu: Process[]; top_mem: Process[] } | null;
}

export interface ListenSocket {
  proto: string;
  address: string;
  port: number;
  process: string | null;
  pid: number | null;
}

export interface Service {
  unit: string;
  load: string;
  active: string;
  sub: string;
  description: string;
}

export interface Container {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  cpu_pct: number | null;
  mem_bytes: number | null;
  mem_limit: number | null;
  mem_pct: number | null;
}

export interface Session {
  user: string;
  line: string;
  from: string | null;
  login: string;
}

export interface Medium {
  listening: Probe<ListenSocket[]>;
  services: Probe<Service[]>;
  containers: Probe<Container[]>;
  sessions: Session[];
}

export interface PendingUpdate {
  name: string;
  current: string | null;
  available: string;
  security: boolean;
  repo: string | null;
}

export interface UpdateReport {
  total: number;
  security: number | null;
  packages: PendingUpdate[];
  notes: string[];
}

export interface RebootStatus {
  required: boolean | null;
  reasons: string[];
}

export interface InventoryView {
  ts: number;
  os: { id: string | null; id_like: string | null; name: string | null; version_id: string | null; pretty_name: string | null };
  kernel: { name: string | null; release: string | null; version: string | null; machine: string | null };
  cpu_model: string | null;
  virtualization: string | null;
  reboot: RebootStatus;
  package_manager: string | null;
  package_count: number | null;
  packages_unavailable: string | null;
  enabled_units: Probe<string[]>;
}

export interface HostDetail {
  summary: HostSummary;
  collection: {
    last_attempt: number | null;
    last_success: number | null;
    duration_ms: number | null;
    missing: string[];
    truncated: boolean;
    interval: number;
    clock_skew: number | null;
  };
  metrics: Timed<Metrics> | null;
  medium: Timed<Medium> | null;
  inventory: InventoryView | null;
  updates: Timed<Probe<UpdateReport>> | null;
  auth: Timed<{ source: string | null; unavailable: string | null }> | null;
  basics: { page_size: number | null; hostname: string | null; user: string | null; uid: number | null } | null;
}

export interface Series {
  key: string;
  points: [number, number, number, number][];
}

export interface QueryResult {
  resolution: string;
  series: Series[];
}

export interface Change {
  id: number;
  host: string;
  ts: number;
  kind: string;
  action: 'added' | 'removed' | 'changed';
  subject: string;
  old: string | null;
  new: string | null;
}

export interface Comparison {
  a: string;
  b: string;
  a_ts: number | null;
  b_ts: number | null;
  differences: Omit<Change, 'id' | 'host' | 'ts'>[];
  counts: Record<string, number>;
  incomplete: string[];
}

export interface AuthBucket {
  ts: number;
  count: number;
}

export interface TopSource {
  value: string;
  count: number;
}

export interface AuthReport {
  source: string | null;
  unavailable: string | null;
  total: number;
  histogram: AuthBucket[];
  top_ips: TopSource[];
  top_users: TopSource[];
}

export interface HostSecurity {
  auth: AuthReport;
  updates: Timed<Probe<UpdateReport>> | null;
  reboot: RebootStatus | null;
  baseline: string | null;
  new_ports: Omit<Change, 'id' | 'host' | 'ts'>[];
}

export interface HostSecurityRow {
  name: string;
  status: HostStatus;
  groups: string[];
  tags: string[];
  failed_logins_24h: number | null;
  auth_source: string | null;
  auth_unavailable: string | null;
  pending_updates: number | null;
  security_updates: number | null;
  updates_unavailable: string | null;
  updates_checked_at: number | null;
  reboot_required: boolean | null;
  reboot_reasons: string[];
  baseline: string | null;
  new_ports: string[];
}

export interface CertView {
  endpoint: string;
  checked_at: number;
  not_after: number | null;
  days_left: number | null;
  subject: string | null;
  issuer: string | null;
  error: string | null;
  trust_error: string | null;
}

export interface SecurityOverview {
  auth: AuthReport;
  hosts: HostSecurityRow[];
  certs: CertView[];
  certs_pending: string[];
}

export interface Alert {
  id: number;
  fingerprint: string;
  rule_id: string;
  host: string | null;
  instance: string | null;
  severity: 'critical' | 'warning' | 'info';
  state: 'firing' | 'resolved';
  summary: string;
  value: number | null;
  started_at: number;
  resolved_at: number | null;
  last_notified_at: number | null;
  ack_by: string | null;
  ack_at: number | null;
  ack_note: string | null;
  silenced: boolean;
}

export interface Silence {
  id: number;
  rule_id: string | null;
  host: string | null;
  reason: string;
  created_by: string;
  created_at: number;
  ends_at: number;
  expired_by: string | null;
}

export interface RuleView {
  id: string;
  expr: string;
  severity: string;
  summary: string | null;
  builtin: boolean;
  hosts: string[];
  groups: string[];
  tags: string[];
}

export interface RulesView {
  rules: RuleView[];
  metrics: { name: string; unit: string; description: string; instance: string | null }[];
}

export interface ActionView {
  id: string;
  label: string;
  description: string;
  command: string;
  role: Role;
  timeout_secs: number;
  hosts: string[];
}

export interface RunResult {
  action: string;
  host: string;
  command: string;
  exit_status: number | null;
  stdout: string;
  stderr: string;
  truncated: boolean;
  duration_ms: number;
  audit_id: number;
}

export interface AuditEntry {
  id: number;
  ts: number;
  actor: string;
  action: string;
  target: string | null;
  detail: Record<string, unknown>;
  hash: string;
}

export interface PendingKey {
  hosts: string[];
  endpoint: string;
  algorithm: string;
  fingerprint: string;
  previous: string | null;
  seen_at: number;
}

export interface DisplayToken {
  id: number;
  name: string;
  groups: string[];
  created_by: string;
  created_at: number;
  revoked_at: number | null;
  last_used_at: number | null;
}

export interface DisplayState {
  product: string;
  viewer: string;
  generated_at: number;
  hosts: {
    name: string;
    status: HostStatus;
    groups: string[];
    cpu_pct: number | null;
    mem_pct: number | null;
    disk_pct: number | null;
    load1: number | null;
    alerts: AlertCounts;
    stale: boolean;
    last_success: number | null;
  }[];
  alerts: { severity: string; host: string | null; summary: string; started_at: number; acknowledged: boolean }[];
}
