//! Implementations of the CLI subcommands.

use common::config::{Config, Loaded};
use common::secrets::SecretKey;
use common::{PRODUCT_NAME, Role};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use store::Store;
use store::users::UserUpdate;

type Res = Result<(), String>;

fn default_config_path() -> PathBuf {
    let system = PathBuf::from(format!("/etc/{PRODUCT_NAME}/{PRODUCT_NAME}.toml"));
    if system.is_file() {
        system
    } else {
        PathBuf::from(format!("{PRODUCT_NAME}.toml"))
    }
}

fn config_path(given: Option<PathBuf>) -> PathBuf {
    given.unwrap_or_else(default_config_path)
}

fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1).map(|u| u == "0"))
        })
        .unwrap_or(false)
}

fn default_local_data_dir() -> PathBuf {
    if is_root() {
        return PathBuf::from(format!("/var/lib/{PRODUCT_NAME}"));
    }
    if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join(PRODUCT_NAME);
    }
    match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join(".local/share").join(PRODUCT_NAME),
        None => PathBuf::from(format!("{PRODUCT_NAME}-data")),
    }
}

fn load(given: Option<PathBuf>) -> Result<Loaded, String> {
    let path = config_path(given);
    if !path.is_file() {
        return Err(format!(
            "configuration file {} not found; create one with `{PRODUCT_NAME} init` or pass --config",
            path.display()
        ));
    }
    let loaded = Config::load(&path).map_err(|e| e.to_string())?;
    for w in &loaded.warnings {
        tracing::warn!("{w}");
    }
    Ok(loaded)
}

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Sets the global `--data-dir` override.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

/// Opens the database and key file. Uses the configuration file when it
/// exists; otherwise the `--data-dir` override or the local-mode default,
/// so that `serve --local` installations can be administered too.
fn open_data(given: Option<PathBuf>) -> Result<(Option<Loaded>, Store, Arc<SecretKey>), String> {
    let path = config_path(given.clone());
    let (loaded, dir) = if path.is_file() {
        let l = load(given)?;
        let d = DATA_DIR
            .get()
            .cloned()
            .unwrap_or_else(|| l.config.data_dir(&l.path));
        (Some(l), d)
    } else if given.is_some() {
        return Err(format!("configuration file {} not found", path.display()));
    } else {
        let d = DATA_DIR
            .get()
            .cloned()
            .unwrap_or_else(default_local_data_dir);
        if !d.is_dir() {
            return Err(format!(
                "no configuration file ({}) and no data directory at {}; pass --config or --data-dir",
                path.display(),
                d.display()
            ));
        }
        (None, d)
    };
    server::app::create_private_dir(&dir)?;
    let key = SecretKey::load_or_create(&dir.join("secret.key")).map_err(|e| e.to_string())?;
    let store = Store::open(&dir.join(format!("{PRODUCT_NAME}.db"))).map_err(|e| e.to_string())?;
    Ok((loaded, store, Arc::new(key)))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}

pub async fn serve(
    config: Option<PathBuf>,
    local: bool,
    listen: Option<String>,
    data_dir: Option<PathBuf>,
) -> Res {
    let path = config_path(config.clone());
    let (mut cfg, cfg_path, default_dir) = if local && config.is_none() && !path.is_file() {
        (Config::default(), None, default_local_data_dir())
    } else {
        let loaded = load(config)?;
        let dir = loaded.config.data_dir(&loaded.path);
        (loaded.config, Some(loaded.path), dir)
    };
    if let Some(l) = listen {
        if l.parse::<std::net::SocketAddr>().is_err() {
            return Err(format!("`{l}` is not an address such as 127.0.0.1:8080"));
        }
        cfg.server.listen = l;
    }
    let data_dir = data_dir.unwrap_or(default_dir);
    if local && cfg!(not(target_os = "linux")) {
        tracing::warn!("local mode reads /proc and is only meaningful on Linux");
    }
    tracing::info!(
        version = common::VERSION,
        data_dir = %data_dir.display(),
        hosts = if local { 1 } else { cfg.hosts.len() },
        "starting {PRODUCT_NAME}"
    );
    server::init_crypto();
    let app = server::App::build(server::BuildOptions {
        config: cfg,
        config_path: cfg_path,
        data_dir,
        local,
    })?;
    server::serve(app, shutdown_signal()).await
}

fn prompt(question: &str, default: &str) -> Result<String, String> {
    if default.is_empty() {
        print!("{question}: ");
    } else {
        print!("{question} [{default}]: ");
    }
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let v = line.trim();
    Ok(if v.is_empty() {
        default.to_string()
    } else {
        v.to_string()
    })
}

fn read_password(username: &str, from_stdin: bool) -> Result<String, String> {
    if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        let pw = line.trim_end_matches(['\n', '\r']).to_string();
        server::auth::password::check_policy(&pw, username)?;
        return Ok(pw);
    }
    loop {
        let pw = rpassword::prompt_password("Password: ").map_err(|e| e.to_string())?;
        if let Err(e) = server::auth::password::check_policy(&pw, username) {
            eprintln!("{e}");
            continue;
        }
        let again = rpassword::prompt_password("Repeat password: ").map_err(|e| e.to_string())?;
        if pw != again {
            eprintln!("Passwords do not match.");
            continue;
        }
        return Ok(pw);
    }
}

fn config_template(listen: &str, data_dir: &str, user: &str, key: &str) -> String {
    let key_line = if key.is_empty() {
        "# identity_files = [\"/etc/helmsight/id_ed25519\"]".to_string()
    } else {
        format!("identity_files = [\"{key}\"]")
    };
    format!(
        r#"# {PRODUCT_NAME} configuration. See examples/helmsight.toml for every option.

[server]
listen = "{listen}"
data_dir = "{data_dir}"
# public_url = "https://monitor.example.com"

[ssh]
# Unprivileged account on the monitored hosts (see docs/security.md).
user = "{user}"
{key_line}
use_agent = true
# Trust keys of hosts seen for the first time. Changed keys are always refused.
accept_new_host_keys = false

# [[hosts]]
# name = "web-1"
# address = "10.0.0.11"
# groups = ["web"]
# tags = ["env:prod", "role:web"]

# [[alerts.rules]]
# id = "disk-full"
# expr = "disk_used_pct > 90 for 10m"
# severity = "critical"
"#
    )
}

pub async fn init(config: Option<PathBuf>, force: bool) -> Res {
    let path = config_path(config);
    if path.exists() && !force {
        return Err(format!(
            "{} already exists; use --force to overwrite it",
            path.display()
        ));
    }
    println!("Creating {}.\n", path.display());
    let listen = prompt("Listen address", "127.0.0.1:8080")?;
    let default_dir = if is_root() {
        format!("/var/lib/{PRODUCT_NAME}")
    } else {
        "data".to_string()
    };
    let data_dir = prompt("Data directory", &default_dir)?;
    let user = prompt("SSH user on monitored hosts", "monitor")?;
    let default_key = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".ssh/id_ed25519"))
        .filter(|p| p.is_file())
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let key = prompt(
        "SSH private key file (empty to use ssh-agent only)",
        &default_key,
    )?;
    let text = config_template(&listen, &data_dir, &user, &key);
    if let Err(e) = Config::from_str_at(&text, &path) {
        return Err(format!(
            "the answers produce an invalid configuration:\n{e}"
        ));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("writing {}: {e}", path.display()))?;
    println!("Wrote {}.", path.display());

    let (loaded, store, _key) = open_data(Some(path.clone()))?;
    let dir = loaded
        .map(|l| l.config.data_dir(&l.path))
        .unwrap_or_default();
    println!(
        "Data directory {} ready (key file secret.key created).",
        dir.display()
    );
    if store.user_count().await.map_err(|e| e.to_string())? == 0 {
        println!("\nCreate the first administrator.");
        let name = prompt("User name", "admin")?;
        let pw = read_password(&name, false)?;
        let hash = tokio::task::spawn_blocking(move || server::auth::password::hash(&pw))
            .await
            .map_err(|e| e.to_string())??;
        store
            .create_user(name.clone(), Some(hash), Role::Admin, None, None)
            .await
            .map_err(|e| e.to_string())?;
        store
            .audit(
                "cli",
                "user.create",
                Some(&name),
                serde_json::json!({"role": "admin"}),
            )
            .await
            .map_err(|e| e.to_string())?;
        println!("Administrator `{name}` created.");
    }
    println!(
        "\nNext steps:\n  1. Add hosts to {}\n  2. {PRODUCT_NAME} hosts test --trust --config {}\n  3. {PRODUCT_NAME} serve --config {}",
        path.display(),
        path.display(),
        path.display()
    );
    Ok(())
}

pub async fn user_add(config: Option<PathBuf>, username: String, role: String, stdin: bool) -> Res {
    let (_, store, _) = open_data(config)?;
    let role: Role = role.parse()?;
    let pw = read_password(&username, stdin)?;
    let hash = tokio::task::spawn_blocking(move || server::auth::password::hash(&pw))
        .await
        .map_err(|e| e.to_string())??;
    let u = store
        .create_user(username.clone(), Some(hash), role, None, None)
        .await
        .map_err(|e| e.to_string())?;
    store
        .audit(
            "cli",
            "user.create",
            Some(&u.username),
            serde_json::json!({"role": role}),
        )
        .await
        .map_err(|e| e.to_string())?;
    println!("User `{}` created with role {role}.", u.username);
    Ok(())
}

pub async fn user_remove(config: Option<PathBuf>, username: String) -> Res {
    let (_, store, _) = open_data(config)?;
    let u = store
        .user_by_name(username.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no user `{username}`"))?;
    if u.role == Role::Admin
        && !u.disabled
        && store.admin_count().await.map_err(|e| e.to_string())? <= 1
    {
        return Err("refusing to remove the last active administrator".into());
    }
    store.delete_user(u.id).await.map_err(|e| e.to_string())?;
    store
        .audit(
            "cli",
            "user.delete",
            Some(&u.username),
            serde_json::json!({}),
        )
        .await
        .map_err(|e| e.to_string())?;
    println!("User `{}` removed.", u.username);
    Ok(())
}

pub async fn user_reset(
    config: Option<PathBuf>,
    username: String,
    stdin: bool,
    reset_totp: bool,
) -> Res {
    let (_, store, _) = open_data(config)?;
    let u = store
        .user_by_name(username.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no user `{username}`"))?;
    let pw = read_password(&u.username, stdin)?;
    let hash = tokio::task::spawn_blocking(move || server::auth::password::hash(&pw))
        .await
        .map_err(|e| e.to_string())??;
    store
        .update_user(
            u.id,
            UserUpdate {
                password_hash: Some(hash),
                must_change_password: Some(false),
                totp: reset_totp.then_some(None),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    store
        .delete_user_sessions(u.id, None)
        .await
        .map_err(|e| e.to_string())?;
    store
        .audit(
            "cli",
            "user.password_reset",
            Some(&u.username),
            serde_json::json!({"totp_reset": reset_totp}),
        )
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Password of `{}` changed; existing sessions ended.",
        u.username
    );
    Ok(())
}

pub async fn user_list(config: Option<PathBuf>) -> Res {
    let (_, store, _) = open_data(config)?;
    let users = store.users().await.map_err(|e| e.to_string())?;
    println!(
        "{:<24} {:<9} {:<6} {:<9} SSO",
        "USER", "ROLE", "TOTP", "STATE"
    );
    for u in users {
        println!(
            "{:<24} {:<9} {:<6} {:<9} {}",
            u.username,
            u.role,
            if u.totp_enabled { "yes" } else { "no" },
            if u.disabled { "disabled" } else { "active" },
            if u.oidc_subject.is_some() { "yes" } else { "" }
        );
    }
    Ok(())
}

pub async fn hosts_test(config: Option<PathBuf>, names: Vec<String>, trust: bool) -> Res {
    let loaded = load(config)?;
    let cfg = &loaded.config;
    let dir = cfg.data_dir(&loaded.path);
    server::app::create_private_dir(&dir)?;
    let hostkeys = Arc::new(transport::HostKeys::new(
        dir.join("known_hosts"),
        cfg.ssh
            .known_hosts_files
            .iter()
            .map(|p| common::config::expand_home(p))
            .collect(),
        cfg.ssh.accept_new_host_keys,
    ));
    let (creds, warnings) =
        transport::Credentials::load(&cfg.ssh.identity_files, cfg.ssh.use_agent);
    for w in warnings {
        eprintln!("warning: {w}");
    }
    let creds = Arc::new(creds);
    let hosts: Vec<_> = cfg
        .hosts
        .iter()
        .filter(|h| names.is_empty() || names.contains(&h.name))
        .cloned()
        .collect();
    if hosts.is_empty() {
        return Err("no matching hosts in the configuration".into());
    }
    let mut failures = 0;
    println!("{:<24} {:<18} DETAIL", "HOST", "STATUS");
    for h in hosts {
        let (target, warning) = transport::SshTarget::new(
            transport::SshTargetConfig {
                address: h.address().to_string(),
                port: h.port(),
                user: h.user.clone().unwrap_or_else(|| cfg.ssh.user.clone()),
                identity_file: h.identity_file.clone(),
                connect_timeout: cfg.ssh.connect_timeout.0,
                keepalive: cfg.ssh.keepalive.0,
            },
            creds.clone(),
            hostkeys.clone(),
        );
        if let Some(w) = warning {
            eprintln!("warning: {}: {w}", h.name);
        }
        let mut attempt = 0;
        loop {
            attempt += 1;
            match probe(&target).await {
                Ok(detail) => {
                    println!("{:<24} {:<18} {detail}", h.name, "ok");
                    break;
                }
                Err(transport::TransportError::HostKeyUnknown { key }) if trust && attempt == 1 => {
                    println!(
                        "{:<24} {:<18} {} {} ({})",
                        h.name, "host key unknown", key.algorithm, key.fingerprint, key.endpoint
                    );
                    let a = prompt(
                        "  Trust this key? Compare with `ssh-keygen -lf` on the host (y/N)",
                        "N",
                    )?;
                    if a.eq_ignore_ascii_case("y") || a.eq_ignore_ascii_case("yes") {
                        hostkeys.approve(h.address(), h.port(), &key.fingerprint)?;
                        println!("  trusted; retrying");
                        continue;
                    }
                    failures += 1;
                    break;
                }
                Err(e) => {
                    let detail = match &e {
                        transport::TransportError::HostKeyUnknown { key } => format!(
                            "{} {} not trusted; rerun with --trust or approve it in the UI",
                            key.algorithm, key.fingerprint
                        ),
                        transport::TransportError::HostKeyChanged { key } => format!(
                            "now {} (was {}); verify the host before approving the new key in the UI",
                            key.fingerprint,
                            key.previous.as_deref().unwrap_or("?")
                        ),
                        other => other.to_string(),
                    };
                    println!(
                        "{:<24} {:<18} {detail}",
                        h.name,
                        e.state().replace('_', " ")
                    );
                    failures += 1;
                    break;
                }
            }
        }
    }
    if failures > 0 {
        Err(format!("{failures} host(s) failed"))
    } else {
        Ok(())
    }
}

async fn probe(t: &transport::SshTarget) -> Result<String, transport::TransportError> {
    use transport::Executor;
    let req = collect::ScriptRequest::new(
        vec![collect::Group::Basics, collect::Group::Inventory],
        0,
        common::secrets::random_bytes(),
    );
    let out = t
        .exec(
            "sh -s",
            Some(req.render().as_bytes()),
            collect::MAX_OUTPUT_BYTES,
            Duration::from_secs(60),
        )
        .await?;
    let mut sampler = collect::Sampler::new();
    let c = sampler.ingest(
        &String::from_utf8_lossy(&out.stdout),
        &req,
        0.0,
        store::now(),
    );
    if !c.complete {
        return Ok(format!(
            "connected, but the script did not complete (exit {:?}); is `sh` POSIX on this host?",
            out.exit_status
        ));
    }
    let b = c.basics.unwrap_or_default();
    let os = c
        .inventory
        .and_then(|i| i.os.pretty_name)
        .unwrap_or_else(|| "unknown OS".into());
    let mut s = format!(
        "{os}; user {} ({} ms)",
        b.user.unwrap_or_else(|| "?".into()),
        out.duration.as_millis()
    );
    if b.uid == Some(0) {
        s.push_str("; warning: connected as root, use an unprivileged account");
    }
    Ok(s)
}

pub fn config_check(config: Option<PathBuf>) -> Res {
    let path = config_path(config);
    match Config::load(&path) {
        Ok(l) => {
            let c = &l.config;
            println!(
                "{}: ok ({} hosts, {} alert rules, {} notification channels, {} actions, {} certificate checks)",
                path.display(),
                c.hosts.len(),
                c.alerts.rules.len(),
                c.notify.len(),
                c.actions.len(),
                c.certs.len()
            );
            for w in &l.warnings {
                println!("warning: {w}");
            }
            check_secrets(&l)?;
            Ok(())
        }
        Err(e) => {
            eprintln!("{e}");
            Err(format!("{} has {} error(s)", path.display(), e.0.len()))
        }
    }
}

/// Reports `secret:` references that are not stored yet.
fn check_secrets(l: &Loaded) -> Res {
    let c = &l.config;
    let mut refs: Vec<(String, &common::secrets::SecretRef)> = Vec::new();
    for (i, n) in c.notify.iter().enumerate() {
        if let Some(u) = &n.url {
            refs.push((format!("notify[{i}].url"), u));
        }
        if let Some(p) = &n.smtp_password {
            refs.push((format!("notify[{i}].smtp_password"), p));
        }
    }
    if let Some(t) = &c.prometheus.token {
        refs.push(("prometheus.token".into(), t));
    }
    if let Some(s) = c.auth.oidc.as_ref().and_then(|o| o.client_secret.as_ref()) {
        refs.push(("auth.oidc.client_secret".into(), s));
    }
    let stored: Vec<_> = refs
        .iter()
        .filter(|(_, r)| r.stored_name().is_some())
        .collect();
    if stored.is_empty() {
        return Ok(());
    }
    let dir = c.data_dir(&l.path);
    let db = dir.join(format!("{PRODUCT_NAME}.db"));
    if !db.is_file() {
        println!(
            "note: database {} does not exist yet; stored secrets cannot be checked",
            db.display()
        );
        return Ok(());
    }
    let store = Store::open(&db).map_err(|e| e.to_string())?;
    let names: Vec<String> = store
        .write_sync(|conn| store::secrets::names(conn))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let mut missing = 0;
    for (key, r) in stored {
        if let Some(name) = r.stored_name()
            && !names.iter().any(|n| n == name)
        {
            println!(
                "error: `{key}` refers to secret `{name}`, which is not stored; run `{PRODUCT_NAME} secret set {name}`"
            );
            missing += 1;
        }
    }
    if missing > 0 {
        return Err(format!("{missing} secret(s) missing"));
    }
    Ok(())
}

fn valid_secret_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

pub async fn secret_set(config: Option<PathBuf>, name: String, stdin: bool) -> Res {
    if !valid_secret_name(&name) {
        return Err("secret names may contain letters, digits, `.`, `-` and `_`".into());
    }
    let (_, store, key) = open_data(config)?;
    let value = if stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        line.trim_end_matches(['\n', '\r']).to_string()
    } else {
        rpassword::prompt_password(format!("Value for `{name}`: ")).map_err(|e| e.to_string())?
    };
    if value.is_empty() {
        return Err("empty value".into());
    }
    let n = name.clone();
    store
        .write(move |c| store::secrets::set(c, &key, &n, &value))
        .await
        .map_err(|e| e.to_string())?;
    store
        .audit("cli", "secret.set", Some(&name), serde_json::json!({}))
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Secret `{name}` stored (encrypted). Reference it as \"secret:{name}\" and restart the server."
    );
    Ok(())
}

pub async fn secret_list(config: Option<PathBuf>) -> Res {
    let (_, store, _) = open_data(config)?;
    let names = store
        .read(store::secrets::names)
        .await
        .map_err(|e| e.to_string())?;
    for (n, ts) in names {
        println!("{n}\t(updated {})", fmt_ts(ts));
    }
    Ok(())
}

pub async fn secret_delete(config: Option<PathBuf>, name: String) -> Res {
    let (_, store, _) = open_data(config)?;
    let n = name.clone();
    let deleted = store
        .write(move |c| store::secrets::delete(c, &n))
        .await
        .map_err(|e| e.to_string())?;
    if !deleted {
        return Err(format!("no secret `{name}`"));
    }
    store
        .audit("cli", "secret.delete", Some(&name), serde_json::json!({}))
        .await
        .map_err(|e| e.to_string())?;
    println!("Secret `{name}` deleted.");
    Ok(())
}

pub async fn audit_verify(config: Option<PathBuf>) -> Res {
    let (_, store, _) = open_data(config)?;
    let s = store
        .verify_audit_chain()
        .await
        .map_err(|e| e.to_string())?;
    match s.broken_at {
        None => {
            println!("audit log intact ({} entries)", s.entries);
            Ok(())
        }
        Some(id) => Err(format!(
            "audit log hash chain broken at entry {id}; the database was modified outside {PRODUCT_NAME}"
        )),
    }
}

fn fmt_ts(ts: i64) -> String {
    let d = Duration::from_secs(u64::try_from(store::now() - ts).unwrap_or(0));
    let s = d.as_secs();
    if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3600)
    } else {
        format!("{}d ago", s / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn template_is_valid() {
        let t = config_template("127.0.0.1:8080", "data", "monitor", "/root/.ssh/id_ed25519");
        Config::from_str_at(&t, Path::new("/tmp/x.toml")).unwrap();
        let t = config_template("0.0.0.0:8443", "/var/lib/helmsight", "monitor", "");
        let l = Config::from_str_at(&t, Path::new("/tmp/x.toml")).unwrap();
        assert!(l.config.tls_enabled());
    }
}
