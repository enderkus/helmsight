//! Users, sessions and display tokens. Password hashing and token
//! generation live in the server crate; this module only stores hashes.

use crate::{Result, Store, StoreError};
use common::Role;
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: Option<String>,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub role: Role,
    #[serde(skip)]
    pub totp_secret: Option<String>,
    pub totp_enabled: bool,
    pub oidc_subject: Option<String>,
    pub created_at: i64,
    pub last_login_at: Option<i64>,
    pub disabled: bool,
    pub must_change_password: bool,
}

const USER_COLS: &str =
    "id, username, display_name, password_hash, role, totp_secret, totp_enabled,
    oidc_subject, created_at, last_login_at, disabled, must_change_password";

fn user_row(r: &Row<'_>) -> rusqlite::Result<User> {
    let role: String = r.get(4)?;
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        display_name: r.get(2)?,
        password_hash: r.get(3)?,
        role: role.parse().unwrap_or(Role::Viewer),
        totp_secret: r.get(5)?,
        totp_enabled: r.get(6)?,
        oidc_subject: r.get(7)?,
        created_at: r.get(8)?,
        last_login_at: r.get(9)?,
        disabled: r.get(10)?,
        must_change_password: r.get(11)?,
    })
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id_hash: String,
    pub user: User,
    pub csrf: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisplayToken {
    pub id: i64,
    pub name: String,
    pub groups: Vec<String>,
    pub created_by: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

fn token_row(r: &Row<'_>) -> rusqlite::Result<DisplayToken> {
    let groups: String = r.get(2)?;
    Ok(DisplayToken {
        id: r.get(0)?,
        name: r.get(1)?,
        groups: serde_json::from_str(&groups).unwrap_or_default(),
        created_by: r.get(3)?,
        created_at: r.get(4)?,
        revoked_at: r.get(5)?,
        last_used_at: r.get(6)?,
    })
}

impl Store {
    pub async fn user_count(&self) -> Result<i64> {
        self.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?))
            .await
    }

    pub async fn create_user(
        &self,
        username: String,
        password_hash: Option<String>,
        role: Role,
        oidc_subject: Option<String>,
        display_name: Option<String>,
    ) -> Result<User> {
        self.write(move |c| {
            let res = c.execute(
                "INSERT INTO users(username, display_name, password_hash, role, oidc_subject, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![username, display_name, password_hash, role.as_str(), oidc_subject, crate::now()],
            );
            match res {
                Err(rusqlite::Error::SqliteFailure(e, _))
                    if e.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    return Err(StoreError::Conflict(format!("user `{username}` already exists")));
                }
                r => r?,
            };
            let id = c.last_insert_rowid();
            let sql = format!("SELECT {USER_COLS} FROM users WHERE id = ?1");
            Ok(c.query_row(&sql, [id], user_row)?)
        })
        .await
    }

    pub async fn user_by_name(&self, username: String) -> Result<Option<User>> {
        self.read(move |c| {
            let sql = format!("SELECT {USER_COLS} FROM users WHERE username = ?1");
            Ok(c.query_row(&sql, [username], user_row).optional()?)
        })
        .await
    }

    pub async fn user_by_id(&self, id: i64) -> Result<Option<User>> {
        self.read(move |c| {
            let sql = format!("SELECT {USER_COLS} FROM users WHERE id = ?1");
            Ok(c.query_row(&sql, [id], user_row).optional()?)
        })
        .await
    }

    pub async fn user_by_oidc(&self, subject: String) -> Result<Option<User>> {
        self.read(move |c| {
            let sql = format!("SELECT {USER_COLS} FROM users WHERE oidc_subject = ?1");
            Ok(c.query_row(&sql, [subject], user_row).optional()?)
        })
        .await
    }

    pub async fn users(&self) -> Result<Vec<User>> {
        self.read(|c| {
            let sql = format!("SELECT {USER_COLS} FROM users ORDER BY username");
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt
                .query_map([], user_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Updates selected fields of a user. `None` leaves a field unchanged.
    pub async fn update_user(&self, id: i64, update: UserUpdate) -> Result<User> {
        self.write(move |c| {
            let tx = c.transaction()?;
            if let Some(role) = update.role {
                tx.execute(
                    "UPDATE users SET role = ?2 WHERE id = ?1",
                    params![id, role.as_str()],
                )?;
            }
            if let Some(h) = update.password_hash {
                tx.execute(
                    "UPDATE users SET password_hash = ?2, must_change_password = ?3 WHERE id = ?1",
                    params![id, h, update.must_change_password.unwrap_or(false)],
                )?;
            }
            if let Some(d) = update.disabled {
                tx.execute(
                    "UPDATE users SET disabled = ?2 WHERE id = ?1",
                    params![id, d],
                )?;
                if d {
                    tx.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
                }
            }
            if let Some(t) = update.totp {
                let (secret, enabled) = match t {
                    Some((s, e)) => (Some(s), e),
                    None => (None, false),
                };
                tx.execute(
                    "UPDATE users SET totp_secret = ?2, totp_enabled = ?3 WHERE id = ?1",
                    params![id, secret, enabled],
                )?;
            }
            if let Some(name) = update.display_name {
                tx.execute(
                    "UPDATE users SET display_name = ?2 WHERE id = ?1",
                    params![id, name],
                )?;
            }
            if update.touch_login {
                tx.execute(
                    "UPDATE users SET last_login_at = ?2 WHERE id = ?1",
                    params![id, crate::now()],
                )?;
            }
            let sql = format!("SELECT {USER_COLS} FROM users WHERE id = ?1");
            let user = tx
                .query_row(&sql, [id], user_row)
                .optional()?
                .ok_or(StoreError::NotFound)?;
            tx.commit()?;
            Ok(user)
        })
        .await
    }

    pub async fn delete_user(&self, id: i64) -> Result<()> {
        self.write(move |c| {
            if c.execute("DELETE FROM users WHERE id = ?1", [id])? == 0 {
                return Err(StoreError::NotFound);
            }
            Ok(())
        })
        .await
    }

    pub async fn admin_count(&self) -> Result<i64> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM users WHERE role = 'admin' AND disabled = 0",
                [],
                |r| r.get(0),
            )?)
        })
        .await
    }

    pub async fn create_session(
        &self,
        id_hash: String,
        user_id: i64,
        csrf: String,
        expires_at: i64,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<()> {
        self.write(move |c| {
            let now = crate::now();
            c.execute(
                "INSERT INTO sessions(id_hash, user_id, csrf, created_at, expires_at, last_seen_at, ip, user_agent)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?4, ?6, ?7)",
                params![id_hash, user_id, csrf, now, expires_at, ip, user_agent],
            )?;
            Ok(())
        })
        .await
    }

    /// Looks up a valid session, enforcing expiry and idle timeout, and
    /// refreshes its last-seen time.
    pub async fn session(&self, id_hash: String, idle_timeout: i64) -> Result<Option<Session>> {
        self.write(move |c| {
            let now = crate::now();
            let row = c
                .query_row(
                    "SELECT s.id_hash, s.user_id, s.csrf, s.created_at, s.expires_at, s.last_seen_at
                     FROM sessions s WHERE s.id_hash = ?1",
                    [&id_hash],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(4)?,
                            r.get::<_, i64>(5)?,
                        ))
                    },
                )
                .optional()?;
            let Some((id_hash, user_id, csrf, created_at, expires_at, last_seen)) = row else {
                return Ok(None);
            };
            if expires_at <= now || last_seen + idle_timeout <= now {
                c.execute("DELETE FROM sessions WHERE id_hash = ?1", [&id_hash])?;
                return Ok(None);
            }
            let sql = format!("SELECT {USER_COLS} FROM users WHERE id = ?1");
            let Some(user) = c.query_row(&sql, [user_id], user_row).optional()? else {
                return Ok(None);
            };
            if user.disabled {
                return Ok(None);
            }
            if now - last_seen >= 30 {
                c.execute(
                    "UPDATE sessions SET last_seen_at = ?2 WHERE id_hash = ?1",
                    params![id_hash, now],
                )?;
            }
            Ok(Some(Session {
                id_hash,
                user,
                csrf,
                created_at,
                expires_at,
                last_seen_at: now,
            }))
        })
        .await
    }

    pub async fn delete_session(&self, id_hash: String) -> Result<()> {
        self.write(move |c| {
            c.execute("DELETE FROM sessions WHERE id_hash = ?1", [id_hash])?;
            Ok(())
        })
        .await
    }

    pub async fn delete_user_sessions(&self, user_id: i64, except: Option<String>) -> Result<()> {
        self.write(move |c| {
            c.execute(
                "DELETE FROM sessions WHERE user_id = ?1 AND (?2 IS NULL OR id_hash != ?2)",
                params![user_id, except],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn purge_sessions(&self) -> Result<usize> {
        self.write(|c| {
            Ok(c.execute(
                "DELETE FROM sessions WHERE expires_at <= ?1",
                [crate::now()],
            )?)
        })
        .await
    }

    pub async fn create_display_token(
        &self,
        name: String,
        token_hash: String,
        groups: Vec<String>,
        created_by: String,
    ) -> Result<DisplayToken> {
        self.write(move |c| {
            let g = serde_json::to_string(&groups).map_err(|e| StoreError::Other(e.to_string()))?;
            c.execute(
                "INSERT INTO display_tokens(name, token_hash, groups, created_by, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![name, token_hash, g, created_by, crate::now()],
            )?;
            let id = c.last_insert_rowid();
            Ok(c.query_row(
                "SELECT id, name, groups, created_by, created_at, revoked_at, last_used_at
                 FROM display_tokens WHERE id = ?1",
                [id],
                token_row,
            )?)
        })
        .await
    }

    /// Validates a display token hash and records its use.
    pub async fn use_display_token(&self, token_hash: String) -> Result<Option<DisplayToken>> {
        self.write(move |c| {
            let t = c
                .query_row(
                    "SELECT id, name, groups, created_by, created_at, revoked_at, last_used_at
                     FROM display_tokens WHERE token_hash = ?1 AND revoked_at IS NULL",
                    [&token_hash],
                    token_row,
                )
                .optional()?;
            if let Some(t) = &t {
                let now = crate::now();
                if t.last_used_at.is_none_or(|l| now - l >= 60) {
                    c.execute(
                        "UPDATE display_tokens SET last_used_at = ?2 WHERE id = ?1",
                        params![t.id, now],
                    )?;
                }
            }
            Ok(t)
        })
        .await
    }

    pub async fn display_tokens(&self) -> Result<Vec<DisplayToken>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, name, groups, created_by, created_at, revoked_at, last_used_at
                 FROM display_tokens ORDER BY created_at DESC",
            )?;
            let rows = stmt
                .query_map([], token_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn revoke_display_token(&self, id: i64) -> Result<()> {
        self.write(move |c| {
            let n = c.execute(
                "UPDATE display_tokens SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
                params![id, crate::now()],
            )?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Ok(())
        })
        .await
    }
}

/// Partial user update.
#[derive(Debug, Default)]
pub struct UserUpdate {
    pub role: Option<Role>,
    pub password_hash: Option<String>,
    pub must_change_password: Option<bool>,
    pub disabled: Option<bool>,
    /// `Some(None)` clears TOTP; `Some(Some((sealed_secret, enabled)))` sets it.
    pub totp: Option<Option<(String, bool)>>,
    pub display_name: Option<String>,
    pub touch_login: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn users_and_sessions() {
        let store = Store::memory().unwrap();
        let u = store
            .create_user("Alice".into(), Some("hash".into()), Role::Admin, None, None)
            .await
            .unwrap();
        assert!(matches!(
            store
                .create_user("alice".into(), None, Role::Viewer, None, None)
                .await,
            Err(StoreError::Conflict(_))
        ));
        assert_eq!(
            store
                .user_by_name("ALICE".into())
                .await
                .unwrap()
                .unwrap()
                .id,
            u.id
        );
        store
            .create_session(
                "h1".into(),
                u.id,
                "csrf".into(),
                crate::now() + 3600,
                None,
                None,
            )
            .await
            .unwrap();
        let s = store.session("h1".into(), 600).await.unwrap().unwrap();
        assert_eq!(s.user.username, "Alice");
        // Disabling a user kills sessions.
        store
            .update_user(
                u.id,
                UserUpdate {
                    disabled: Some(true),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(store.session("h1".into(), 600).await.unwrap().is_none());
        assert_eq!(store.admin_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn expired_sessions_are_rejected() {
        let store = Store::memory().unwrap();
        let u = store
            .create_user("bob".into(), None, Role::Viewer, None, None)
            .await
            .unwrap();
        store
            .create_session("old".into(), u.id, "c".into(), crate::now() - 1, None, None)
            .await
            .unwrap();
        assert!(store.session("old".into(), 600).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn display_tokens() {
        let store = Store::memory().unwrap();
        let t = store
            .create_display_token(
                "NOC".into(),
                "th".into(),
                vec!["web".into()],
                "alice".into(),
            )
            .await
            .unwrap();
        assert_eq!(
            store
                .use_display_token("th".into())
                .await
                .unwrap()
                .unwrap()
                .groups,
            ["web"]
        );
        store.revoke_display_token(t.id).await.unwrap();
        assert!(
            store
                .use_display_token("th".into())
                .await
                .unwrap()
                .is_none()
        );
    }
}
