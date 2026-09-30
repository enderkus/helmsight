//! Inventory snapshots and drift detection.
//!
//! A snapshot is a normalized view of what is installed and exposed on a
//! host. Comparing two snapshots (the same host over time, or two hosts)
//! yields a list of [`Change`]s.

use crate::{Result, Store};
use collect::model::{Inventory, ListenSocket, Probe};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Snapshot {
    pub os: Option<String>,
    pub kernel: Option<String>,
    /// name -> version(s); several versions are joined with ", ".
    pub packages: BTreeMap<String, String>,
    /// `proto/port` -> listening addresses and processes.
    pub ports: BTreeMap<String, String>,
    pub units: BTreeSet<String>,
    /// Which parts are known; missing parts are not compared.
    pub has_packages: bool,
    pub has_ports: bool,
    pub has_units: bool,
}

impl Snapshot {
    pub fn build(inv: &Inventory, listening: Option<&Probe<Vec<ListenSocket>>>) -> Self {
        let mut s = Snapshot {
            os: inv.os.pretty_name.clone().or_else(|| inv.os.name.clone()),
            kernel: inv.kernel.release.clone(),
            ..Snapshot::default()
        };
        if let Probe::Ok { data, .. } = &inv.packages {
            s.has_packages = true;
            for p in data {
                s.packages
                    .entry(p.name.clone())
                    .and_modify(|v| {
                        if !v.split(", ").any(|x| x == p.version) {
                            v.push_str(", ");
                            v.push_str(&p.version);
                        }
                    })
                    .or_insert_with(|| p.version.clone());
            }
        }
        if let Some(Probe::Ok { data, .. }) = listening {
            s.has_ports = true;
            let mut ports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for l in data {
                let desc = match &l.process {
                    Some(p) => format!("{} ({p})", l.address),
                    None => l.address.clone(),
                };
                ports
                    .entry(format!("{}/{}", l.proto, l.port))
                    .or_default()
                    .insert(desc);
            }
            s.ports = ports
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().collect::<Vec<_>>().join(", ")))
                .collect();
        }
        if let Probe::Ok { data, .. } = &inv.enabled_units {
            s.has_units = true;
            s.units = data.iter().cloned().collect();
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Change {
    /// `package`, `port`, `unit`, `kernel` or `os`.
    pub kind: String,
    /// `added`, `removed` or `changed`.
    pub action: String,
    pub subject: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

fn change(kind: &str, action: &str, subject: &str, old: Option<&str>, new: Option<&str>) -> Change {
    Change {
        kind: kind.into(),
        action: action.into(),
        subject: subject.into(),
        old: old.map(str::to_string),
        new: new.map(str::to_string),
    }
}

fn diff_maps(
    kind: &str,
    a: &BTreeMap<String, String>,
    b: &BTreeMap<String, String>,
    out: &mut Vec<Change>,
) {
    for (k, va) in a {
        match b.get(k) {
            None => out.push(change(kind, "removed", k, Some(va), None)),
            Some(vb) if vb != va => out.push(change(kind, "changed", k, Some(va), Some(vb))),
            _ => {}
        }
    }
    for (k, vb) in b {
        if !a.contains_key(k) {
            out.push(change(kind, "added", k, None, Some(vb)));
        }
    }
}

/// Differences going from `a` to `b`. Parts unknown on either side are
/// skipped rather than reported as mass additions or removals.
pub fn diff(a: &Snapshot, b: &Snapshot) -> Vec<Change> {
    let mut out = Vec::new();
    if a.os != b.os && a.os.is_some() && b.os.is_some() {
        out.push(change(
            "os",
            "changed",
            "os",
            a.os.as_deref(),
            b.os.as_deref(),
        ));
    }
    if a.kernel != b.kernel && a.kernel.is_some() && b.kernel.is_some() {
        out.push(change(
            "kernel",
            "changed",
            "kernel",
            a.kernel.as_deref(),
            b.kernel.as_deref(),
        ));
    }
    if a.has_packages && b.has_packages {
        diff_maps("package", &a.packages, &b.packages, &mut out);
    }
    if a.has_ports && b.has_ports {
        diff_maps("port", &a.ports, &b.ports, &mut out);
    }
    if a.has_units && b.has_units {
        for u in a.units.difference(&b.units) {
            out.push(change("unit", "removed", u, Some("enabled"), None));
        }
        for u in b.units.difference(&a.units) {
            out.push(change("unit", "added", u, None, Some("enabled")));
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ChangeRecord {
    pub id: i64,
    pub host_id: i64,
    pub ts: i64,
    #[serde(flatten)]
    pub change: Change,
}

impl Store {
    /// Stores a new snapshot and records its differences to the previous one.
    /// Returns the changes recorded.
    pub async fn record_snapshot(
        &self,
        host_id: i64,
        ts: i64,
        snap: Snapshot,
    ) -> Result<Vec<Change>> {
        self.write(move |c| {
            let tx = c.transaction()?;
            let prev: Option<String> = tx
                .query_row(
                    "SELECT data FROM host_latest WHERE host_id = ?1 AND kind = 'snapshot'",
                    [host_id],
                    |r| r.get(0),
                )
                .ok();
            let prev: Option<Snapshot> = prev.and_then(|p| serde_json::from_str(&p).ok());
            let changes = match &prev {
                Some(p) => diff(p, &snap),
                None => Vec::new(),
            };
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO changes(host_id, ts, kind, action, subject, old, new)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for ch in &changes {
                    stmt.execute(params![
                        host_id, ts, ch.kind, ch.action, ch.subject, ch.old, ch.new
                    ])?;
                }
            }
            let json = serde_json::to_string(&snap)
                .map_err(|e| crate::StoreError::Other(e.to_string()))?;
            tx.execute(
                "INSERT INTO host_latest(host_id, kind, ts, data) VALUES (?1, 'snapshot', ?2, ?3)
                 ON CONFLICT(host_id, kind) DO UPDATE SET ts = excluded.ts, data = excluded.data",
                params![host_id, ts, json],
            )?;
            tx.commit()?;
            Ok(changes)
        })
        .await
    }

    pub async fn snapshot(&self, host_id: i64) -> Result<Option<(i64, Snapshot)>> {
        Ok(self
            .get_latest(host_id, "snapshot")
            .await?
            .and_then(|(ts, j)| serde_json::from_str(&j).ok().map(|s| (ts, s))))
    }

    /// Changes for a host (or all hosts) since `since`, newest first.
    pub async fn changes(
        &self,
        host_id: Option<i64>,
        since: i64,
        limit: u32,
    ) -> Result<Vec<ChangeRecord>> {
        self.read(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, host_id, ts, kind, action, subject, old, new FROM changes
                 WHERE ts >= ?1 AND (?2 IS NULL OR host_id = ?2)
                 ORDER BY ts DESC, id DESC LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(params![since, host_id, limit], |r| {
                    Ok(ChangeRecord {
                        id: r.get(0)?,
                        host_id: r.get(1)?,
                        ts: r.get(2)?,
                        change: Change {
                            kind: r.get(3)?,
                            action: r.get(4)?,
                            subject: r.get(5)?,
                            old: r.get(6)?,
                            new: r.get(7)?,
                        },
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(pkgs: &[(&str, &str)], ports: &[(&str, &str)], units: &[&str]) -> Snapshot {
        Snapshot {
            os: Some("Debian 12".into()),
            kernel: Some("6.1.0".into()),
            packages: pkgs
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            ports: ports
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            units: units.iter().map(|u| u.to_string()).collect(),
            has_packages: true,
            has_ports: true,
            has_units: true,
        }
    }

    #[test]
    fn diff_reports_each_kind() {
        let a = snap(
            &[("curl", "7.88.1"), ("vim", "9.0")],
            &[("tcp/22", "0.0.0.0")],
            &["ssh.service"],
        );
        let mut b = snap(
            &[("curl", "7.88.2"), ("nginx", "1.22")],
            &[("tcp/22", "0.0.0.0"), ("tcp/80", "0.0.0.0 (nginx)")],
            &["ssh.service", "nginx.service"],
        );
        b.kernel = Some("6.1.1".into());
        let d = diff(&a, &b);
        let has = |kind: &str, action: &str, subject: &str| {
            d.iter()
                .any(|c| c.kind == kind && c.action == action && c.subject == subject)
        };
        assert!(has("kernel", "changed", "kernel"));
        assert!(has("package", "changed", "curl"));
        assert!(has("package", "removed", "vim"));
        assert!(has("package", "added", "nginx"));
        assert!(has("port", "added", "tcp/80"));
        assert!(has("unit", "added", "nginx.service"));
        assert_eq!(d.len(), 6);
    }

    #[test]
    fn unknown_parts_are_not_compared() {
        let a = snap(&[("curl", "1")], &[], &[]);
        let b = Snapshot {
            os: a.os.clone(),
            kernel: a.kernel.clone(),
            ..Snapshot::default()
        };
        assert!(diff(&a, &b).is_empty());
    }

    #[tokio::test]
    async fn records_changes_over_time() {
        let store = Store::memory().unwrap();
        let h = store.host_id("db-1").await.unwrap();
        let first = store
            .record_snapshot(h, 100, snap(&[("a", "1")], &[], &[]))
            .await
            .unwrap();
        assert!(first.is_empty(), "first snapshot has no baseline");
        let second = store
            .record_snapshot(h, 200, snap(&[("a", "2")], &[], &[]))
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        let list = store.changes(Some(h), 0, 100).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].change.new.as_deref(), Some("2"));
        assert_eq!(store.snapshot(h).await.unwrap().unwrap().0, 200);
    }
}
