//! Security data: failed SSH logins and TLS certificate status.

use crate::{Result, Store};
use collect::model::AuthFailure;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AuthBucket {
    pub ts: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopSource {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CertStatus {
    pub endpoint: String,
    pub checked_at: i64,
    pub not_after: Option<i64>,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub error: Option<String>,
}

impl Store {
    pub async fn insert_auth_failures(&self, host_id: i64, events: Vec<AuthFailure>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        self.write(move |c| {
            let tx = c.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO auth_failures(host_id, ts, user, ip, invalid, method)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )?;
                for e in &events {
                    stmt.execute(params![
                        host_id,
                        e.ts,
                        e.user,
                        e.ip,
                        e.invalid_user,
                        e.method
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Newest stored failure timestamp for a host, used as the next `since`.
    pub async fn last_auth_failure(&self, host_id: i64) -> Result<Option<i64>> {
        self.read(move |c| {
            Ok(c.query_row(
                "SELECT MAX(ts) FROM auth_failures WHERE host_id = ?1",
                [host_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
        })
        .await
    }

    /// Failed logins bucketed by `bucket` seconds, for one host or all.
    pub async fn auth_histogram(
        &self,
        host_id: Option<i64>,
        from: i64,
        bucket: i64,
    ) -> Result<Vec<AuthBucket>> {
        let bucket = bucket.max(60);
        self.read(move |c| {
            let mut stmt = c.prepare(
                "SELECT (ts / ?1) * ?1 AS b, COUNT(*) FROM auth_failures
                 WHERE ts >= ?2 AND (?3 IS NULL OR host_id = ?3)
                 GROUP BY b ORDER BY b",
            )?;
            let rows = stmt
                .query_map(params![bucket, from, host_id], |r| {
                    Ok(AuthBucket {
                        ts: r.get(0)?,
                        count: r.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Failed logins per host since `from`.
    pub async fn auth_counts(&self, from: i64) -> Result<Vec<(i64, i64)>> {
        self.read(move |c| {
            let mut stmt = c.prepare(
                "SELECT host_id, COUNT(*) FROM auth_failures WHERE ts >= ?1 GROUP BY host_id",
            )?;
            let rows = stmt
                .query_map([from], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Most frequent source IPs or user names (`column` is `ip` or `user`).
    pub async fn auth_top(
        &self,
        host_id: Option<i64>,
        from: i64,
        column: &'static str,
        limit: u32,
    ) -> Result<Vec<TopSource>> {
        let col = if column == "ip" { "ip" } else { "user" };
        self.read(move |c| {
            let sql = format!(
                "SELECT COALESCE({col}, '?'), COUNT(*) AS n FROM auth_failures
                 WHERE ts >= ?1 AND (?2 IS NULL OR host_id = ?2)
                 GROUP BY 1 ORDER BY n DESC LIMIT ?3"
            );
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt
                .query_map(params![from, host_id, limit], |r| {
                    Ok(TopSource {
                        value: r.get(0)?,
                        count: r.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn put_cert_status(&self, s: CertStatus) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "INSERT INTO cert_status(endpoint, checked_at, not_after, subject, issuer, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(endpoint) DO UPDATE SET checked_at = excluded.checked_at,
                   not_after = excluded.not_after, subject = excluded.subject,
                   issuer = excluded.issuer, error = excluded.error",
                params![
                    s.endpoint,
                    s.checked_at,
                    s.not_after,
                    s.subject,
                    s.issuer,
                    s.error
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn cert_statuses(&self) -> Result<Vec<CertStatus>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT endpoint, checked_at, not_after, subject, issuer, error FROM cert_status ORDER BY endpoint",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(CertStatus {
                        endpoint: r.get(0)?,
                        checked_at: r.get(1)?,
                        not_after: r.get(2)?,
                        subject: r.get(3)?,
                        issuer: r.get(4)?,
                        error: r.get(5)?,
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

    #[tokio::test]
    async fn auth_aggregation() {
        let store = Store::memory().unwrap();
        let h = store.host_id("h").await.unwrap();
        let ev = |ts: i64, user: &str, ip: &str| AuthFailure {
            ts,
            user: user.into(),
            ip: Some(ip.into()),
            invalid_user: true,
            method: "password".into(),
        };
        store
            .insert_auth_failures(
                h,
                vec![
                    ev(3600, "root", "1.1.1.1"),
                    ev(3700, "admin", "1.1.1.1"),
                    ev(7300, "root", "2.2.2.2"),
                ],
            )
            .await
            .unwrap();
        assert_eq!(store.last_auth_failure(h).await.unwrap(), Some(7300));
        let hist = store.auth_histogram(Some(h), 0, 3600).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].count, 2);
        let top = store.auth_top(None, 0, "ip", 5).await.unwrap();
        assert_eq!(top[0].value, "1.1.1.1");
        assert_eq!(top[0].count, 2);
        assert_eq!(store.auth_counts(3650).await.unwrap(), vec![(h, 2)]);
    }
}
