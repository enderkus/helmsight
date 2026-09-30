//! Alert manager: evaluates rules against the fleet, keeps alert records in
//! the database and dispatches notifications.

use crate::app::App;
use crate::state::{AlertCounts, Conn, Event};
use alerts::notify::{Event as NotifyEvent, Notifier, Status};
use alerts::{CertInput, Evaluator, Firing, HostInput, Reachability};
use common::config::Severity;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use store::alerts::{AlertRecord, Delivery, NewAlert, Silence};

pub fn start(app: Arc<App>) {
    tokio::spawn(async move {
        let mut evaluator = Evaluator::new(&app.cfg.alerts);
        let period = app.cfg.collect.interval.0.max(Duration::from_secs(5));
        let mut ticker = tokio::time::interval(period);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if app.shutdown.is_cancelled() {
                return;
            }
            if let Err(e) = cycle(&app, &mut evaluator).await {
                tracing::error!(error = %e, "alert evaluation failed");
            }
        }
    });
}

fn parse_severity(s: &str) -> Severity {
    match s {
        "critical" => Severity::Critical,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    }
}

fn is_silenced(a: &AlertRecord, silences: &[Silence], now: i64) -> bool {
    silences
        .iter()
        .any(|s| s.matches(&a.rule_id, a.host.as_deref(), now))
}

async fn cycle(app: &Arc<App>, ev: &mut Evaluator) -> Result<(), store::StoreError> {
    let now = store::now();
    let mut pending_hosts: HashSet<String> = HashSet::new();
    let inputs: Vec<HostInput> = app
        .fleet
        .with(|m| {
            m.values()
                .filter(|h| !h.cfg.disabled)
                .map(|h| {
                    let reachability = match &h.conn {
                        Conn::Pending | Conn::Disabled => Reachability::Pending,
                        Conn::Ok => Reachability::Ok,
                        Conn::Failed {
                            kind,
                            message,
                            since,
                            key,
                        } => match *kind {
                            "auth_failed" => Reachability::AuthFailed {
                                since: *since,
                                message: message.clone(),
                            },
                            "host_key_unknown" => Reachability::HostKeyUnknown {
                                fingerprint: key
                                    .as_ref()
                                    .map(|k| k.fingerprint.clone())
                                    .unwrap_or_default(),
                            },
                            "host_key_changed" => Reachability::HostKeyChanged {
                                fingerprint: key
                                    .as_ref()
                                    .map(|k| k.fingerprint.clone())
                                    .unwrap_or_default(),
                            },
                            _ => Reachability::Unreachable {
                                since: *since,
                                message: message.clone(),
                            },
                        },
                    };
                    HostInput {
                        name: h.cfg.name.clone(),
                        groups: h.cfg.groups.clone(),
                        tags: h.cfg.tags.clone(),
                        reachability,
                        samples: alerts::samples(
                            h.metrics.as_ref().map(|t| &t.data),
                            h.medium.as_ref().map(|t| &t.data),
                            h.inventory.as_ref().map(|t| &t.data),
                            h.updates.as_ref().map(|t| &t.data),
                            h.failed_logins_1h,
                            h.clock_skew,
                        ),
                        failed_units: h.failed_units(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    for i in &inputs {
        if i.reachability == Reachability::Pending {
            pending_hosts.insert(i.name.clone());
        }
    }
    let certs: Vec<CertInput> = app
        .store
        .cert_statuses()
        .await?
        .into_iter()
        .filter(|c| app.cfg.certs.iter().any(|x| x.endpoint == c.endpoint))
        .map(|c| CertInput {
            days_left: c.not_after.map(|na| (na - now) as f64 / 86_400.0),
            endpoint: c.endpoint,
            error: c.error,
        })
        .collect();

    let firing: HashMap<String, Firing> = ev
        .evaluate(now, &inputs, &certs)
        .into_iter()
        .map(|f| (f.fingerprint.clone(), f))
        .collect();
    let open = app.store.firing_alerts().await?;
    let silences = app.store.silences(0).await?;
    let open_fps: HashSet<String> = open.iter().map(|a| a.fingerprint.clone()).collect();
    let mut changed = false;

    for f in firing.values() {
        if open_fps.contains(&f.fingerprint) {
            continue;
        }
        let new = NewAlert {
            fingerprint: f.fingerprint.clone(),
            rule_id: f.rule_id.clone(),
            host: f.host.clone(),
            instance: f.instance.clone(),
            severity: f.severity.as_str().to_string(),
            summary: f.summary.chars().take(500).collect(),
            value: f.value,
            started_at: now,
        };
        if let Some(rec) = app.store.open_alert(new).await? {
            changed = true;
            tracing::info!(rule = %rec.rule_id, host = ?rec.host, summary = %rec.summary, "alert firing");
            if !is_silenced(&rec, &silences, now) {
                notify(app, rec, Status::Firing);
            }
        }
    }

    let repeat = i64::try_from(app.cfg.alerts.repeat_interval.as_secs()).unwrap_or(0);
    for rec in open {
        match firing.get(&rec.fingerprint) {
            Some(f) => {
                let summary: String = f.summary.chars().take(500).collect();
                if summary != rec.summary {
                    app.store
                        .update_alert_value(rec.id, f.value, summary)
                        .await?;
                }
                let due = now - rec.last_notified_at.unwrap_or(rec.started_at) >= repeat;
                if repeat > 0 && rec.ack_at.is_none() && due && !is_silenced(&rec, &silences, now) {
                    app.store.mark_notified(rec.id, now).await?;
                    notify(app, rec, Status::Reminder);
                }
            }
            None => {
                // Keep alerts of hosts without data yet (just after start),
                // and conditions that still hold but restarted their timer.
                if rec.host.as_ref().is_some_and(|h| pending_hosts.contains(h))
                    || ev.is_active(&rec.fingerprint)
                {
                    continue;
                }
                let resolved = app.store.resolve_alert(rec.id, now).await?;
                changed = true;
                tracing::info!(rule = %resolved.rule_id, host = ?resolved.host, "alert resolved");
                if resolved.last_notified_at.is_some() && !is_silenced(&resolved, &silences, now) {
                    notify(app, resolved, Status::Resolved);
                }
            }
        }
    }

    // Per-host counts for the fleet overview.
    let current = app.store.firing_alerts().await?;
    let mut counts: BTreeMap<String, AlertCounts> = BTreeMap::new();
    for a in &current {
        if let Some(h) = &a.host {
            let c = counts.entry(h.clone()).or_default();
            match a.severity.as_str() {
                "critical" => c.critical += 1,
                "warning" => c.warning += 1,
                _ => c.info += 1,
            }
        }
    }
    let mut touched = Vec::new();
    for name in app.fleet.names() {
        let new = counts.get(&name).copied().unwrap_or_default();
        if app
            .fleet
            .update(&name, |h| std::mem::replace(&mut h.alerts, new) != new)
            == Some(true)
        {
            touched.push(name);
        }
    }
    let stale = app.stale_after();
    for name in touched {
        if let Some(s) = app
            .fleet
            .with(|m| m.get(&name).map(|h| h.summary(now, stale, false)))
            .flatten()
        {
            let _ = app.events.send(Event::Host(Box::new(s)));
        }
    }
    if changed {
        let _ = app.events.send(Event::Alerts);
    }
    Ok(())
}

/// Sends a notification to every matching channel in the background.
fn notify(app: &Arc<App>, rec: AlertRecord, status: Status) {
    let severity = parse_severity(&rec.severity);
    let host_cfg = rec.host.as_ref().and_then(|h| app.cfg.host(h).cloned());
    let channels: Vec<_> = app
        .channels
        .iter()
        .filter(|ch| {
            Notifier::accepts(ch, severity, |sel| {
                host_cfg.as_ref().is_some_and(|h| sel.matches(h))
            })
        })
        .cloned()
        .collect();
    if channels.is_empty() {
        return;
    }
    let app = app.clone();
    tokio::spawn(async move {
        let now = store::now();
        if matches!(status, Status::Firing) {
            let _ = app.store.mark_notified(rec.id, now).await;
        }
        let event = NotifyEvent {
            status,
            link: app.link(&format!("/alerts?id={}", rec.id)),
            product: common::PRODUCT_NAME.to_string(),
            alert: rec,
        };
        for ch in channels {
            let r = app.notifier.send(&ch, &event).await;
            if let Err(e) = &r {
                tracing::warn!(channel = %ch.id, error = %e, "notification not delivered");
            }
            let _ = app
                .store
                .record_delivery(Delivery {
                    alert_id: event.alert.id,
                    channel: ch.id.clone(),
                    ts: store::now(),
                    ok: r.is_ok(),
                    error: r.err(),
                })
                .await;
        }
    });
}
