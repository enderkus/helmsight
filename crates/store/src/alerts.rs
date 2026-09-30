//! Alert instances, silences and notification delivery records.

use crate::{Result, Store, StoreError};
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct AlertRecord {
    pub id: i64,
    pub fingerprint: String,
    pub rule_id: String,
    pub host: Option<String>,
    pub instance: Option<String>,
    pub severity: String,
    /// `firing` or `resolved`.
    pub state: String,
    pub summary: String,
    pub value: Option<f64>,
    pub started_at: i64,
    pub resolved_at: Option<i64>,
    pub last_notified_at: Option<i64>,
    pub ack_by: Option<String>,
    pub ack_at: Option<i64>,
    pub ack_note: Option<String>,
}

const ALERT_COLS: &str =
    "id, fingerprint, rule_id, host, instance, severity, state, summary, value,
    started_at, resolved_at, last_notified_at, ack_by, ack_at, ack_note";

fn alert_row(r: &Row<'_>) -> rusqlite::Result<AlertRecord> {
    Ok(AlertRecord {
        id: r.get(0)?,
        fingerprint: r.get(1)?,
        rule_id: r.get(2)?,
        host: r.get(3)?,
        instance: r.get(4)?,
        severity: r.get(5)?,
        state: r.get(6)?,
        summary: r.get(7)?,
        value: r.get(8)?,
        started_at: r.get(9)?,
        resolved_at: r.get(10)?,
        last_notified_at: r.get(11)?,
        ack_by: r.get(12)?,
        ack_at: r.get(13)?,
        ack_note: r.get(14)?,
    })
}

/// A new firing alert.
#[derive(Debug, Clone)]
pub struct NewAlert {
    pub fingerprint: String,
    pub rule_id: String,
    pub host: Option<String>,
    pub instance: Option<String>,
    pub severity: String,
    pub summary: String,
    pub value: Option<f64>,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Silence {
    pub id: i64,
    pub rule_id: Option<String>,
    pub host: Option<String>,
    pub reason: String,
    pub created_by: String,
    pub created_at: i64,
    pub ends_at: i64,
    pub expired_by: Option<String>,
}

impl Silence {
    pub fn matches(&self, rule_id: &str, host: Option<&str>, now: i64) -> bool {
        self.ends_at > now
            && self.rule_id.as_deref().is_none_or(|r| r == rule_id)
            && self.host.as_deref().is_none_or(|h| Some(h) == host)
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Delivery {
    pub alert_id: i64,
    pub channel: String,
    pub ts: i64,
    pub ok: bool,
    pub error: Option<String>,
}

impl Store {
    pub async fn firing_alerts(&self) -> Result<Vec<AlertRecord>> {
        self.read(|c| {
            let sql = format!(
                "SELECT {ALERT_COLS} FROM alerts WHERE state = 'firing' ORDER BY started_at DESC"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt
                .query_map([], alert_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Alerts started or resolved since `since`, newest first.
    pub async fn recent_alerts(&self, since: i64, limit: u32) -> Result<Vec<AlertRecord>> {
        self.read(move |c| {
            let sql = format!(
                "SELECT {ALERT_COLS} FROM alerts
                 WHERE state = 'firing' OR started_at >= ?1 OR resolved_at >= ?1
                 ORDER BY (state = 'firing') DESC, started_at DESC LIMIT ?2"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt
                .query_map(params![since, limit], alert_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn alert(&self, id: i64) -> Result<AlertRecord> {
        self.read(move |c| {
            let sql = format!("SELECT {ALERT_COLS} FROM alerts WHERE id = ?1");
            c.query_row(&sql, [id], alert_row)
                .optional()?
                .ok_or(StoreError::NotFound)
        })
        .await
    }

    /// Inserts a firing alert. Returns `None` if one with the same
    /// fingerprint is already firing.
    pub async fn open_alert(&self, a: NewAlert) -> Result<Option<AlertRecord>> {
        self.write(move |c| {
            let n = c.execute(
                "INSERT OR IGNORE INTO alerts(fingerprint, rule_id, host, instance, severity, state,
                   summary, value, started_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'firing', ?6, ?7, ?8)",
                params![a.fingerprint, a.rule_id, a.host, a.instance, a.severity, a.summary, a.value, a.started_at],
            )?;
            if n == 0 {
                return Ok(None);
            }
            let id = c.last_insert_rowid();
            let sql = format!("SELECT {ALERT_COLS} FROM alerts WHERE id = ?1");
            Ok(Some(c.query_row(&sql, [id], alert_row)?))
        })
        .await
    }

    pub async fn update_alert_value(
        &self,
        id: i64,
        value: Option<f64>,
        summary: String,
    ) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "UPDATE alerts SET value = ?2, summary = ?3 WHERE id = ?1",
                params![id, value, summary],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn resolve_alert(&self, id: i64, at: i64) -> Result<AlertRecord> {
        self.write(move |c| {
            c.execute(
                "UPDATE alerts SET state = 'resolved', resolved_at = ?2 WHERE id = ?1 AND state = 'firing'",
                params![id, at],
            )?;
            let sql = format!("SELECT {ALERT_COLS} FROM alerts WHERE id = ?1");
            Ok(c.query_row(&sql, [id], alert_row)?)
        })
        .await
    }

    pub async fn ack_alert(
        &self,
        id: i64,
        by: String,
        note: Option<String>,
        at: i64,
    ) -> Result<AlertRecord> {
        self.write(move |c| {
            let n = c.execute(
                "UPDATE alerts SET ack_by = ?2, ack_at = ?3, ack_note = ?4
                 WHERE id = ?1 AND state = 'firing' AND ack_at IS NULL",
                params![id, by, at, note],
            )?;
            let sql = format!("SELECT {ALERT_COLS} FROM alerts WHERE id = ?1");
            let rec = c
                .query_row(&sql, [id], alert_row)
                .optional()?
                .ok_or(StoreError::NotFound)?;
            if n == 0 {
                return Err(StoreError::Conflict(
                    "alert is not firing or is already acknowledged".into(),
                ));
            }
            Ok(rec)
        })
        .await
    }

    pub async fn mark_notified(&self, id: i64, at: i64) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "UPDATE alerts SET last_notified_at = ?2 WHERE id = ?1",
                params![id, at],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn record_delivery(&self, d: Delivery) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "INSERT INTO notifications(alert_id, channel, ts, ok, error) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![d.alert_id, d.channel, d.ts, d.ok, d.error],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn deliveries(&self, alert_id: i64) -> Result<Vec<Delivery>> {
        self.read(move |c| {
            let mut stmt = c.prepare(
                "SELECT alert_id, channel, ts, ok, error FROM notifications WHERE alert_id = ?1 ORDER BY ts",
            )?;
            let rows = stmt
                .query_map([alert_id], |r| {
                    Ok(Delivery {
                        alert_id: r.get(0)?,
                        channel: r.get(1)?,
                        ts: r.get(2)?,
                        ok: r.get(3)?,
                        error: r.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn create_silence(
        &self,
        rule_id: Option<String>,
        host: Option<String>,
        reason: String,
        created_by: String,
        ends_at: i64,
    ) -> Result<Silence> {
        self.write(move |c| {
            let now = crate::now();
            c.execute(
                "INSERT INTO silences(rule_id, host, reason, created_by, created_at, ends_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![rule_id, host, reason, created_by, now, ends_at],
            )?;
            Ok(Silence {
                id: c.last_insert_rowid(),
                rule_id,
                host,
                reason,
                created_by,
                created_at: now,
                ends_at,
                expired_by: None,
            })
        })
        .await
    }

    /// Ends a silence now.
    pub async fn expire_silence(&self, id: i64, by: String) -> Result<()> {
        self.write(move |c| {
            let n = c.execute(
                "UPDATE silences SET ends_at = ?2, expired_by = ?3 WHERE id = ?1 AND ends_at > ?2",
                params![id, crate::now(), by],
            )?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Ok(())
        })
        .await
    }

    /// Active silences, plus those that ended within `recent` seconds.
    pub async fn silences(&self, recent: i64) -> Result<Vec<Silence>> {
        self.read(move |c| {
            let mut stmt = c.prepare(
                "SELECT id, rule_id, host, reason, created_by, created_at, ends_at, expired_by
                 FROM silences WHERE ends_at > ?1 ORDER BY ends_at DESC",
            )?;
            let rows = stmt
                .query_map([crate::now() - recent], |r| {
                    Ok(Silence {
                        id: r.get(0)?,
                        rule_id: r.get(1)?,
                        host: r.get(2)?,
                        reason: r.get(3)?,
                        created_by: r.get(4)?,
                        created_at: r.get(5)?,
                        ends_at: r.get(6)?,
                        expired_by: r.get(7)?,
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

    fn new(fp: &str) -> NewAlert {
        NewAlert {
            fingerprint: fp.into(),
            rule_id: "disk".into(),
            host: Some("web-1".into()),
            instance: Some("/".into()),
            severity: "critical".into(),
            summary: "disk full".into(),
            value: Some(95.0),
            started_at: 100,
        }
    }

    #[tokio::test]
    async fn lifecycle() {
        let store = Store::memory().unwrap();
        let a = store.open_alert(new("x")).await.unwrap().unwrap();
        assert!(
            store.open_alert(new("x")).await.unwrap().is_none(),
            "one firing per fingerprint"
        );
        let acked = store
            .ack_alert(a.id, "alice".into(), Some("on it".into()), 150)
            .await
            .unwrap();
        assert_eq!(acked.ack_by.as_deref(), Some("alice"));
        assert!(
            store
                .ack_alert(a.id, "bob".into(), None, 160)
                .await
                .is_err()
        );
        let r = store.resolve_alert(a.id, 200).await.unwrap();
        assert_eq!(r.state, "resolved");
        // A new instance may fire after resolution.
        assert!(store.open_alert(new("x")).await.unwrap().is_some());
        assert_eq!(store.firing_alerts().await.unwrap().len(), 1);
        assert_eq!(store.recent_alerts(0, 10).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn silences_match() {
        let store = Store::memory().unwrap();
        let now = crate::now();
        let s = store
            .create_silence(
                None,
                Some("web-1".into()),
                "maintenance".into(),
                "alice".into(),
                now + 3600,
            )
            .await
            .unwrap();
        assert!(s.matches("anything", Some("web-1"), now));
        assert!(!s.matches("anything", Some("web-2"), now));
        assert!(!s.matches("anything", Some("web-1"), now + 7200));
        store.expire_silence(s.id, "bob".into()).await.unwrap();
        let list = store.silences(86_400).await.unwrap();
        assert_eq!(list[0].expired_by.as_deref(), Some("bob"));
    }
}
