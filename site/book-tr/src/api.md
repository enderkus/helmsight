# API ve entegrasyonlar

## JSON API

Web arayüzü, `/api/v1` altındaki sürümlü API'nin aynısını kullanır; yani
arayüzde görebildiğiniz her şeye API'den de ulaşabilirsiniz. Koddan üretilen
OpenAPI belgesi `/api/v1/openapi.json` adresinde sunulur ve Swagger UI,
Postman veya bir istemci üreteci ile kullanılabilir.

Kimlik doğrulama, `POST /api/v1/auth/login` ile alınan oturum çerezi
(cookie) ile yapılır. Durum değiştiren istekler, `GET /api/v1/auth/me`
yanıtında dönen `X-CSRF-Token` başlığını göndermelidir:

```sh
curl -c cookies -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"…"}' https://monitor.example.com/api/v1/auth/login
curl -b cookies https://monitor.example.com/api/v1/hosts
```

Otomasyon için ayrı bir `viewer` (veya gerekiyorsa `operator`) kullanıcısı
oluşturun; kişisel hesapları betiklerde kullanmayın.

Sık kullanılan uç noktalar:

| Uç nokta | Açıklama |
|---|---|
| `GET /api/v1/hosts` | Filo özeti |
| `GET /api/v1/hosts/{name}` | Bir sunucu hakkında bilinen her şey |
| `GET /api/v1/hosts/{name}/metrics?series=cpu.*,mem.used_pct&from=…&to=…` | Geçmiş veriler |
| `GET /api/v1/hosts/{name}/changes` | Envanter değişiklikleri |
| `GET /api/v1/compare?a=…&b=…` | İki sunucu arasındaki farklar |
| `GET /api/v1/security` | Filo güvenlik özeti |
| `GET /api/v1/alerts` | Etkin ve yakın zamanda çözülen alarmlar |
| `GET /api/v1/events` | Sunucu gönderimli olaylar (SSE) |
| `GET /healthz` | Canlılık denetimi, kimlik doğrulama gerektirmez |

Tam liste ve şemalar için OpenAPI belgesine bakın.

### Örnek: kritik alarmları listelemek

```sh
curl -sb cookies https://monitor.example.com/api/v1/alerts \
  | jq '.[] | select(.severity == "critical" and .state == "firing") | {host, rule_id, summary}'
```

Yanıtın tam biçimi için OpenAPI belgesindeki `alerts` şemasına bakın.

## Sunucu gönderimli olaylar (SSE)

`GET /api/v1/events` bir olay akışı sunar: `host` olayları (bir sunucunun
özeti her değiştiğinde) ile istemcinin veriyi yeniden çekmesi gerektiğini
bildiren `alerts`, `changes` ve `hostkeys` olayları. `resync` olayı
istemcinin geride kaldığını ve sayfayı yeniden yüklemesi gerektiğini
belirtir.

```sh
curl -Nb cookies https://monitor.example.com/api/v1/events
```

Ters vekil sunucu kullanıyorsanız bu yol için tamponlamayı kapatın
(nginx'te `proxy_buffering off`), aksi hâlde olaylar gecikmeli gelir.

## Prometheus

```toml
[prometheus]
enabled = true
token = "secret:prometheus"
```

Bu ayarla `/metrics`, `Authorization: Bearer <token>` başlığı taşıyan
isteklere toplanan sunucu metriklerini Prometheus metin biçiminde sunar.
Metrik adları [yapılandırma başvurusunda](configuration.md#prometheus)
listelenir.

Bu, helmsight'ı mevcut bir Prometheus + Grafana kurulumuna veri kaynağı
olarak eklemeyi sağlar: sunuculara node_exporter kurmadan temel sistem
metriklerini Prometheus'ta toplayabilir, uzun süreli saklamayı ve
panoları orada yapabilirsiniz.

## Webhook'lar

Webhook kanalları her bildirim için bir JSON belgesi alır; belge biçimi
[yapılandırma başvurusunda](configuration.md#notify) açıklanmıştır.
