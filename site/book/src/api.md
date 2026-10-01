# API and integrations

## JSON API

The web UI is built on the same versioned API under `/api/v1`, so
everything visible in the UI is also available through the API. The
OpenAPI document, generated from the code, is served at
`/api/v1/openapi.json` and works with Swagger UI, Postman or a client
generator.

Authentication uses the session cookie set by `POST /api/v1/auth/login`.
State-changing requests must send the `X-CSRF-Token` header returned by
`GET /api/v1/auth/me`:

```sh
curl -c cookies -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"…"}' https://monitor.example.com/api/v1/auth/login
curl -b cookies https://monitor.example.com/api/v1/hosts
```

Create a separate `viewer` user (or `operator` if needed) for automation;
do not use personal accounts in scripts.

Frequently used endpoints:

| Endpoint | Description |
|---|---|
| `GET /api/v1/hosts` | Fleet summary |
| `GET /api/v1/hosts/{name}` | Everything known about a host |
| `GET /api/v1/hosts/{name}/metrics?series=cpu.*,mem.used_pct&from=…&to=…` | History |
| `GET /api/v1/hosts/{name}/changes` | Inventory changes |
| `GET /api/v1/compare?a=…&b=…` | Differences between two hosts |
| `GET /api/v1/security` | Fleet security overview |
| `GET /api/v1/alerts` | Firing and recently resolved alerts |
| `GET /api/v1/events` | Server-sent events |
| `GET /healthz` | Liveness check, no authentication |

See the OpenAPI document for the full list and the schemas.

### Example: list critical alerts

```sh
curl -sb cookies https://monitor.example.com/api/v1/alerts \
  | jq '.[] | select(.severity == "critical" and .state == "firing") | {host, rule_id, summary}'
```

See the `alerts` schema in the OpenAPI document for the full response
format.

## Server-sent events

`GET /api/v1/events` streams `host` events (a host summary whenever it
changes) and `alerts`, `changes` and `hostkeys` events that signal clients
to refetch. A `resync` event means the client fell behind and should reload.

```sh
curl -Nb cookies https://monitor.example.com/api/v1/events
```

Behind a reverse proxy, turn off buffering for this path
(`proxy_buffering off` in nginx); otherwise events arrive late.

## Prometheus

```toml
[prometheus]
enabled = true
token = "secret:prometheus"
```

`/metrics` then exposes the collected host metrics in the Prometheus text
format to requests with `Authorization: Bearer <token>`. Metric names are
listed in the [configuration reference](configuration.md#prometheus).

This makes helmsight a data source for an existing Prometheus and Grafana
setup: basic system metrics reach Prometheus without installing
node_exporter on the hosts, and long-term storage and dashboards can live
there.

## Webhooks

Webhook channels receive a JSON document for every notification; see the
[configuration reference](configuration.md#notify) for the payload.
