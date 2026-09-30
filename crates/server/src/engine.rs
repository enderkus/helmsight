//! The collection engine: one loop per host that runs the collection
//! script, parses the output, updates the fleet state and persists history.

use crate::app::App;
use crate::state::{AuthSource, Conn, Event, SparkPoint, Timed};
use collect::model::Probe;
use collect::{Group, Sampler, ScriptOptions, ScriptRequest};
use std::sync::Arc;
use std::time::{Duration, Instant};
use store::inventory::Snapshot;
use tokio::sync::Semaphore;
use transport::{Executor, TransportError};

/// Command used to run the script: the script itself is sent on stdin so
/// that its contents (including the marker nonce) never appear in the
/// remote process list. The command is kept trivial so that any login
/// shell (sh, bash, zsh, fish, csh) can start it.
pub const SCRIPT_COMMAND: &str = "sh -s";

/// When each slower group is next due, as monotonic instants.
struct Due {
    medium: Instant,
    inventory: Instant,
    auth: Instant,
    updates: Instant,
    basics_done: bool,
}

impl Due {
    fn now() -> Self {
        let n = Instant::now();
        Self {
            medium: n,
            inventory: n,
            auth: n,
            updates: n,
            basics_done: false,
        }
    }
}

/// Backoff after consecutive failures. Authentication failures back off
/// quickly to a long interval so repeated attempts do not trip intrusion
/// prevention on the monitored host.
fn backoff(err: &TransportError, failures: u32, interval: Duration) -> Duration {
    let cap = Duration::from_secs(300);
    match err {
        TransportError::AuthFailed { .. } => {
            Duration::from_secs(60 * u64::from(failures.min(5))).min(cap)
        }
        TransportError::HostKeyUnknown { .. } | TransportError::HostKeyChanged { .. } => {
            Duration::from_secs(60)
        }
        _ => {
            let exp = interval.saturating_mul(1u32 << failures.min(6).saturating_sub(1));
            exp.min(cap)
        }
    }
}

/// Starts one collection task per enabled host.
pub fn start(app: Arc<App>) {
    let sem = Arc::new(Semaphore::new(app.cfg.collect.max_parallel.max(1)));
    for (name, exec) in app.executors.iter() {
        let disabled = app.cfg.host(name).is_some_and(|h| h.disabled);
        if disabled {
            continue;
        }
        let app = app.clone();
        let exec = exec.clone();
        let name = name.clone();
        let sem = sem.clone();
        tokio::spawn(async move { host_loop(app, name, exec, sem).await });
    }
}

async fn host_loop(app: Arc<App>, name: String, exec: Arc<dyn Executor>, sem: Arc<Semaphore>) {
    let col = app.cfg.collect.clone();
    let interval = col.interval.0;
    // Spread start times so hosts are not collected in lockstep.
    let jitter =
        u64::from(common::secrets::random_bytes::<2>()[0]) * interval.as_millis() as u64 / 256;
    tokio::time::sleep(Duration::from_millis(jitter)).await;

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut sampler = Sampler::new();
    let mut due = Due::now();
    let mut failures: u32 = 0;
    let mut retry_at: Option<Instant> = None;
    let started = Instant::now();
    let host_id = app.fleet.get(&name).map(|h| h.id).unwrap_or(0);
    let mut auth_since: i64 = match app.store.last_auth_failure(host_id).await {
        Ok(Some(ts)) => ts,
        _ => store::now() - 86_400,
    };

    loop {
        ticker.tick().await;
        if app.shutdown.is_cancelled() {
            return;
        }
        if app.take_wake(&name) {
            retry_at = None;
        }
        if let Some(t) = retry_at
            && Instant::now() < t
        {
            continue;
        }
        let now_i = Instant::now();
        let mut groups = vec![Group::Metrics];
        let want_medium = now_i >= due.medium;
        let want_inventory = now_i >= due.inventory;
        let want_auth = now_i >= due.auth;
        let want_updates = now_i >= due.updates;
        if want_medium {
            groups.push(Group::Medium);
        }
        if want_inventory || !due.basics_done {
            groups.push(Group::Inventory);
            groups.push(Group::Basics);
        }
        if want_auth {
            groups.push(Group::Auth);
        }
        if want_updates {
            groups.push(Group::Updates);
        }
        let mut req =
            ScriptRequest::new(groups.clone(), auth_since, common::secrets::random_bytes());
        req.options = ScriptOptions {
            dnf_updates: app.cfg.collect.dnf_updates,
        };
        let script = req.render();

        let attempt_ts = store::now();
        let result = {
            let _permit = sem.acquire().await;
            let timeout = if want_updates {
                app.cfg.ssh.command_timeout.0.max(Duration::from_secs(300))
            } else {
                app.cfg.ssh.command_timeout.0
            };
            exec.exec(
                SCRIPT_COMMAND,
                Some(script.as_bytes()),
                collect::MAX_OUTPUT_BYTES,
                timeout,
            )
            .await
        };

        let out = match result {
            Ok(out) => out,
            Err(e) => {
                failures = failures.saturating_add(1);
                sampler.reset();
                let wait = backoff(&e, failures, interval);
                retry_at = Some(Instant::now() + wait);
                let key = match &e {
                    TransportError::HostKeyUnknown { key }
                    | TransportError::HostKeyChanged { key } => Some(key.clone()),
                    _ => None,
                };
                if failures == 1 || failures.is_multiple_of(20) {
                    tracing::warn!(host = %name, error = %e, retry_in = wait.as_secs(), "collection failed");
                }
                let kind = e.state();
                let message = e.to_string();
                let changed = app.fleet.update(&name, |h| {
                    h.last_attempt = Some(attempt_ts);
                    let since = match &h.conn {
                        Conn::Failed { kind: k, since, .. } if *k == kind => *since,
                        _ => attempt_ts,
                    };
                    let prev = h.conn.clone();
                    h.conn = Conn::Failed {
                        kind,
                        message,
                        since,
                        key,
                    };
                    prev != h.conn
                });
                if matches!(
                    e,
                    TransportError::HostKeyUnknown { .. } | TransportError::HostKeyChanged { .. }
                ) && changed == Some(true)
                {
                    let _ = app.events.send(Event::HostKeys);
                }
                publish(&app, &name);
                continue;
            }
        };

        let text = String::from_utf8_lossy(&out.stdout);
        let mono = started.elapsed().as_secs_f64();
        let now = store::now();
        let c = sampler.ingest(&text, &req, mono, now);
        if !c.complete && c.metrics.is_none() {
            failures = failures.saturating_add(1);
            let hint = String::from_utf8_lossy(&out.stderr)
                .lines()
                .next()
                .map(|l| format!(": {}", l.chars().take(200).collect::<String>()))
                .unwrap_or_default();
            app.fleet.update(&name, |h| {
                h.last_attempt = Some(attempt_ts);
                h.conn = Conn::Failed {
                    kind: "unreachable",
                    message: format!(
                        "collection script produced no usable output (exit status {:?}){hint}",
                        out.exit_status
                    ),
                    since: attempt_ts,
                    key: None,
                };
            });
            retry_at = Some(
                Instant::now()
                    + backoff(
                        &TransportError::Protocol {
                            message: String::new(),
                        },
                        failures,
                        interval,
                    ),
            );
            publish(&app, &name);
            continue;
        }
        failures = 0;
        retry_at = None;
        let missing = c.missing.clone();
        let t = |g: Group| groups.contains(&g) && !c.missing.iter().any(|m| group_of(m) == Some(g));

        // Schedule the next run of each group that succeeded.
        if want_medium && t(Group::Medium) {
            due.medium = now_i + col.medium_interval.0;
        }
        if groups.contains(&Group::Inventory) && t(Group::Inventory) {
            due.inventory = now_i + col.inventory_interval.0;
            due.basics_done = true;
        }
        if want_auth && t(Group::Auth) {
            due.auth = now_i + col.auth_interval.0;
        }
        if want_updates && t(Group::Updates) {
            due.updates = now_i + col.updates_interval.0;
        }

        // Persist history.
        if let Some(m) = &c.metrics {
            let values = store::metrics::flatten(m);
            if let Err(e) = app.store.insert_sample(host_id, now, values).await {
                tracing::error!(host = %name, error = %e, "storing metrics failed");
            }
        }
        if let Some(med) = &c.medium
            && let Ok(j) = serde_json::to_string(med)
        {
            let _ = app.store.put_latest(host_id, "medium", now, j).await;
        }
        if let Some(u) = &c.updates
            && let Ok(j) = serde_json::to_string(u)
        {
            let _ = app.store.put_latest(host_id, "updates", now, j).await;
        }
        if let Some(inv) = &c.inventory {
            if let Ok(j) = serde_json::to_string(inv) {
                let _ = app.store.put_latest(host_id, "inventory", now, j).await;
            }
            let listening = c.medium.as_ref().map(|m| m.listening.clone()).or_else(|| {
                app.fleet
                    .get(&name)
                    .and_then(|h| h.medium.map(|m| m.data.listening))
            });
            let snap = Snapshot::build(inv, listening.as_ref());
            match app.store.record_snapshot(host_id, now, snap).await {
                Ok(changes) if !changes.is_empty() => {
                    tracing::info!(host = %name, changes = changes.len(), "inventory changed");
                    let _ = app.events.send(Event::Changes {
                        host: name.clone(),
                        count: changes.len(),
                    });
                }
                Ok(_) => {}
                Err(e) => tracing::error!(host = %name, error = %e, "storing inventory failed"),
            }
        }
        let mut auth_state = None;
        if let Some(a) = &c.auth {
            auth_state = Some(match a {
                Probe::Ok { source, data } => {
                    if let Some(max) = data.iter().map(|e| e.ts).max() {
                        auth_since = auth_since.max(max);
                    }
                    if let Err(e) = app.store.insert_auth_failures(host_id, data.clone()).await {
                        tracing::error!(host = %name, error = %e, "storing auth failures failed");
                    }
                    AuthSource {
                        source: Some(source.clone()),
                        unavailable: None,
                    }
                }
                Probe::Na { reason } => AuthSource {
                    source: None,
                    unavailable: Some(reason.clone()),
                },
            });
        }
        let (f1h, f24h) = if auth_state.is_some() {
            let one = app.store.auth_counts(now - 3600).await.ok();
            let day = app.store.auth_counts(now - 86_400).await.ok();
            let pick = |v: Option<Vec<(i64, i64)>>| {
                v.map(|v| {
                    v.into_iter()
                        .find(|(h, _)| *h == host_id)
                        .map(|(_, n)| u64::try_from(n).unwrap_or(0))
                        .unwrap_or(0)
                })
            };
            (pick(one), pick(day))
        } else {
            (None, None)
        };

        app.fleet.update(&name, |h| {
            h.conn = Conn::Ok;
            h.last_attempt = Some(attempt_ts);
            h.last_success = Some(now);
            h.last_duration_ms = Some(u64::try_from(out.duration.as_millis()).unwrap_or(u64::MAX));
            h.missing = missing;
            h.truncated = out.truncated;
            if let Some(b) = c.basics {
                h.basics = Some(b);
            }
            if let Some(m) = c.metrics {
                h.clock_skew = m.remote_time.map(|r| (r - now) as f64);
                h.push_spark(SparkPoint {
                    ts: now,
                    cpu: m.cpu.as_ref().map(|c| c.total.busy as f32),
                    mem: m.mem.as_ref().map(|m| m.used_pct as f32),
                });
                h.metrics = Some(Timed { ts: now, data: m });
            }
            if let Some(m) = c.medium {
                h.medium = Some(Timed { ts: now, data: m });
            }
            if let Some(i) = c.inventory {
                h.inventory = Some(Timed { ts: now, data: i });
            }
            if let Some(u) = c.updates {
                h.updates = Some(Timed { ts: now, data: u });
            }
            if let Some(a) = auth_state {
                h.auth = Some(Timed { ts: now, data: a });
            }
            if f1h.is_some() {
                h.failed_logins_1h = f1h;
                h.failed_logins_24h = f24h;
            }
        });
        publish(&app, &name);
    }
}

fn group_of(section: &str) -> Option<Group> {
    Some(match section {
        "stat" | "meminfo" | "loadavg" | "uptime" | "diskstats" | "netdev" | "df" => Group::Metrics,
        "listen" | "services" | "containers" => Group::Medium,
        "osrelease" | "uname" | "packages" | "units" => Group::Inventory,
        "basics" => Group::Basics,
        "auth" => Group::Auth,
        "updates" => Group::Updates,
        _ => return None,
    })
}

fn publish(app: &App, name: &str) {
    let now = store::now();
    let stale = app.stale_after();
    if let Some(s) = app
        .fleet
        .with(|m| m.get(name).map(|h| h.summary(now, stale, false)))
        .flatten()
    {
        let _ = app.events.send(Event::Host(Box::new(s)));
    }
}

/// Loads persisted slow-changing data so the UI has something to show
/// right after a restart.
pub async fn restore(app: &App) {
    for (kind, apply) in [("medium", 0u8), ("inventory", 1u8), ("updates", 2u8)] {
        let Ok(rows) = app.store.all_latest(kind).await else {
            continue;
        };
        for (host_id, ts, json) in rows {
            let Some(name) = app
                .fleet
                .with(|m| {
                    m.values()
                        .find(|h| h.id == host_id)
                        .map(|h| h.cfg.name.clone())
                })
                .flatten()
            else {
                continue;
            };
            app.fleet.update(&name, |h| match apply {
                0 => {
                    if let Ok(d) = serde_json::from_str(&json) {
                        h.medium = Some(Timed { ts, data: d });
                    }
                }
                1 => {
                    if let Ok(d) = serde_json::from_str(&json) {
                        h.inventory = Some(Timed { ts, data: d });
                    }
                }
                _ => {
                    if let Ok(d) = serde_json::from_str(&json) {
                        h.updates = Some(Timed { ts, data: d });
                    }
                }
            });
        }
    }
    let now = store::now();
    if let (Ok(day), Ok(hour)) = (
        app.store.auth_counts(now - 86_400).await,
        app.store.auth_counts(now - 3600).await,
    ) {
        for name in app.fleet.names() {
            app.fleet.update(&name, |h| {
                let find = |v: &[(i64, i64)]| {
                    v.iter()
                        .find(|(id, _)| *id == h.id)
                        .map(|(_, n)| u64::try_from(*n).unwrap_or(0))
                        .unwrap_or(0)
                };
                h.failed_logins_24h = Some(find(&day));
                h.failed_logins_1h = Some(find(&hour));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_policies() {
        let i = Duration::from_secs(5);
        let unreachable = TransportError::Unreachable {
            message: String::new(),
        };
        assert_eq!(backoff(&unreachable, 1, i), Duration::from_secs(5));
        assert_eq!(backoff(&unreachable, 3, i), Duration::from_secs(20));
        assert_eq!(backoff(&unreachable, 30, i), Duration::from_secs(160));
        let auth = TransportError::AuthFailed {
            message: String::new(),
        };
        assert_eq!(backoff(&auth, 1, i), Duration::from_secs(60));
        assert_eq!(backoff(&auth, 9, i), Duration::from_secs(300));
    }
}
