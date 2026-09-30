//! Schema migrations, tracked with `PRAGMA user_version`.

use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
CREATE TABLE hosts (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL
);

CREATE TABLE series_keys (
    id   INTEGER PRIMARY KEY,
    key  TEXT NOT NULL UNIQUE
);

-- One row per host and tick; `data` holds (series id u32, value f32) pairs.
CREATE TABLE samples_raw (
    host_id  INTEGER NOT NULL,
    ts       INTEGER NOT NULL,
    data     BLOB NOT NULL,
    PRIMARY KEY (host_id, ts)
) WITHOUT ROWID;

-- Rollups hold (series id u32, avg f32, min f32, max f32) tuples.
CREATE TABLE samples_1m (
    host_id  INTEGER NOT NULL,
    ts       INTEGER NOT NULL,
    data     BLOB NOT NULL,
    PRIMARY KEY (host_id, ts)
) WITHOUT ROWID;

CREATE TABLE samples_5m (
    host_id  INTEGER NOT NULL,
    ts       INTEGER NOT NULL,
    data     BLOB NOT NULL,
    PRIMARY KEY (host_id, ts)
) WITHOUT ROWID;

CREATE TABLE rollup_state (
    level       TEXT PRIMARY KEY,
    done_until  INTEGER NOT NULL
);

-- Latest value of slow-changing data per host (JSON), kept across restarts.
CREATE TABLE host_latest (
    host_id  INTEGER NOT NULL,
    kind     TEXT NOT NULL,
    ts       INTEGER NOT NULL,
    data     TEXT NOT NULL,
    PRIMARY KEY (host_id, kind)
);

CREATE TABLE changes (
    id       INTEGER PRIMARY KEY,
    host_id  INTEGER NOT NULL,
    ts       INTEGER NOT NULL,
    kind     TEXT NOT NULL,
    action   TEXT NOT NULL,
    subject  TEXT NOT NULL,
    old      TEXT,
    new      TEXT
);
CREATE INDEX changes_host_ts ON changes(host_id, ts);

CREATE TABLE auth_failures (
    host_id  INTEGER NOT NULL,
    ts       INTEGER NOT NULL,
    user     TEXT NOT NULL,
    ip       TEXT,
    invalid  INTEGER NOT NULL,
    method   TEXT NOT NULL
);
CREATE INDEX auth_failures_host_ts ON auth_failures(host_id, ts);
CREATE INDEX auth_failures_ts ON auth_failures(ts);

CREATE TABLE alerts (
    id                INTEGER PRIMARY KEY,
    fingerprint       TEXT NOT NULL,
    rule_id           TEXT NOT NULL,
    host              TEXT,
    instance          TEXT,
    severity          TEXT NOT NULL,
    state             TEXT NOT NULL,
    summary           TEXT NOT NULL,
    value             REAL,
    started_at        INTEGER NOT NULL,
    resolved_at       INTEGER,
    last_notified_at  INTEGER,
    ack_by            TEXT,
    ack_at            INTEGER,
    ack_note          TEXT
);
CREATE UNIQUE INDEX alerts_open ON alerts(fingerprint) WHERE state = 'firing';
CREATE INDEX alerts_started ON alerts(started_at);

CREATE TABLE silences (
    id          INTEGER PRIMARY KEY,
    rule_id     TEXT,
    host        TEXT,
    reason      TEXT NOT NULL,
    created_by  TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    ends_at     INTEGER NOT NULL,
    expired_by  TEXT
);

CREATE TABLE notifications (
    id        INTEGER PRIMARY KEY,
    alert_id  INTEGER NOT NULL,
    channel   TEXT NOT NULL,
    ts        INTEGER NOT NULL,
    ok        INTEGER NOT NULL,
    error     TEXT
);
CREATE INDEX notifications_alert ON notifications(alert_id);

CREATE TABLE cert_status (
    endpoint    TEXT PRIMARY KEY,
    checked_at  INTEGER NOT NULL,
    not_after   INTEGER,
    subject     TEXT,
    issuer      TEXT,
    error       TEXT,
    trust_error TEXT
);

CREATE TABLE users (
    id                    INTEGER PRIMARY KEY,
    username              TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name          TEXT,
    password_hash         TEXT,
    role                  TEXT NOT NULL,
    totp_secret           TEXT,
    totp_enabled          INTEGER NOT NULL DEFAULT 0,
    oidc_subject          TEXT UNIQUE,
    created_at            INTEGER NOT NULL,
    last_login_at         INTEGER,
    disabled              INTEGER NOT NULL DEFAULT 0,
    must_change_password  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sessions (
    id_hash       TEXT PRIMARY KEY,
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf          TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    expires_at    INTEGER NOT NULL,
    last_seen_at  INTEGER NOT NULL,
    ip            TEXT,
    user_agent    TEXT
);
CREATE INDEX sessions_user ON sessions(user_id);

CREATE TABLE display_tokens (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    token_hash    TEXT NOT NULL UNIQUE,
    groups        TEXT NOT NULL,
    created_by    TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    revoked_at    INTEGER,
    last_used_at  INTEGER
);

CREATE TABLE secrets (
    name        TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- Append-only, hash-chained audit log.
CREATE TABLE audit_log (
    id         INTEGER PRIMARY KEY,
    ts         INTEGER NOT NULL,
    actor      TEXT NOT NULL,
    action     TEXT NOT NULL,
    target     TEXT,
    detail     TEXT NOT NULL,
    prev_hash  TEXT NOT NULL,
    hash       TEXT NOT NULL
);
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit log is append-only'); END;
CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'audit log is append-only'); END;
"#,
];

pub fn apply(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let current = usize::try_from(version).unwrap_or(0);
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i64::try_from(i + 1).unwrap_or(0))?;
        tx.commit()?;
        tracing::info!(version = i + 1, "applied database migration");
    }
    Ok(())
}
