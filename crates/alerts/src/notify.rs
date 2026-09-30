//! Notification channels: generic JSON webhook, Slack-compatible webhook and
//! SMTP email.

use common::config::{NotifyKind, Severity};
use common::selector::Selector;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use store::alerts::AlertRecord;
use zeroize::Zeroizing;

/// A channel with its secrets resolved.
#[derive(Clone)]
pub struct Channel {
    pub id: String,
    pub kind: NotifyKind,
    pub min_severity: Severity,
    pub scope: Selector,
    pub url: Option<Zeroizing<String>>,
    pub headers: Vec<(String, Zeroizing<String>)>,
    pub smtp: Option<SmtpSettings>,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct SmtpSettings {
    pub host: String,
    pub port: Option<u16>,
    /// `starttls`, `tls` or `none`.
    pub security: String,
    pub username: Option<String>,
    pub password: Option<Zeroizing<String>>,
    pub from: String,
    pub to: Vec<String>,
}

/// Why a notification is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Firing,
    Resolved,
    Reminder,
    Test,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Firing => "FIRING",
            Status::Resolved => "RESOLVED",
            Status::Reminder => "STILL FIRING",
            Status::Test => "TEST",
        }
    }
}

/// A notification to deliver.
#[derive(Debug, Clone)]
pub struct Event {
    pub status: Status,
    pub alert: AlertRecord,
    /// Link to the alert in the UI, when `public_url` is configured.
    pub link: Option<String>,
    pub product: String,
}

#[derive(Serialize)]
struct WebhookPayload<'a> {
    version: u32,
    source: &'a str,
    status: Status,
    alert: WebhookAlert<'a>,
    url: Option<&'a str>,
}

#[derive(Serialize)]
struct WebhookAlert<'a> {
    id: i64,
    rule: &'a str,
    host: Option<&'a str>,
    instance: Option<&'a str>,
    severity: &'a str,
    summary: &'a str,
    value: Option<f64>,
    started_at: String,
    resolved_at: Option<String>,
    acknowledged_by: Option<&'a str>,
}

/// Formats unix seconds as RFC 3339 UTC.
pub fn rfc3339(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Escapes text for Slack mrkdwn; alert data may contain host-controlled text.
fn slack_escape(s: &str) -> String {
    clean(s)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn headline(ev: &Event) -> String {
    let a = &ev.alert;
    let mut s = format!("[{}] {}", ev.status.label(), a.severity);
    if let Some(h) = &a.host {
        s.push_str(&format!(" · {h}"));
    }
    s.push_str(&format!(" · {}", a.summary));
    clean(&s)
}

/// Delivers notifications over HTTP and SMTP.
#[derive(Clone)]
pub struct Notifier {
    http: reqwest::Client,
}

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Notifier")
    }
}

/// Root certificates: the bundled Mozilla set plus the system store, so that
/// private CAs installed on the server are honoured.
pub fn root_store() -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    roots
}

/// A rustls client configuration using the ring provider.
pub fn client_tls_config() -> Result<rustls::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(root_store())
        .with_no_client_auth())
}

impl Notifier {
    pub fn new(user_agent: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .use_preconfigured_tls(client_tls_config()?)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(user_agent)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http })
    }

    /// Whether `ch` should receive a notification for this alert.
    pub fn accepts(
        ch: &Channel,
        severity: Severity,
        host_matches: impl Fn(&Selector) -> bool,
    ) -> bool {
        severity >= ch.min_severity && (ch.scope.is_empty() || host_matches(&ch.scope))
    }

    /// Sends with up to three attempts. Returns the last error.
    pub async fn send(&self, ch: &Channel, ev: &Event) -> Result<(), String> {
        let mut last = String::new();
        for (attempt, delay) in [0u64, 2, 10].into_iter().enumerate() {
            if delay > 0 {
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
            match self.send_once(ch, ev).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(channel = %ch.id, attempt = attempt + 1, error = %e, "notification failed");
                    last = e;
                }
            }
        }
        Err(last)
    }

    async fn send_once(&self, ch: &Channel, ev: &Event) -> Result<(), String> {
        match ch.kind {
            NotifyKind::Webhook => self.webhook(ch, ev).await,
            NotifyKind::Slack => self.slack(ch, ev).await,
            NotifyKind::Email => email(ch, ev).await,
        }
    }

    async fn post(&self, ch: &Channel, body: &serde_json::Value) -> Result<(), String> {
        let url = ch.url.as_ref().ok_or("channel has no URL")?;
        let mut req = self.http.post(url.as_str()).json(body);
        for (k, v) in &ch.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().await.map_err(describe_http_error)?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(format!("endpoint returned HTTP {}", status.as_u16()))
        }
    }

    async fn webhook(&self, ch: &Channel, ev: &Event) -> Result<(), String> {
        let a = &ev.alert;
        let payload = WebhookPayload {
            version: 1,
            source: &ev.product,
            status: ev.status,
            alert: WebhookAlert {
                id: a.id,
                rule: &a.rule_id,
                host: a.host.as_deref(),
                instance: a.instance.as_deref(),
                severity: &a.severity,
                summary: &a.summary,
                value: a.value,
                started_at: rfc3339(a.started_at),
                resolved_at: a.resolved_at.map(rfc3339),
                acknowledged_by: a.ack_by.as_deref(),
            },
            url: ev.link.as_deref(),
        };
        let v = serde_json::to_value(&payload).map_err(|e| e.to_string())?;
        self.post(ch, &v).await
    }

    async fn slack(&self, ch: &Channel, ev: &Event) -> Result<(), String> {
        let text = slack_text(ev);
        self.post(ch, &serde_json::json!({ "text": text })).await
    }
}

fn slack_text(ev: &Event) -> String {
    let a = &ev.alert;
    let mut lines = vec![format!("*{}*", slack_escape(&headline(ev)))];
    let mut meta = vec![format!("rule `{}`", slack_escape(&a.rule_id))];
    if let Some(i) = &a.instance {
        meta.push(format!("instance `{}`", slack_escape(i)));
    }
    meta.push(format!("since {}", rfc3339(a.started_at)));
    if let Some(by) = &a.ack_by {
        meta.push(format!("acknowledged by {}", slack_escape(by)));
    }
    lines.push(meta.join(" · "));
    if let Some(link) = &ev.link {
        lines.push(format!(
            "<{}|Open in {}>",
            slack_escape(link),
            slack_escape(&ev.product)
        ));
    }
    lines.join("\n")
}

fn describe_http_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "request timed out".into()
    } else if e.is_connect() {
        "could not connect to endpoint".into()
    } else {
        // Strip the URL (it may contain a secret token).
        e.without_url().to_string()
    }
}

async fn email(ch: &Channel, ev: &Event) -> Result<(), String> {
    let smtp = ch.smtp.as_ref().ok_or("channel has no SMTP settings")?;
    let subject = format!("[{}] {}", ev.product, headline(ev));
    let a = &ev.alert;
    let mut body = String::new();
    body.push_str(&format!("{}\n\n", clean(&a.summary)));
    body.push_str(&format!("Status:    {}\n", ev.status.label()));
    body.push_str(&format!("Severity:  {}\n", a.severity));
    body.push_str(&format!("Rule:      {}\n", clean(&a.rule_id)));
    if let Some(h) = &a.host {
        body.push_str(&format!("Host:      {}\n", clean(h)));
    }
    if let Some(i) = &a.instance {
        body.push_str(&format!("Instance:  {}\n", clean(i)));
    }
    body.push_str(&format!("Started:   {}\n", rfc3339(a.started_at)));
    if let Some(r) = a.resolved_at {
        body.push_str(&format!("Resolved:  {}\n", rfc3339(r)));
    }
    if let Some(by) = &a.ack_by {
        body.push_str(&format!("Acknowledged by {}\n", clean(by)));
    }
    if let Some(link) = &ev.link {
        body.push_str(&format!("\n{link}\n"));
    }

    let mut builder = Message::builder()
        .from(
            smtp.from
                .parse()
                .map_err(|e| format!("invalid from address: {e}"))?,
        )
        .subject(subject.chars().take(200).collect::<String>())
        .header(ContentType::TEXT_PLAIN);
    for to in &smtp.to {
        builder = builder.to(to
            .parse()
            .map_err(|e| format!("invalid recipient {to}: {e}"))?);
    }
    let msg = builder.body(body).map_err(|e| e.to_string())?;

    let transport = match smtp.security.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host),
        "none" => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            &smtp.host,
        )),
        _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host),
    }
    .map_err(|e| e.to_string())?;
    let mut transport = transport.timeout(Some(Duration::from_secs(20)));
    if let Some(p) = smtp.port {
        transport = transport.port(p);
    }
    if let (Some(u), Some(p)) = (&smtp.username, &smtp.password) {
        transport = transport.credentials(Credentials::new(u.clone(), p.to_string()));
    }
    transport
        .build()
        .send(msg)
        .await
        .map(|_| ())
        .map_err(|e| format!("SMTP delivery failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert() -> AlertRecord {
        AlertRecord {
            id: 7,
            fingerprint: "f".into(),
            rule_id: "failed_units".into(),
            host: Some("web-1".into()),
            instance: Some("<evil>&unit\n.service".into()),
            severity: "warning".into(),
            state: "firing".into(),
            summary: "Service failed".into(),
            value: Some(1.0),
            started_at: 1_790_787_372,
            resolved_at: None,
            last_notified_at: None,
            ack_by: None,
            ack_at: None,
            ack_note: None,
        }
    }

    #[test]
    fn formats_timestamps() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_790_787_372), "2026-09-30T16:56:12Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn slack_text_is_escaped() {
        let ev = Event {
            status: Status::Firing,
            alert: alert(),
            link: Some("https://m.example.com/alerts/7".into()),
            product: "helmsight".into(),
        };
        let t = slack_text(&ev);
        assert!(t.contains("&lt;evil&gt;&amp;unit .service"), "{t}");
        assert!(t.starts_with("*[FIRING] warning · web-1 · Service failed*"));
        assert!(t.contains("<https://m.example.com/alerts/7|Open in helmsight>"));
    }

    #[tokio::test]
    async fn webhook_delivery_to_local_server() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let mut req = Vec::new();
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                req.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&req);
                if let Some(i) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if req.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            sock.write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8_lossy(&req).to_string()
        });
        let n = Notifier::new("test").unwrap();
        let ch = Channel {
            id: "hook".into(),
            kind: NotifyKind::Webhook,
            min_severity: Severity::Warning,
            scope: Selector::default(),
            url: Some(Zeroizing::new(format!("http://{addr}/hook"))),
            headers: vec![("x-token".into(), Zeroizing::new("s3cret".into()))],
            smtp: None,
        };
        let ev = Event {
            status: Status::Resolved,
            alert: alert(),
            link: None,
            product: "helmsight".into(),
        };
        n.send_once(&ch, &ev).await.unwrap();
        let req = server.await.unwrap();
        assert!(req.starts_with("POST /hook"));
        assert!(req.contains("x-token: s3cret"));
        let body = &req[req.find("\r\n\r\n").unwrap() + 4..];
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v["status"], "resolved");
        assert_eq!(v["alert"]["host"], "web-1");
        assert_eq!(v["alert"]["started_at"], "2026-09-30T16:56:12Z");
    }
}
