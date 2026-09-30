//! Application context shared by the HTTP layer and background workers.

use crate::auth::oidc::Oidc;
use crate::auth::ratelimit::RateLimiter;
use crate::auth::totp::ReplayGuard;
use crate::state::{Event, Fleet, HostState};
use alerts::notify::{Channel, Notifier, SmtpSettings};
use common::config::{Config, HostConfig, NotifyConfig};
use common::secrets::{SecretKey, SecretLookup, SecretRef};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use store::Store;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use transport::{Credentials, Executor, HostKeys, LocalExecutor, SshTarget, SshTargetConfig};
use zeroize::Zeroizing;

pub struct App {
    pub cfg: Config,
    pub config_path: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub local_mode: bool,
    pub store: Store,
    pub fleet: Fleet,
    pub hostkeys: Arc<HostKeys>,
    pub key: Arc<SecretKey>,
    pub events: broadcast::Sender<Event>,
    pub executors: BTreeMap<String, Arc<dyn Executor>>,
    pub shutdown: CancellationToken,
    pub limiter: RateLimiter,
    pub totp_guard: ReplayGuard,
    /// Hash of the one-time token that allows creating the first admin.
    pub setup_token: Mutex<Option<String>>,
    pub oidc: Option<Oidc>,
    pub notifier: Notifier,
    pub channels: Vec<Channel>,
    pub prometheus_token: Option<Zeroizing<String>>,
    pub tls: bool,
    pub secure_cookies: bool,
    pub running_actions: Mutex<HashSet<String>>,
    /// Hosts whose collection loop should retry immediately.
    pub wake: Mutex<HashSet<String>>,
    pub started_at: i64,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("data_dir", &self.data_dir)
            .finish_non_exhaustive()
    }
}

/// Inputs to [`App::build`].
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub local: bool,
}

/// Looks up `secret:<name>` references in the database.
pub struct DbSecrets<'a> {
    pub store: &'a Store,
    pub key: &'a SecretKey,
}

impl SecretLookup for DbSecrets<'_> {
    fn lookup(&self, name: &str) -> Option<Zeroizing<String>> {
        self.store
            .write_sync(|c| store::secrets::get(c, self.key, name))
            .ok()
            .flatten()
    }
}

fn local_hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
                .take(64)
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".into())
}

fn resolve(
    r: &SecretRef,
    lookup: &dyn SecretLookup,
    key_path: &str,
) -> Result<Zeroizing<String>, String> {
    r.resolve(lookup).map_err(|e| format!("`{key_path}`: {e}"))
}

fn build_channel(i: usize, n: &NotifyConfig, lookup: &dyn SecretLookup) -> Result<Channel, String> {
    let url = match &n.url {
        Some(u) => Some(resolve(u, lookup, &format!("notify[{i}].url"))?),
        None => None,
    };
    let mut headers = Vec::new();
    for (k, v) in &n.headers {
        headers.push((
            k.clone(),
            resolve(v, lookup, &format!("notify[{i}].headers.{k}"))?,
        ));
    }
    let smtp = match n.kind {
        common::config::NotifyKind::Email => Some(SmtpSettings {
            host: n.smtp_host.clone().unwrap_or_default(),
            port: n.smtp_port,
            security: n.smtp_security.clone().unwrap_or_else(|| "starttls".into()),
            username: n.smtp_username.clone(),
            password: match &n.smtp_password {
                Some(p) => Some(resolve(p, lookup, &format!("notify[{i}].smtp_password"))?),
                None => None,
            },
            from: n.from.clone().unwrap_or_default(),
            to: n.to.clone(),
        }),
        _ => None,
    };
    Ok(Channel {
        id: n.id.clone(),
        kind: n.kind,
        min_severity: n.min_severity,
        scope: n.scope.clone(),
        url,
        headers,
        smtp,
    })
}

impl App {
    pub fn build(opts: BuildOptions) -> Result<Arc<App>, String> {
        let BuildOptions {
            mut config,
            config_path,
            data_dir,
            local,
        } = opts;
        create_private_dir(&data_dir)?;
        let key = Arc::new(
            SecretKey::load_or_create(&data_dir.join("secret.key")).map_err(|e| e.to_string())?,
        );
        let db_path = data_dir.join(format!("{}.db", common::PRODUCT_NAME));
        let store =
            Store::open(&db_path).map_err(|e| format!("opening {}: {e}", db_path.display()))?;
        let hostkeys = Arc::new(HostKeys::new(
            data_dir.join("known_hosts"),
            config
                .ssh
                .known_hosts_files
                .iter()
                .map(|p| common::config::expand_home(p))
                .collect(),
            config.ssh.accept_new_host_keys,
        ));

        let mut executors: BTreeMap<String, Arc<dyn Executor>> = BTreeMap::new();
        if local {
            let name = local_hostname();
            config.hosts = vec![HostConfig {
                name: name.clone(),
                address: Some("localhost".into()),
                groups: vec!["local".into()],
                ..HostConfig::default()
            }];
            executors.insert(name, Arc::new(LocalExecutor));
        } else {
            let (creds, warnings) =
                Credentials::load(&config.ssh.identity_files, config.ssh.use_agent);
            for w in warnings {
                tracing::warn!("{w}");
            }
            if creds.is_empty()
                && config.hosts.iter().all(|h| h.identity_file.is_none())
                && !config.hosts.is_empty()
            {
                tracing::warn!(
                    "no SSH identity files configured and ssh-agent disabled; hosts cannot authenticate"
                );
            }
            let creds = Arc::new(creds);
            for h in &config.hosts {
                let (target, warning) = SshTarget::new(
                    SshTargetConfig {
                        address: h.address().to_string(),
                        port: h.port(),
                        user: h.user.clone().unwrap_or_else(|| config.ssh.user.clone()),
                        identity_file: h.identity_file.clone(),
                        connect_timeout: config.ssh.connect_timeout.0,
                        keepalive: config.ssh.keepalive.0,
                    },
                    creds.clone(),
                    hostkeys.clone(),
                );
                if let Some(w) = warning {
                    tracing::warn!(host = %h.name, "{w}");
                }
                executors.insert(h.name.clone(), Arc::new(target));
            }
        }

        let fleet = Fleet::default();
        for h in &config.hosts {
            let id = store
                .write_sync(|c| {
                    c.execute(
                        "INSERT OR IGNORE INTO hosts(name, created_at) VALUES (?1, ?2)",
                        store::rusqlite::params![h.name, store::now()],
                    )?;
                    Ok(
                        c.query_row("SELECT id FROM hosts WHERE name = ?1", [&h.name], |r| {
                            r.get(0)
                        })?,
                    )
                })
                .map_err(|e| e.to_string())?;
            fleet.insert(HostState::new(h.clone(), id));
        }

        let lookup = DbSecrets {
            store: &store,
            key: &key,
        };
        let mut channels = Vec::new();
        for (i, n) in config.notify.iter().enumerate() {
            channels.push(build_channel(i, n, &lookup)?);
        }
        let prometheus_token = match (&config.prometheus.enabled, &config.prometheus.token) {
            (true, Some(t)) => Some(resolve(t, &lookup, "prometheus.token")?),
            _ => None,
        };
        let oidc = match &config.auth.oidc {
            Some(o) => {
                let secret = match &o.client_secret {
                    Some(s) => Some(resolve(s, &lookup, "auth.oidc.client_secret")?),
                    None => None,
                };
                let public = config.server.public_url.clone().unwrap_or_default();
                Some(Oidc::new(o.clone(), &public, secret)?)
            }
            None => None,
        };

        let tls = config.tls_enabled();
        let secure_cookies = tls
            || config
                .server
                .public_url
                .as_deref()
                .is_some_and(|u| u.to_ascii_lowercase().starts_with("https://"));
        let notifier = Notifier::new(&format!("{}/{}", common::PRODUCT_NAME, common::VERSION))?;
        let (events, _) = broadcast::channel(1024);

        Ok(Arc::new(App {
            cfg: config,
            config_path,
            data_dir,
            local_mode: local,
            store,
            fleet,
            hostkeys,
            key,
            events,
            executors,
            shutdown: CancellationToken::new(),
            limiter: RateLimiter::default(),
            totp_guard: ReplayGuard::default(),
            setup_token: Mutex::new(None),
            oidc,
            notifier,
            channels,
            prometheus_token,
            tls,
            secure_cookies,
            running_actions: Mutex::new(HashSet::new()),
            wake: Mutex::new(HashSet::new()),
            started_at: store::now(),
        }))
    }

    /// Seconds without a successful collection after which data is stale.
    pub fn stale_after(&self) -> i64 {
        let i = i64::try_from(self.cfg.collect.interval.as_secs()).unwrap_or(5);
        let t = i64::try_from(self.cfg.ssh.command_timeout.as_secs()).unwrap_or(30);
        (i * 3 + t).max(30)
    }

    /// Link to a UI path, when the public URL is known.
    pub fn link(&self, path: &str) -> Option<String> {
        self.cfg
            .server
            .public_url
            .as_ref()
            .map(|u| format!("{}{path}", u.trim_end_matches('/')))
    }

    /// Asks a host's collection loop to retry now (e.g. after a host key
    /// was approved).
    pub fn wake(&self, host: &str) {
        if let Ok(mut w) = self.wake.lock() {
            w.insert(host.to_string());
        }
    }

    pub fn take_wake(&self, host: &str) -> bool {
        self.wake
            .lock()
            .map(|mut w| w.remove(host))
            .unwrap_or(false)
    }

    pub fn secrets(&self) -> DbSecrets<'_> {
        DbSecrets {
            store: &self.store,
            key: &self.key,
        }
    }
}

/// Creates a directory readable only by the current user.
pub fn create_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("creating {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    Ok(())
}
