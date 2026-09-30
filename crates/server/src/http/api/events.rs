//! Server-sent events for live updates.

use crate::app::App;
use crate::auth::session::Authed;
use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_util::stream::{self, Stream, StreamExt};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// Live updates: `host` (a host summary), `alerts`, `changes` and
/// `hostkeys` events. A `resync` event asks the client to refetch because
/// it fell behind.
#[utoipa::path(get, path = "/events", tag = "hosts",
    responses((status = 200, content_type = "text/event-stream", description = "Event stream")))]
pub async fn stream(
    State(app): State<Arc<App>>,
    _auth: Authed,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = app.events.subscribe();
    let shutdown = app.shutdown.clone();
    let hello = stream::once(async { Ok(SseEvent::default().event("hello").data("{}")) });
    let updates = BroadcastStream::new(rx)
        .map(|msg| match msg {
            Ok(ev) => {
                let data = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
                Ok(SseEvent::default().event(ev.name()).data(data))
            }
            Err(BroadcastStreamRecvError::Lagged(_)) => {
                Ok(SseEvent::default().event("resync").data("{}"))
            }
        })
        .take_until(async move { shutdown.cancelled().await });
    Sse::new(hello.chain(updates)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
