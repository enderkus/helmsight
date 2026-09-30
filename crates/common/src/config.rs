//! The TOML configuration file: schema, defaults, loading and validation.
//!
//! Validation errors carry the file, line, column and key path, so that
//! `config check` can say exactly what to fix.

use crate::duration::Dur;
use crate::locate::{Seg, line_col, locate, path_string};
use crate::role::Role;
use crate::rules::Expr;
use crate::secrets::SecretRef;
use crate::selector::{Selectable, Selector};
use crate::sshconfig;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub ssh: SshConfig,
    #[serde(default)]
    pub collect: CollectConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    /// Optional separate inventory file containing `[[hosts]]` entries.
    /// Relative paths are resolved against the config file directory.
    pub hosts_file: Option<PathBuf>,
    /// Import hosts from an OpenSSH client config.
    pub ssh_config_import: Option<SshConfigImport>,
    #[serde(default)]
    pub hosts: Vec<HostConfig>,
    #[serde(default)]
    pub alerts: AlertsConfig,
    #[serde(default)]
    pub notify: Vec<NotifyConfig>,
    #[serde(default)]
    pub certs: Vec<CertConfig>,
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
    #[serde(default)]
    pub prometheus: PrometheusConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to bind. Defaults to loopback only.
    #[serde(default = "default_listen")]
    pub listen: String,
    /// Directory for the database, key file, known_hosts and TLS files.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// External URL, e.g. `https://monitor.example.com`. Required for OIDC
    /// and used in notification links.
    pub public_url: Option<String>,
    /// Peers whose `X-Forwarded-For` / `X-Forwarded-Proto` headers are trusted.
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    #[serde(default)]
    pub tls: TlsConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            data_dir: default_data_dir(),
            public_url: None,
            trusted_proxies: Vec::new(),
            tls: TlsConfig::default(),
        }
    }
}

fn default_listen() -> String {
    "127.0.0.1:8080".into()
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("data")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    /// Plain HTTP on loopback addresses, self-signed TLS otherwise.
    #[default]
    Auto,
    /// Plain HTTP. Only allowed on loopback unless `allow_insecure_http`.
    Off,
    /// A self-signed certificate generated on first run.
    SelfSigned,
    /// Certificate and key from files.
    Files,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsConfig {
    #[serde(default)]
    pub mode: TlsMode,
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
    /// Permit plain HTTP on a non-loopback address, for deployments behind a
    /// TLS-terminating reverse proxy.
    #[serde(default)]
    pub allow_insecure_http: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_session_ttl")]
    pub session_ttl: Dur,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: Dur,
    /// Require TOTP for local admin accounts.
    #[serde(default)]
    pub require_totp_for_admins: bool,
    /// Disable local password accounts (OIDC only).
    #[serde(default)]
    pub disable_local_login: bool,
    pub oidc: Option<OidcConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_ttl: default_session_ttl(),
            idle_timeout: default_idle_timeout(),
            require_totp_for_admins: false,
            disable_local_login: false,
            oidc: None,
        }
    }
}

fn default_session_ttl() -> Dur {
    Dur::secs(12 * 3600)
}

fn default_idle_timeout() -> Dur {
    Dur::secs(2 * 3600)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<SecretRef>,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    /// Claim holding group or role names, e.g. `groups` or `roles`.
    #[serde(default = "default_role_claim")]
    pub role_claim: String,
    /// Claim value to role. The highest matching role wins.
    #[serde(default)]
    pub role_map: BTreeMap<String, Role>,
    /// Role for users without a matching claim. Unset denies access.
    pub default_role: Option<Role>,
    /// Label on the login button.
    #[serde(default = "default_oidc_label")]
    pub label: String,
}

fn default_scopes() -> Vec<String> {
    vec!["openid".into(), "profile".into(), "email".into()]
}

fn default_role_claim() -> String {
    "groups".into()
}

fn default_oidc_label() -> String {
    "Single sign-on".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshConfig {
    /// Default remote user.
    #[serde(default = "default_ssh_user")]
    pub user: String,
    /// Private key files tried in order.
    #[serde(default)]
    pub identity_files: Vec<PathBuf>,
    /// Use keys from the running ssh-agent (`SSH_AUTH_SOCK`).
    #[serde(default = "yes")]
    pub use_agent: bool,
    /// Additional read-only known_hosts files (e.g. `~/.ssh/known_hosts`).
    /// Keys approved in helmsight are stored in `<data_dir>/known_hosts`.
    #[serde(default)]
    pub known_hosts_files: Vec<PathBuf>,
    /// Trust keys of hosts seen for the first time (like OpenSSH
    /// `StrictHostKeyChecking=accept-new`). Changed keys are always refused.
    #[serde(default)]
    pub accept_new_host_keys: bool,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: Dur,
    #[serde(default = "default_command_timeout")]
    pub command_timeout: Dur,
    #[serde(default = "default_keepalive")]
    pub keepalive: Dur,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            user: default_ssh_user(),
            identity_files: Vec::new(),
            use_agent: true,
            known_hosts_files: Vec::new(),
            accept_new_host_keys: false,
            connect_timeout: default_connect_timeout(),
            command_timeout: default_command_timeout(),
            keepalive: default_keepalive(),
        }
    }
}

fn yes() -> bool {
    true
}

fn default_ssh_user() -> String {
    "monitor".into()
}

fn default_connect_timeout() -> Dur {
    Dur::secs(10)
}

fn default_command_timeout() -> Dur {
    Dur::secs(30)
}

fn default_keepalive() -> Dur {
    Dur::secs(30)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectConfig {
    /// Fast metrics.
    #[serde(default = "d5s")]
    pub interval: Dur,
    /// Listening ports, services, containers, sessions.
    #[serde(default = "d60s")]
    pub medium_interval: Dur,
    /// Packages, enabled units, OS and kernel.
    #[serde(default = "d15m")]
    pub inventory_interval: Dur,
    /// Failed SSH logins.
    #[serde(default = "d5m")]
    pub auth_interval: Dur,
    /// Pending updates (expensive).
    #[serde(default = "d6h")]
    pub updates_interval: Dur,
    /// Maximum number of hosts collected concurrently.
    #[serde(default = "default_parallel")]
    pub max_parallel: usize,
}

impl Default for CollectConfig {
    fn default() -> Self {
        Self {
            interval: d5s(),
            medium_interval: d60s(),
            inventory_interval: d15m(),
            auth_interval: d5m(),
            updates_interval: d6h(),
            max_parallel: default_parallel(),
        }
    }
}

fn d5s() -> Dur {
    Dur::secs(5)
}
fn d60s() -> Dur {
    Dur::secs(60)
}
fn d5m() -> Dur {
    Dur::secs(300)
}
fn d15m() -> Dur {
    Dur::secs(900)
}
fn d6h() -> Dur {
    Dur::secs(6 * 3600)
}
fn default_parallel() -> usize {
    64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionConfig {
    #[serde(default = "d24h")]
    pub raw: Dur,
    #[serde(default = "d7d")]
    pub minute: Dur,
    #[serde(default = "d90d")]
    pub five_minute: Dur,
    /// Inventory changes, resolved alerts and security events.
    #[serde(default = "d90d")]
    pub events: Dur,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            raw: d24h(),
            minute: d7d(),
            five_minute: d90d(),
            events: d90d(),
        }
    }
}

fn d24h() -> Dur {
    Dur::secs(86_400)
}
fn d7d() -> Dur {
    Dur::secs(7 * 86_400)
}
fn d90d() -> Dur {
    Dur::secs(90 * 86_400)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshConfigImport {
    #[serde(default = "default_ssh_config_path")]
    pub path: PathBuf,
    /// Glob patterns of host aliases to import. Empty imports none.
    pub hosts: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_ssh_config_path() -> PathBuf {
    PathBuf::from("~/.ssh/config")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    /// Unique display name.
    pub name: String,
    /// Hostname or IP address. Defaults to `name`.
    pub address: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_file: Option<PathBuf>,
    #[serde(default)]
    pub groups: Vec<String>,
    /// Free-form tags, typically `key:value` such as `env:prod`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Host used as the reference for drift and security comparisons.
    pub baseline: Option<String>,
    /// Temporarily stop collecting from this host.
    #[serde(default)]
    pub disabled: bool,
}

impl HostConfig {
    pub fn address(&self) -> &str {
        self.address.as_deref().unwrap_or(&self.name)
    }

    pub fn port(&self) -> u16 {
        self.port.unwrap_or(22)
    }
}

impl Selectable for HostConfig {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertsConfig {
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
    /// Host unreachable for longer than this fires `host_unreachable`.
    #[serde(default = "d2m")]
    pub unreachable_after: Dur,
    #[serde(default = "yes")]
    pub failed_units: bool,
    #[serde(default = "default_cert_warning")]
    pub cert_warning_days: u32,
    #[serde(default = "default_cert_critical")]
    pub cert_critical_days: u32,
    /// Re-send notifications for alerts still firing and not acknowledged.
    /// `0s` disables reminders.
    #[serde(default = "d4h")]
    pub repeat_interval: Dur,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            unreachable_after: d2m(),
            failed_units: true,
            cert_warning_days: default_cert_warning(),
            cert_critical_days: default_cert_critical(),
            repeat_interval: d4h(),
        }
    }
}

fn d2m() -> Dur {
    Dur::secs(120)
}
fn d4h() -> Dur {
    Dur::secs(4 * 3600)
}
fn default_cert_warning() -> u32 {
    21
}
fn default_cert_critical() -> u32 {
    7
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleConfig {
    pub id: String,
    /// `<metric> <op> <threshold> [for <duration>]`
    pub expr: String,
    #[serde(default = "default_severity")]
    pub severity: Severity,
    /// Short human description shown in the UI and notifications.
    pub summary: Option<String>,
    /// Hosts the rule applies to. Empty applies to all hosts.
    #[serde(default, flatten)]
    pub scope: Selector,
}

fn default_severity() -> Severity {
    Severity::Warning
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyKind {
    Webhook,
    Slack,
    Email,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotifyConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: NotifyKind,
    /// Minimum severity delivered to this channel.
    #[serde(default = "default_notify_severity")]
    pub min_severity: Severity,
    /// Webhook or Slack URL.
    pub url: Option<SecretRef>,
    /// Extra HTTP headers for webhooks (values may be secret references).
    #[serde(default)]
    pub headers: BTreeMap<String, SecretRef>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    /// `starttls` (default), `tls` or `none` (localhost relays only).
    pub smtp_security: Option<String>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<SecretRef>,
    pub from: Option<String>,
    #[serde(default)]
    pub to: Vec<String>,
    /// Only deliver alerts for these hosts.
    #[serde(default, flatten)]
    pub scope: Selector,
}

fn default_notify_severity() -> Severity {
    Severity::Warning
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertConfig {
    /// `host:port` to connect to.
    pub endpoint: String,
    /// Name sent in SNI and verified; defaults to the endpoint host.
    pub server_name: Option<String>,
    #[serde(default = "d6h")]
    pub interval: Dur,
}

impl CertConfig {
    pub fn server_name(&self) -> &str {
        self.server_name.as_deref().unwrap_or_else(|| {
            self.endpoint
                .rsplit_once(':')
                .map(|(h, _)| h.trim_start_matches('[').trim_end_matches(']'))
                .unwrap_or(&self.endpoint)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConfig {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// Exact command executed over SSH. No parameters, no templating.
    pub command: String,
    /// Minimum role allowed to run the action.
    #[serde(default = "default_action_role")]
    pub role: Role,
    #[serde(default = "d60s")]
    pub timeout: Dur,
    /// Hosts where the action is offered. Must not be empty.
    #[serde(default, flatten)]
    pub targets: Selector,
}

fn default_action_role() -> Role {
    Role::Operator
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrometheusConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Bearer token required on `/metrics`.
    pub token: Option<SecretRef>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub key: Option<String>,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())?;
        if let Some(l) = self.line {
            write!(f, ":{l}")?;
            if let Some(c) = self.column {
                write!(f, ":{c}")?;
            }
        }
        write!(f, ": ")?;
        if let Some(k) = &self.key {
            write!(f, "`{k}`: ")?;
        }
        write!(f, "{}", self.message)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigErrors(pub Vec<ConfigError>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "error: {e}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigErrors {}

/// A source file and its text, used to locate errors.
struct Source {
    path: PathBuf,
    text: String,
}

impl Source {
    fn err(&self, path: &[Seg], message: impl Into<String>) -> ConfigError {
        let (line, column) = match locate(&self.text, path) {
            Some((l, c)) => (Some(l), Some(c)),
            None => (None, None),
        };
        ConfigError {
            file: self.path.clone(),
            line,
            column,
            key: (!path.is_empty()).then(|| path_string(path)),
            message: message.into(),
        }
    }

    fn parse_error(&self, e: &toml::de::Error) -> ConfigError {
        let (line, column) = match e.span() {
            Some(s) => {
                let (l, c) = line_col(&self.text, s.start);
                (Some(l), Some(c))
            }
            None => (None, None),
        };
        ConfigError {
            file: self.path.clone(),
            line,
            column,
            key: None,
            message: e.message().trim().to_string(),
        }
    }
}

fn k(s: &str) -> Seg {
    Seg::Key(s.to_string())
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Where a host definition came from, for error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostOrigin {
    Config(usize),
    HostsFile(usize),
    SshConfig(String),
}

/// A loaded, validated configuration.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: Config,
    pub path: PathBuf,
    /// Non-fatal findings such as plaintext secrets.
    pub warnings: Vec<String>,
}

impl Config {
    /// Parses and validates a config from a string (host files and ssh
    /// config imports are resolved relative to `path`).
    pub fn from_str_at(text: &str, path: &Path) -> Result<Loaded, ConfigErrors> {
        let main = Source {
            path: path.to_path_buf(),
            text: text.to_string(),
        };
        let mut config: Config =
            toml::from_str(text).map_err(|e| ConfigErrors(vec![main.parse_error(&e)]))?;
        let base = path.parent().unwrap_or(Path::new("."));
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut origins: Vec<HostOrigin> =
            (0..config.hosts.len()).map(HostOrigin::Config).collect();

        let mut hosts_source = None;
        if let Some(hf) = &config.hosts_file {
            let hf_path = resolve_path(base, hf);
            match std::fs::read_to_string(&hf_path) {
                Ok(t) => {
                    let src = Source {
                        path: hf_path.clone(),
                        text: t,
                    };
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct HostsFile {
                        #[serde(default)]
                        hosts: Vec<HostConfig>,
                    }
                    match toml::from_str::<HostsFile>(&src.text) {
                        Ok(h) => {
                            for (i, host) in h.hosts.into_iter().enumerate() {
                                origins.push(HostOrigin::HostsFile(i));
                                config.hosts.push(host);
                            }
                        }
                        Err(e) => errors.push(src.parse_error(&e)),
                    }
                    hosts_source = Some(src);
                }
                Err(e) => errors.push(main.err(
                    &[k("hosts_file")],
                    format!("cannot read {}: {e}", hf_path.display()),
                )),
            }
        }

        if let Some(imp) = &config.ssh_config_import {
            let p = expand_home(&imp.path);
            match std::fs::read_to_string(&p) {
                Ok(t) => {
                    let (hosts, warns) = sshconfig::parse(&t);
                    for w in warns {
                        warnings.push(format!("{}: {w}", p.display()));
                    }
                    for h in hosts {
                        if !imp.hosts.iter().any(|g| sshconfig::glob(g, &h.alias)) {
                            continue;
                        }
                        if config.hosts.iter().any(|c| c.name == h.alias) {
                            continue; // explicit definitions take precedence
                        }
                        if h.proxy_jump.is_some() {
                            warnings.push(format!(
                                "{}: host `{}` uses ProxyJump, which is not supported; skipped",
                                p.display(),
                                h.alias
                            ));
                            continue;
                        }
                        origins.push(HostOrigin::SshConfig(h.alias.clone()));
                        config.hosts.push(HostConfig {
                            name: h.alias.clone(),
                            address: h.hostname,
                            port: h.port,
                            user: h.user,
                            identity_file: h.identity_file.map(PathBuf::from),
                            groups: imp.groups.clone(),
                            tags: imp.tags.clone(),
                            baseline: None,
                            disabled: false,
                        });
                    }
                }
                Err(e) => errors.push(main.err(
                    &[k("ssh_config_import"), k("path")],
                    format!("cannot read {}: {e}", p.display()),
                )),
            }
        }

        validate(
            &config,
            &main,
            hosts_source.as_ref(),
            &origins,
            &mut errors,
            &mut warnings,
        );
        if errors.is_empty() {
            Ok(Loaded {
                config,
                path: path.to_path_buf(),
                warnings,
            })
        } else {
            Err(ConfigErrors(errors))
        }
    }

    /// Reads and validates the config file at `path`.
    pub fn load(path: &Path) -> Result<Loaded, ConfigErrors> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            ConfigErrors(vec![ConfigError {
                file: path.to_path_buf(),
                line: None,
                column: None,
                key: None,
                message: format!("cannot read file: {e}"),
            }])
        })?;
        Self::from_str_at(&text, path)
    }

    pub fn host(&self, name: &str) -> Option<&HostConfig> {
        self.hosts.iter().find(|h| h.name == name)
    }

    /// Absolute data directory, resolved against the config file location.
    pub fn data_dir(&self, config_path: &Path) -> PathBuf {
        resolve_path(
            config_path.parent().unwrap_or(Path::new(".")),
            &self.server.data_dir,
        )
    }
}

/// Expands a leading `~/` using `$HOME`.
pub fn expand_home(p: &Path) -> PathBuf {
    if let Ok(rest) = p.strip_prefix("~")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    p.to_path_buf()
}

pub fn resolve_path(base: &Path, p: &Path) -> PathBuf {
    let p = expand_home(p);
    if p.is_absolute() { p } else { base.join(p) }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn valid_ident(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !s.starts_with(['-', '.'])
}

fn valid_tag(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_graphic())
        && !s.contains(['"', '\'', '<', '>', '`', '\\', ','])
}

fn is_loopback(listen: &str) -> bool {
    listen
        .parse::<SocketAddr>()
        .map(|a| a.ip().is_loopback())
        .unwrap_or(false)
}

fn check_url(s: &str, allow_http_loopback: bool) -> Result<(), String> {
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    if lower.starts_with("http://") {
        let host = lower
            .trim_start_matches("http://")
            .split(['/', ':'])
            .next()
            .unwrap_or("");
        if allow_http_loopback && (host == "localhost" || host == "127.0.0.1" || host == "[") {
            return Ok(());
        }
        return Err("must use https:// (plain http is only allowed for localhost)".into());
    }
    Err("must be an https:// URL".into())
}

fn warn_literal(warnings: &mut Vec<String>, key: &str, r: &SecretRef) {
    if r.is_literal() {
        warnings.push(format!(
            "`{key}` contains a plaintext secret; prefer `secret:<name>` (stored encrypted), `env:<VAR>` or `file:<path>`"
        ));
    }
}

fn validate(
    c: &Config,
    src: &Source,
    hosts_src: Option<&Source>,
    origins: &[HostOrigin],
    errors: &mut Vec<ConfigError>,
    warnings: &mut Vec<String>,
) {
    // server
    match c.server.listen.parse::<SocketAddr>() {
        Ok(addr) => {
            let tls = &c.server.tls;
            if !addr.ip().is_loopback() && tls.mode == TlsMode::Off && !tls.allow_insecure_http {
                errors.push(src.err(
                    &[k("server"), k("tls"), k("mode")],
                    "plain HTTP on a non-loopback address exposes sessions; use TLS or set \
                     `allow_insecure_http = true` when a TLS-terminating proxy is in front",
                ));
            }
        }
        Err(_) => errors.push(src.err(
            &[k("server"), k("listen")],
            format!(
                "`{}` is not an address such as 127.0.0.1:8080",
                c.server.listen
            ),
        )),
    }
    if let Some(u) = &c.server.public_url
        && let Err(e) = check_url(u, true)
    {
        errors.push(src.err(&[k("server"), k("public_url")], e));
    }
    for (i, p) in c.server.trusted_proxies.iter().enumerate() {
        if p.parse::<std::net::IpAddr>().is_err() {
            errors.push(src.err(
                &[k("server"), k("trusted_proxies"), Seg::Index(i)],
                format!("`{p}` is not an IP address"),
            ));
        }
    }
    if c.server.tls.mode == TlsMode::Files
        && (c.server.tls.cert.is_none() || c.server.tls.key.is_none())
    {
        errors.push(src.err(
            &[k("server"), k("tls"), k("mode")],
            "mode = \"files\" requires both `cert` and `key`",
        ));
    }

    // auth
    if c.auth.session_ttl.as_secs() < 300 {
        errors.push(src.err(&[k("auth"), k("session_ttl")], "must be at least 5m"));
    }
    if c.auth.idle_timeout.as_secs() < 60 {
        errors.push(src.err(&[k("auth"), k("idle_timeout")], "must be at least 1m"));
    }
    if let Some(o) = &c.auth.oidc {
        let base = [k("auth"), k("oidc")];
        if let Err(e) = check_url(&o.issuer, true) {
            errors.push(src.err(&[base[0].clone(), base[1].clone(), k("issuer")], e));
        }
        if o.client_id.trim().is_empty() {
            errors.push(src.err(
                &[base[0].clone(), base[1].clone(), k("client_id")],
                "must not be empty",
            ));
        }
        if c.server.public_url.is_none() {
            errors.push(src.err(
                &[k("server")],
                "`public_url` is required when OIDC is configured (it forms the redirect URI)",
            ));
        }
        if o.role_map.is_empty() && o.default_role.is_none() {
            errors.push(src.err(
                &[base[0].clone(), base[1].clone()],
                "set `role_map` and/or `default_role`; otherwise nobody can sign in",
            ));
        }
        if !o.scopes.iter().any(|s| s == "openid") {
            errors.push(src.err(
                &[base[0].clone(), base[1].clone(), k("scopes")],
                "must include `openid`",
            ));
        }
        if let Some(s) = &o.client_secret {
            warn_literal(warnings, "auth.oidc.client_secret", s);
        }
    }
    if c.auth.disable_local_login && c.auth.oidc.is_none() {
        errors.push(src.err(
            &[k("auth"), k("disable_local_login")],
            "local login can only be disabled when OIDC is configured",
        ));
    }

    // ssh
    if c.ssh.user.trim().is_empty() {
        errors.push(src.err(&[k("ssh"), k("user")], "must not be empty"));
    }
    if c.ssh.connect_timeout.as_secs() == 0 || c.ssh.command_timeout.as_secs() == 0 {
        errors.push(src.err(&[k("ssh")], "timeouts must be at least 1s"));
    }

    // collect
    let col = &c.collect;
    if col.interval.0.as_millis() < 1000 {
        errors.push(src.err(&[k("collect"), k("interval")], "must be at least 1s"));
    }
    for (name, d) in [
        ("medium_interval", col.medium_interval),
        ("inventory_interval", col.inventory_interval),
        ("auth_interval", col.auth_interval),
        ("updates_interval", col.updates_interval),
    ] {
        if d < col.interval {
            errors.push(src.err(
                &[k("collect"), k(name)],
                format!("must not be shorter than `interval` ({})", col.interval),
            ));
        }
    }
    if col.max_parallel == 0 || col.max_parallel > 4096 {
        errors.push(src.err(
            &[k("collect"), k("max_parallel")],
            "must be between 1 and 4096",
        ));
    }

    // retention
    let r = &c.retention;
    if r.raw.as_secs() < 3600 {
        errors.push(src.err(&[k("retention"), k("raw")], "must be at least 1h"));
    }
    if r.minute < r.raw {
        errors.push(src.err(
            &[k("retention"), k("minute")],
            "must not be shorter than `raw`",
        ));
    }
    if r.five_minute < r.minute {
        errors.push(src.err(
            &[k("retention"), k("five_minute")],
            "must not be shorter than `minute`",
        ));
    }

    // hosts
    let mut names = BTreeSet::new();
    for (i, h) in c.hosts.iter().enumerate() {
        let origin = origins.get(i).cloned().unwrap_or(HostOrigin::Config(i));
        let herr = |field: &str, msg: String| -> ConfigError {
            match &origin {
                HostOrigin::Config(j) => src.err(&[k("hosts"), Seg::Index(*j), k(field)], msg),
                HostOrigin::HostsFile(j) => hosts_src
                    .map(|s| s.err(&[k("hosts"), Seg::Index(*j), k(field)], msg.clone()))
                    .unwrap_or_else(|| src.err(&[k("hosts_file")], msg.clone())),
                HostOrigin::SshConfig(alias) => src.err(
                    &[k("ssh_config_import")],
                    format!("imported host `{alias}`: {msg}"),
                ),
            }
        };
        if !valid_ident(&h.name, 64) {
            errors.push(herr(
                "name",
                format!(
                    "`{}` is not a valid host name (letters, digits, `.`, `-`, `_`; at most 64 characters)",
                    h.name
                ),
            ));
        } else if !names.insert(h.name.clone()) {
            errors.push(herr("name", format!("duplicate host name `{}`", h.name)));
        }
        let addr = h.address();
        if addr.is_empty()
            || addr.len() > 253
            || addr.starts_with('-')
            || addr.chars().any(|ch| ch.is_whitespace() || ch.is_control())
        {
            errors.push(herr(
                "address",
                format!("`{addr}` is not a valid hostname or IP address"),
            ));
        }
        if h.port == Some(0) {
            errors.push(herr("port", "must be between 1 and 65535".into()));
        }
        if let Some(u) = &h.user
            && !valid_ident(u, 32)
        {
            errors.push(herr("user", format!("`{u}` is not a valid user name")));
        }
        for (field, list) in [("groups", &h.groups), ("tags", &h.tags)] {
            for t in list {
                if !valid_tag(t) {
                    errors.push(herr(
                        field,
                        format!("`{t}` is not valid (no spaces or quotes)"),
                    ));
                }
            }
        }
    }
    for (i, h) in c.hosts.iter().enumerate() {
        if let Some(b) = &h.baseline {
            let path = [k("hosts"), Seg::Index(i), k("baseline")];
            if b == &h.name {
                errors.push(src.err(&path, "a host cannot be its own baseline"));
            } else if !names.contains(b) {
                errors.push(src.err(&path, format!("unknown host `{b}`")));
            }
        }
    }

    // alerts
    let mut ids = BTreeSet::new();
    for (i, rule) in c.alerts.rules.iter().enumerate() {
        let path = |f: &str| [k("alerts"), k("rules"), Seg::Index(i), k(f)];
        if !valid_ident(&rule.id, 64) {
            errors.push(src.err(&path("id"), format!("`{}` is not a valid id", rule.id)));
        } else if !ids.insert(rule.id.clone()) {
            errors.push(src.err(&path("id"), format!("duplicate rule id `{}`", rule.id)));
        }
        if let Err(e) = Expr::parse(&rule.expr) {
            errors.push(src.err(&path("expr"), e));
        }
        check_selector(&rule.scope, &names, src, &path("hosts"), errors);
    }
    if c.alerts.cert_critical_days > c.alerts.cert_warning_days {
        errors.push(src.err(
            &[k("alerts"), k("cert_critical_days")],
            "must not be greater than `cert_warning_days`",
        ));
    }

    // notifications
    let mut ids = BTreeSet::new();
    for (i, n) in c.notify.iter().enumerate() {
        let path = |f: &str| [k("notify"), Seg::Index(i), k(f)];
        if !valid_ident(&n.id, 64) {
            errors.push(src.err(&path("id"), format!("`{}` is not a valid id", n.id)));
        } else if !ids.insert(n.id.clone()) {
            errors.push(src.err(&path("id"), format!("duplicate channel id `{}`", n.id)));
        }
        match n.kind {
            NotifyKind::Webhook | NotifyKind::Slack => match &n.url {
                None => {
                    errors.push(src.err(&path("url"), "required for webhook and slack channels"))
                }
                Some(u) if u.is_literal() => {
                    if let Err(e) = check_url(&u.0, true) {
                        errors.push(src.err(&path("url"), e));
                    }
                    if n.kind == NotifyKind::Slack {
                        warn_literal(warnings, &format!("notify[{i}].url"), u);
                    }
                }
                Some(_) => {}
            },
            NotifyKind::Email => {
                if n.smtp_host.as_deref().unwrap_or("").is_empty() {
                    errors.push(src.err(&path("smtp_host"), "required for email channels"));
                }
                if n.from.as_deref().unwrap_or("").is_empty() {
                    errors.push(src.err(&path("from"), "required for email channels"));
                }
                if n.to.is_empty() {
                    errors.push(src.err(&path("to"), "list at least one recipient"));
                }
                for (j, addr) in n.to.iter().enumerate() {
                    if !addr.contains('@') || addr.contains(char::is_whitespace) {
                        errors.push(src.err(
                            &[k("notify"), Seg::Index(i), k("to"), Seg::Index(j)],
                            format!("`{addr}` is not an email address"),
                        ));
                    }
                }
                match n.smtp_security.as_deref() {
                    None | Some("starttls") | Some("tls") => {}
                    Some("none") => {
                        let host = n.smtp_host.as_deref().unwrap_or("");
                        if host != "localhost" && host != "127.0.0.1" && host != "::1" {
                            errors.push(src.err(
                                &path("smtp_security"),
                                "`none` is only allowed for a relay on localhost",
                            ));
                        }
                    }
                    Some(other) => errors.push(src.err(
                        &path("smtp_security"),
                        format!("unknown value `{other}` (use starttls, tls or none)"),
                    )),
                }
                if let Some(p) = &n.smtp_password {
                    warn_literal(warnings, &format!("notify[{i}].smtp_password"), p);
                }
            }
        }
        for v in n.headers.values() {
            warn_literal(warnings, &format!("notify[{i}].headers"), v);
        }
        check_selector(&n.scope, &names, src, &path("hosts"), errors);
    }

    // certificates
    for (i, cert) in c.certs.iter().enumerate() {
        let ok = cert
            .endpoint
            .rsplit_once(':')
            .is_some_and(|(h, p)| !h.is_empty() && p.parse::<u16>().is_ok_and(|p| p > 0));
        if !ok {
            errors.push(src.err(
                &[k("certs"), Seg::Index(i), k("endpoint")],
                format!(
                    "`{}` must be host:port, e.g. example.com:443",
                    cert.endpoint
                ),
            ));
        }
    }

    // actions
    let mut ids = BTreeSet::new();
    for (i, a) in c.actions.iter().enumerate() {
        let path = |f: &str| [k("actions"), Seg::Index(i), k(f)];
        if !valid_ident(&a.id, 64) {
            errors.push(src.err(&path("id"), format!("`{}` is not a valid id", a.id)));
        } else if !ids.insert(a.id.clone()) {
            errors.push(src.err(&path("id"), format!("duplicate action id `{}`", a.id)));
        }
        if a.label.trim().is_empty() {
            errors.push(src.err(&path("label"), "must not be empty"));
        }
        let cmd = a.command.trim();
        if cmd.is_empty() {
            errors.push(src.err(&path("command"), "must not be empty"));
        }
        if a.command.contains(['\n', '\r', '\0']) {
            errors.push(src.err(&path("command"), "must be a single line"));
        }
        let uses_sudo = cmd
            .split(|ch: char| ch.is_whitespace() || ch == ';' || ch == '&' || ch == '|')
            .any(|w| w == "sudo");
        if uses_sudo && !cmd.starts_with("sudo -n ") {
            errors.push(src.err(
                &path("command"),
                "privileged commands must start with `sudo -n ` (non-interactive) and use a \
                 sudoers rule for exactly this command",
            ));
        }
        if a.targets.is_empty() {
            errors.push(src.err(
                &path("id"),
                "list the target `hosts`, `groups` or `tags`; actions never apply to all hosts implicitly",
            ));
        }
        check_selector(&a.targets, &names, src, &path("hosts"), errors);
        if a.timeout.as_secs() == 0 || a.timeout.as_secs() > 3600 {
            errors.push(src.err(&path("timeout"), "must be between 1s and 1h"));
        }
        if a.role == Role::Viewer {
            errors.push(src.err(
                &path("role"),
                "viewers cannot run actions; use operator or admin",
            ));
        }
    }

    // prometheus
    if c.prometheus.enabled {
        match &c.prometheus.token {
            None => errors.push(src.err(
                &[k("prometheus"), k("token")],
                "required when the /metrics endpoint is enabled",
            )),
            Some(t) => warn_literal(warnings, "prometheus.token", t),
        }
    }
}

fn check_selector(
    sel: &Selector,
    names: &BTreeSet<String>,
    src: &Source,
    path: &[Seg],
    errors: &mut Vec<ConfigError>,
) {
    for h in &sel.hosts {
        if !names.contains(h) {
            errors.push(src.err(path, format!("unknown host `{h}`")));
        }
    }
}

impl Config {
    /// True when TLS will be used for the configured listen address.
    pub fn tls_enabled(&self) -> bool {
        match self.server.tls.mode {
            TlsMode::Off => false,
            TlsMode::SelfSigned | TlsMode::Files => true,
            TlsMode::Auto => !is_loopback(&self.server.listen),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: &str) -> Result<Loaded, ConfigErrors> {
        Config::from_str_at(text, Path::new("/etc/helmsight/helmsight.toml"))
    }

    #[test]
    fn empty_config_uses_secure_defaults() {
        let l = load("").unwrap();
        let c = l.config;
        assert_eq!(c.server.listen, "127.0.0.1:8080");
        assert!(!c.tls_enabled());
        assert!(!c.ssh.accept_new_host_keys);
        assert_eq!(c.collect.interval, Dur::secs(5));
        assert_eq!(c.retention.raw, Dur::secs(86_400));
        assert!(c.actions.is_empty());
    }

    #[test]
    fn syntax_errors_have_positions() {
        let e = load("[server]\nlisten = \n").unwrap_err();
        assert_eq!(e.0.len(), 1);
        assert_eq!(e.0[0].line, Some(2));
        let e = load("[server]\nlisen = \"x\"\n").unwrap_err();
        assert!(e.0[0].message.contains("unknown field"), "{e}");
        assert_eq!(e.0[0].line, Some(2));
    }

    #[test]
    fn semantic_errors_point_at_keys() {
        let text = r#"
[server]
listen = "0.0.0.0:8080"
tls = { mode = "off" }

[[hosts]]
name = "web-1"
tags = ["env:prod"]

[[hosts]]
name = "web-1"
address = "bad host"
baseline = "nope"

[[alerts.rules]]
id = "disk"
expr = "disk_used > 90 for 10m"

[[actions]]
id = "restart"
label = "Restart nginx"
command = "sudo systemctl restart nginx"
hosts = ["web-2"]
"#;
        let e = load(text).unwrap_err();
        let msgs: Vec<String> = e.0.iter().map(|e| e.to_string()).collect();
        let find = |needle: &str| {
            msgs.iter()
                .find(|m| m.contains(needle))
                .unwrap_or_else(|| panic!("no error containing {needle}: {msgs:#?}"))
        };
        assert!(find("plain HTTP").contains(":4:"));
        assert!(find("duplicate host name").contains("helmsight.toml:11:1: `hosts[1].name`"));
        assert!(find("not a valid hostname").contains(":12:"));
        assert!(find("unknown host `nope`").contains(":13:"));
        assert!(find("unknown metric").contains(":17:"));
        assert!(find("sudo -n").contains(":22:"));
        assert!(find("unknown host `web-2`").contains(":23:"));
    }

    #[test]
    fn full_example_is_valid() {
        let text = r#"
[server]
listen = "0.0.0.0:8443"
public_url = "https://monitor.example.com"
tls = { mode = "self-signed" }

[auth.oidc]
issuer = "https://id.example.com/realms/ops"
client_id = "helmsight"
client_secret = "secret:oidc"
role_map = { "ops-admins" = "admin", "ops" = "operator" }

[ssh]
user = "monitor"
identity_files = ["/etc/helmsight/id_ed25519"]

[[hosts]]
name = "web-1"
address = "10.0.0.11"
groups = ["web"]
tags = ["env:prod", "role:web"]

[[hosts]]
name = "web-2"
address = "10.0.0.12"
groups = ["web"]
tags = ["env:prod", "role:web"]
baseline = "web-1"

[[alerts.rules]]
id = "disk-full"
expr = "disk_used_pct > 90 for 10m"
severity = "critical"
tags = ["env:prod"]

[[notify]]
id = "ops"
type = "slack"
url = "secret:slack-webhook"

[[notify]]
id = "mail"
type = "email"
smtp_host = "smtp.example.com"
from = "helmsight@example.com"
to = ["ops@example.com"]
smtp_password = "env:SMTP_PASSWORD"

[[certs]]
endpoint = "example.com:443"

[[actions]]
id = "restart-nginx"
label = "Restart nginx"
command = "sudo -n /usr/bin/systemctl restart nginx"
groups = ["web"]

[prometheus]
enabled = true
token = "secret:prometheus"
"#;
        let l = load(text).unwrap_or_else(|e| panic!("{e}"));
        assert!(l.warnings.is_empty(), "{:?}", l.warnings);
        assert!(l.config.tls_enabled());
        assert_eq!(l.config.hosts.len(), 2);
        assert_eq!(l.config.actions[0].targets.groups, ["web"]);
        assert_eq!(l.config.certs[0].server_name(), "example.com");
    }

    #[test]
    fn plaintext_secrets_warn() {
        let l = load("[prometheus]\nenabled = true\ntoken = \"abc\"\n").unwrap();
        assert_eq!(l.warnings.len(), 1);
    }

    #[test]
    fn hosts_file_and_ssh_config_import() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("hosts.toml"),
            "[[hosts]]\nname = \"db-1\"\naddress = \"10.0.1.1\"\n\n[[hosts]]\nname = \"db 2\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("ssh_config"),
            "Host app-1\n  HostName 10.0.2.1\n  User deploy\nHost app-2\n  ProxyJump bastion\nHost other\n",
        )
        .unwrap();
        let main = format!(
            "hosts_file = \"hosts.toml\"\n[ssh_config_import]\npath = \"{}\"\nhosts = [\"app-*\"]\ntags = [\"imported\"]\n",
            dir.path().join("ssh_config").display()
        );
        let e = Config::from_str_at(&main, &dir.path().join("helmsight.toml")).unwrap_err();
        // Invalid name, and the address defaults to the (invalid) name.
        assert_eq!(e.0.len(), 2, "{e}");
        assert!(e.0.iter().all(|err| err.file.ends_with("hosts.toml")));
        assert_eq!(e.0[0].line, Some(6));

        std::fs::write(
            dir.path().join("hosts.toml"),
            "[[hosts]]\nname = \"db-1\"\naddress = \"10.0.1.1\"\n",
        )
        .unwrap();
        let l = Config::from_str_at(&main, &dir.path().join("helmsight.toml")).unwrap();
        let names: Vec<&str> = l.config.hosts.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, ["db-1", "app-1"]);
        assert_eq!(l.config.hosts[1].user.as_deref(), Some("deploy"));
        assert_eq!(l.config.hosts[1].tags, ["imported"]);
        assert!(l.warnings.iter().any(|w| w.contains("ProxyJump")));
    }
}
