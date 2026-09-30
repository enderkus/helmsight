//! SSH execution with one persistent session per host.

use crate::hostkeys::{HostKeys, Verdict};
use crate::{ExecOutput, Executor, HostKeyInfo, STDERR_LIMIT, TransportError};
use russh::ChannelMsg;
use russh::client::{self, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;

/// Private keys and agent settings used to authenticate.
#[derive(Debug, Default)]
pub struct Credentials {
    keys: Vec<(PathBuf, Arc<PrivateKey>)>,
    use_agent: bool,
}

impl Credentials {
    /// Loads private key files. Encrypted keys are not supported directly;
    /// load them into ssh-agent instead. Returns warnings for unusable files.
    pub fn load(files: &[PathBuf], use_agent: bool) -> (Self, Vec<String>) {
        let mut keys = Vec::new();
        let mut warnings = Vec::new();
        for f in files {
            let path = common::config::expand_home(f);
            match load_key(&path) {
                Ok(k) => keys.push((path, Arc::new(k))),
                Err(e) => warnings.push(e),
            }
        }
        (Self { keys, use_agent }, warnings)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && !self.use_agent
    }
}

fn load_key(path: &Path) -> Result<PrivateKey, String> {
    match russh::keys::load_secret_key(path, None) {
        Ok(k) => Ok(k),
        Err(russh::keys::Error::KeyIsEncrypted) => Err(format!(
            "{} is passphrase-protected; add it to ssh-agent and enable `use_agent`",
            path.display()
        )),
        Err(e) => Err(format!("cannot load {}: {e}", path.display())),
    }
}

/// Connection parameters for one host.
#[derive(Debug, Clone)]
pub struct SshTargetConfig {
    pub address: String,
    pub port: u16,
    pub user: String,
    /// Host-specific key, tried before the shared credentials.
    pub identity_file: Option<PathBuf>,
    pub connect_timeout: Duration,
    pub keepalive: Duration,
}

struct Client {
    hostkeys: Arc<HostKeys>,
    address: String,
    port: u16,
    verdict: Arc<Mutex<Option<Verdict>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
            .unwrap_or(0);
        let v = self
            .hostkeys
            .verify(&self.address, self.port, &key.public_key(), now);
        let ok = matches!(v, Verdict::Trusted | Verdict::Learned);
        if let Verdict::Learned = v {
            tracing::info!(host = %self.address, port = self.port, "trusted new host key (accept_new_host_keys)");
        }
        if let Ok(mut slot) = self.verdict.lock() {
            *slot = Some(v);
        }
        Ok(ok)
    }
}

/// A host reachable over SSH. The session is opened lazily and reused for
/// every execution until it breaks.
pub struct SshTarget {
    cfg: SshTargetConfig,
    creds: Arc<Credentials>,
    hostkeys: Arc<HostKeys>,
    own_key: Option<Arc<PrivateKey>>,
    session: AsyncMutex<Option<Arc<Handle<Client>>>>,
}

impl std::fmt::Debug for SshTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshTarget")
            .field("address", &self.cfg.address)
            .field("port", &self.cfg.port)
            .field("user", &self.cfg.user)
            .finish()
    }
}

impl SshTarget {
    pub fn new(
        cfg: SshTargetConfig,
        creds: Arc<Credentials>,
        hostkeys: Arc<HostKeys>,
    ) -> (Self, Option<String>) {
        let (own_key, warning) = match &cfg.identity_file {
            Some(p) => match load_key(&common::config::expand_home(p)) {
                Ok(k) => (Some(Arc::new(k)), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, None),
        };
        (
            Self {
                cfg,
                creds,
                hostkeys,
                own_key,
                session: AsyncMutex::new(None),
            },
            warning,
        )
    }

    pub fn address(&self) -> &str {
        &self.cfg.address
    }

    pub fn port(&self) -> u16 {
        self.cfg.port
    }

    async fn session(&self) -> Result<Arc<Handle<Client>>, TransportError> {
        let mut guard = self.session.lock().await;
        if let Some(h) = guard.as_ref()
            && !h.is_closed()
        {
            return Ok(h.clone());
        }
        *guard = None;
        let h = Arc::new(self.connect().await?);
        *guard = Some(h.clone());
        Ok(h)
    }

    async fn connect(&self) -> Result<Handle<Client>, TransportError> {
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(self.cfg.keepalive),
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        });
        let verdict = Arc::new(Mutex::new(None));
        let handler = Client {
            hostkeys: self.hostkeys.clone(),
            address: self.cfg.address.clone(),
            port: self.cfg.port,
            verdict: verdict.clone(),
        };
        let addr = (self.cfg.address.clone(), self.cfg.port);
        let connected = tokio::time::timeout(
            self.cfg.connect_timeout,
            client::connect(config, addr, handler),
        )
        .await;
        let mut handle = match connected {
            Err(_) => {
                return Err(TransportError::Unreachable {
                    message: format!(
                        "no response within {}s from {}:{}",
                        self.cfg.connect_timeout.as_secs(),
                        self.cfg.address,
                        self.cfg.port
                    ),
                });
            }
            Ok(Err(e)) => {
                let v = verdict.lock().ok().and_then(|mut v| v.take());
                return Err(match v {
                    Some(Verdict::Unknown(k)) => TransportError::HostKeyUnknown {
                        key: HostKeyInfo::from(&k),
                    },
                    Some(Verdict::Changed(k)) => TransportError::HostKeyChanged {
                        key: HostKeyInfo::from(&k),
                    },
                    _ => classify_connect_error(&e),
                });
            }
            Ok(Ok(h)) => h,
        };
        let auth = tokio::time::timeout(self.cfg.connect_timeout, self.authenticate(&mut handle))
            .await
            .map_err(|_| TransportError::AuthFailed {
                message: "authentication did not complete in time".into(),
            })?;
        auth?;
        Ok(handle)
    }

    async fn authenticate(&self, handle: &mut Handle<Client>) -> Result<(), TransportError> {
        let user = self.cfg.user.clone();
        let rsa_hash = handle
            .best_supported_rsa_hash()
            .await
            .ok()
            .flatten()
            .flatten();
        let mut tried = 0usize;
        let keys = self
            .own_key
            .iter()
            .cloned()
            .chain(self.creds.keys.iter().map(|(_, k)| k.clone()));
        for key in keys {
            tried += 1;
            let r = handle
                .authenticate_publickey(user.clone(), PrivateKeyWithHashAlg::new(key, rsa_hash))
                .await
                .map_err(|e| TransportError::Protocol {
                    message: e.to_string(),
                })?;
            if r.success() {
                return Ok(());
            }
        }
        let mut agent_note = String::new();
        if self.creds.use_agent {
            match AgentClient::connect_env().await {
                Ok(mut agent) => {
                    let ids = agent.request_identities().await.unwrap_or_default();
                    for id in ids {
                        tried += 1;
                        let pk = id.public_key().into_owned();
                        let r = handle
                            .authenticate_publickey_with(user.clone(), pk, rsa_hash, &mut agent)
                            .await;
                        if matches!(r, Ok(ref a) if a.success()) {
                            return Ok(());
                        }
                    }
                }
                Err(_) => agent_note = "; ssh-agent not available".into(),
            }
        }
        Err(TransportError::AuthFailed {
            message: if tried == 0 {
                format!("no SSH keys configured{agent_note}")
            } else {
                format!(
                    "server rejected all {tried} key(s) for user `{}`{agent_note}",
                    self.cfg.user
                )
            },
        })
    }

    async fn run(
        &self,
        handle: &Handle<Client>,
        command: &str,
        stdin: Option<&[u8]>,
        limit: usize,
    ) -> Result<ExecOutput, TransportError> {
        let start = Instant::now();
        let proto = |e: russh::Error| TransportError::Protocol {
            message: e.to_string(),
        };
        let mut ch = handle.channel_open_session().await.map_err(proto)?;
        ch.exec(true, command).await.map_err(proto)?;
        if let Some(input) = stdin {
            ch.data(input).await.map_err(proto)?;
        }
        ch.eof().await.map_err(proto)?;
        let mut out = ExecOutput::default();
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    let room = limit.saturating_sub(out.stdout.len());
                    out.stdout
                        .extend_from_slice(data.get(..data.len().min(room)).unwrap_or(&[]));
                    if data.len() > room {
                        out.truncated = true;
                        let _ = ch.close().await;
                        break;
                    }
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    let room = STDERR_LIMIT.saturating_sub(out.stderr.len());
                    out.stderr
                        .extend_from_slice(data.get(..data.len().min(room)).unwrap_or(&[]));
                }
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        out.duration = start.elapsed();
        Ok(out)
    }
}

fn classify_connect_error(e: &russh::Error) -> TransportError {
    let message = e.to_string();
    match e {
        russh::Error::IO(_) | russh::Error::ConnectionTimeout | russh::Error::Disconnect => {
            TransportError::Unreachable { message }
        }
        _ => TransportError::Protocol { message },
    }
}

#[async_trait::async_trait]
impl Executor for SshTarget {
    async fn exec(
        &self,
        command: &str,
        stdin: Option<&[u8]>,
        limit: usize,
        timeout: Duration,
    ) -> Result<ExecOutput, TransportError> {
        // A cached session may have died silently; retry once on a fresh one.
        for attempt in 0..2 {
            let handle = self.session().await?;
            match tokio::time::timeout(timeout, self.run(&handle, command, stdin, limit)).await {
                Ok(Ok(out)) => return Ok(out),
                Ok(Err(e)) => {
                    self.reset().await;
                    if attempt == 1 {
                        return Err(e);
                    }
                }
                Err(_) => {
                    // Drop the session so the remote command gets SIGHUP.
                    self.reset().await;
                    return Err(TransportError::Timeout {
                        secs: timeout.as_secs(),
                    });
                }
            }
        }
        Err(TransportError::Protocol {
            message: "unreachable retry state".into(),
        })
    }

    async fn reset(&self) {
        let old = self.session.lock().await.take();
        if let Some(h) = old {
            let _ = h
                .disconnect(russh::Disconnect::ByApplication, "", "en")
                .await;
        }
    }
}
