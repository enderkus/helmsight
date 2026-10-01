# API and integrations

## JSON API

The web UI is built on the same versioned API under `/api/v1`. The OpenAPI
document, generated from the code, is served at `/api/v1/openapi.json`.

Authentication uses the session cookie set by `POST /api/v1/auth/login`.
State-changing requests must send the `X-CSRF-Token` header returned by
`GET /api/v1/auth/me`:

```sh
curl -c cookies -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"…"}' https://monitor.example.com/api/v1/auth/login
curl -b cookies https://monitor.example.com/api/v1/hosts
```

Frequently used endpoints:

| Endpoint | Description |
|---|---|
| `GET /api/v1/hosts` | Fleet summary |
| `GET /api/v1/hosts/{name}` | Everything known about a host |
| `GET /api/v1/hosts/{name}/metrics?series=cpu.*,mem.used_pct&from=…&to=…` | History |
| `GET /api/v1/hosts/{name}/changes` | Inventory changes |
| `GET /api/v1/compare?a=…&b=…` | Differences between two hosts |
| `GET /api/v1/security` | Fleet security overview |
| `GET /api/v1/alerts` | Firing and recent alerts |
| `GET /api/v1/events` | Server-sent events |
| `GET /healthz` | Liveness check, no authentication |

## Server-sent events

`GET /api/v1/events` streams `host` events (a host summary whenever it
changes) and `alerts`, `changes` and `hostkeys` events that signal clients
to refetch. A `resync` event means the client fell behind and should reload.

## Prometheus

```toml
[prometheus]
enabled = true
token = "secret:prometheus"
```

`/metrics` then exposes the collected host metrics in the Prometheus text
format to requests with `Authorization: Bearer <token>`. Metric names are
listed in the [configuration reference](configuration.md#prometheus).

## Webhooks

Webhook channels receive a JSON document for every notification; see the
[configuration reference](configuration.md#notify) for the payload.
