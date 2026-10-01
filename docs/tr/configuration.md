# Yapılandırma başvurusu

helmsight tek bir TOML dosyası okur. Aşağıdakilerden ilk bulunan
kullanılır:

1. `--config <yol>`
2. `$HELMSIGHT_CONFIG`
3. `/etc/helmsight/helmsight.toml`
4. `./helmsight.toml`

*[English version](https://enderkus.github.io/helmsight/docs/configuration.html)*

[`examples/helmsight.toml`](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.toml)
her seçeneği yorumlarıyla gösteren bir örnektir. Bir dosyayı
`helmsight config check` ile doğrulayın; hatalar dosyayı, satırı, sütunu ve
anahtarı gösterir:

```text
error: helmsight.toml:23:1: `hosts[1].name`: duplicate host name `web-1`
error: helmsight.toml:41:8: `alerts.rules[0].expr`: unknown operator `=>` (use > >= < <= == !=)
```

Bilinmeyen anahtarlar hatadır; böylece yazım yanlışları sessizce geçmez.

**Süreler** `"500ms"`, `"30s"`, `"5m"`, `"1h30m"`, `"7d"`, `"2w"` gibi
metinlerdir (`ms` milisaniye, `s` saniye, `m` dakika, `h` saat, `d` gün,
`w` hafta).

**Gizli değerler** düz metin olarak yazılabilir, ancak başvuru kullanmak
tercih edilir:

| Biçim | Anlamı |
|---|---|
| `"secret:<ad>"` | `helmsight secret set <ad>` ile veritabanında şifreli saklanan değer |
| `"env:<DEĞİŞKEN>"` | Bir ortam değişkeninin değeri |
| `"file:<yol>"` | Bir dosyanın içeriği (sondaki satır sonu atılır) |

`env:` systemd'nin `EnvironmentFile=` veya konteynerin ortam değişkenleriyle,
`file:` ise Docker/Kubernetes gizli dosyalarıyla (`/run/secrets/...`) iyi
çalışır.

Yapılandırma değişiklikleri yeniden başlatmadan sonra geçerli olur.

## Tam bir örnek

```toml
# En üst düzey anahtarlar herhangi bir [bölüm] başlığından önce gelmelidir.
hosts_file = "hosts.toml"

[server]
listen = "127.0.0.1:8080"
data_dir = "/var/lib/helmsight"
public_url = "https://monitor.example.com"
trusted_proxies = ["127.0.0.1"]

[auth]
require_totp_for_admins = true

[ssh]
user = "monitor"
identity_files = ["/etc/helmsight/id_ed25519"]

[[alerts.rules]]
id = "disk-full"
expr = "disk_used_pct > 90 for 10m"
severity = "critical"

[[notify]]
id = "ops"
type = "slack"
url = "secret:slack-webhook"

[[certs]]
endpoint = "www.example.com:443"
```

TOML kuralları gereği bir `[bölüm]` başlığından sonra yazılan her anahtar o
bölüme ait sayılır. `hosts_file` gibi en üst düzey anahtarları bu yüzden
dosyanın en başına yazın; yanlış yere yazılırsa `config check` bunu
bilinmeyen anahtar olarak bildirir.

## `[server]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `listen` | `"127.0.0.1:8080"` | Dinlenecek adres ve port |
| `data_dir` | `"data"` | Veritabanı, anahtar dosyası, `known_hosts` ve TLS dosyaları; yapılandırma dosyasına göre göreli |
| `public_url` | yok | Dışarıdan erişilen `https://` adresi; OIDC için zorunlu, bildirim bağlantılarında ve köken denetiminde kullanılır |
| `trusted_proxies` | `[]` | `X-Forwarded-For` başlığına güvenilen IP adresleri |

`trusted_proxies` istemci adresinin doğru belirlenmesi için önemlidir:
oturum açma hız sınırlaması istemci adresine göre çalışır. Vekil sunucu
arkasında bu ayar yapılmazsa bütün kullanıcılar vekil sunucunun adresinden
geliyormuş gibi görünür.

### `[server.tls]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `mode` | `"auto"` | `auto` (loopback'te HTTP, diğer adreslerde kendinden imzalı), `self-signed`, `files` veya `off` |
| `cert`, `key` | yok | `mode = "files"` için PEM dosyaları |
| `allow_insecure_http` | `false` | Loopback olmayan bir adreste `mode = "off"` kullanılmasına izin verir (TLS vekil sunucusunun arkasında) |

Let's Encrypt sertifikası kullanıyorsanız `cert` için `fullchain.pem`,
`key` için `privkey.pem` dosyasını gösterin ve sertifika yenilendikten sonra
helmsight'ı yeniden başlatın (ör. certbot'un `--deploy-hook` seçeneğiyle).

## `[auth]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `session_ttl` | `"12h"` | En uzun oturum süresi (en az 5m) |
| `idle_timeout` | `"2h"` | Bu kadar hareketsizlikten sonra oturum sona erer (en az 1m) |
| `require_totp_for_admins` | `false` | Yerel parolası olan yöneticiler arayüzü kullanmadan önce TOTP kaydı yapmak zorundadır |
| `disable_local_login` | `false` | Yalnızca OIDC ile oturum açmaya izin verir |

### `[auth.oidc]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `issuer` | zorunlu | Kimlik sağlayıcının adresi; keşif için `<issuer>/.well-known/openid-configuration` kullanılır |
| `client_id` | zorunlu | |
| `client_secret` | yok | Gizli değer başvurusu önerilir |
| `scopes` | `["openid", "profile", "email"]` | `openid` içermelidir |
| `role_claim` | `"groups"` | Grup veya rol adlarını taşıyan alan (claim); `realm_access.roles` gibi noktalı yollar çalışır |
| `role_map` | `{}` | Alan değeri → `viewer`, `operator` veya `admin`; en yüksek eşleşme kazanır |
| `default_role` | yok | Eşleşmesi olmayan kullanıcıların rolü; ayarlanmazsa erişim reddedilir |
| `label` | `"Single sign-on"` | Oturum açma düğmesinin metni |

Kimlik sağlayıcıda yönlendirme adresi (redirect URI) olarak
`<public_url>/api/v1/auth/oidc/callback` kaydedin. Roller her oturum
açılışında alandan yeniden okunur; yani kimlik sağlayıcıda bir kullanıcının
grubunu değiştirmek bir sonraki girişte rolünü de değiştirir.

Örnek (Keycloak):

```toml
[auth.oidc]
issuer = "https://sso.example.com/realms/ops"
client_id = "helmsight"
client_secret = "secret:oidc"
role_claim = "realm_access.roles"
role_map = { "helmsight-admin" = "admin", "sre" = "operator", "dev" = "viewer" }
label = "Şirket hesabıyla giriş"
```

Yerel yönetici hesabını acil durum girişi olarak tutmanız önerilir; SSO
çalışmadığında arayüze erişimi kaybetmemek için `disable_local_login`
ayarını dikkatle kullanın.

## `[ssh]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `user` | `"monitor"` | Uzak kullanıcı (sunucu bazında değiştirilebilir) |
| `identity_files` | `[]` | Sırayla denenen özel anahtar dosyaları |
| `use_agent` | `true` | `SSH_AUTH_SOCK` üzerindeki anahtarları da dener |
| `known_hosts_files` | `[]` | Salt okunur olarak kullanılan ek OpenSSH known_hosts dosyaları |
| `accept_new_host_keys` | `false` | Yeni sunucuların anahtarlarına otomatik güvenir; değişen anahtarlar yine reddedilir |
| `connect_timeout` | `"10s"` | TCP bağlantısı, anahtar değişimi ve kimlik doğrulama |
| `command_timeout` | `"30s"` | Tek bir veri toplama (güncelleme denetimlerine en az 5 dakika verilir) |
| `keepalive` | `"30s"` | SSH canlı tutma (keepalive) aralığı |

## `[collect]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `interval` | `"5s"` | Metrikler: CPU, bellek, yük, diskler, ağ, TCP, süreçler |
| `medium_interval` | `"60s"` | Dinlenen portlar, servisler, konteynerler, oturumlar |
| `inventory_interval` | `"15m"` | İşletim sistemi, çekirdek, paketler, etkin birimler, yeniden başlatma işaretleri |
| `auth_interval` | `"5m"` | Başarısız SSH girişleri |
| `updates_interval` | `"6h"` | Bekleyen güncellemeler |
| `max_parallel` | `64` | Aynı anda veri toplanan sunucu sayısı |
| `dnf_updates` | `false` | Bekleyen güncellemeler için `dnf`/`yum` çalıştırır (sunucuya günlük dosyası yazarlar) |

Başarısız sunucular artan aralıklarla yeniden denenir: ulaşılamadığında 5
dakikaya kadar üstel olarak, kimlik doğrulama hatalarından sonra (saldırı
önleme sistemlerini tetiklememek için) 1 ila 5 dakika arasında ve sunucu
anahtarı sorunlarında dakikada bir.

`interval` değerini düşürmek grafiklerin çözünürlüğünü artırır ama hem
sunuculardaki hem de veritabanındaki yükü orantılı olarak artırır. Çok
sayıda sunucuda 10–15 saniye makul bir başlangıçtır.

## `[retention]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `raw` | `"24h"` | Her örnek (en az 1h) |
| `minute` | `"7d"` | 1 dakikalık ortalama, en düşük ve en yüksek |
| `five_minute` | `"90d"` | 5 dakikalık ortalama, en düşük ve en yüksek |
| `events` | `"90d"` | Envanter değişiklikleri, çözülmüş alarmlar, başarısız girişler, bildirimler |

Grafikler istenen aralığı kapsayan en ince çözünürlüğü seçer. Süreleri
uzatmak veritabanını orantılı olarak büyütür. Süresi dolan veriler saatte
bir kendiliğinden silinir; özetler dakikada bir hesaplanır.

## Sunucular

### `[[hosts]]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `name` | zorunlu | Benzersiz; harf, rakam, `.`, `-`, `_` |
| `address` | ad ile aynı | Sunucu adı veya IP adresi |
| `port` | `22` | |
| `user` | `ssh.user` | |
| `identity_file` | yok | Ortak anahtarlardan önce denenen anahtar |
| `groups` | `[]` | Süzme, kurallar, eylemler, bildirim kanalları ve duvar ekranı belirteçleri için kullanılır |
| `tags` | `[]` | Serbest biçimli; genellikle `env:prod` gibi `anahtar:değer` |
| `baseline` | yok | Sapma ve yeni port tespiti için referans alınan sunucu |
| `disabled` | `false` | Sunucu listede kalır ama veri toplanmaz |

### `hosts_file`

En üst düzeydeki `hosts_file = "hosts.toml"` başka bir dosyadan
(yapılandırma dosyasına göre göreli) ek `[[hosts]]` kayıtları yükler.

### `[ssh_config_import]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `path` | `"~/.ssh/config"` | OpenSSH istemci yapılandırması |
| `hosts` | zorunlu | İçe aktarılacak takma adların kalıpları, ör. `["web-*"]` |
| `groups`, `tags` | `[]` | İçe aktarılan her sunucuya eklenir |

`HostName`, `User`, `Port` ve `IdentityFile` dikkate alınır; eşleşen
`Host` blokları arasında OpenSSH'nin "ilk değer kazanır" kuralı geçerlidir.
Aynı adla açıkça yazılmış `[[hosts]]` kayıtları önceliklidir. `ProxyJump`
olan sunucular uyarıyla atlanır; `Match` ve `Include` değerlendirilmez.

## Alarmlar

### `[alerts]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `unreachable_after` | `"2m"` | `host_unreachable` tetiklenmeden önceki bekleme |
| `failed_units` | `true` | Başarısız systemd birimleri ve çökmüş OpenRC servisleri için alarm |
| `cert_warning_days` | `21` | |
| `cert_critical_days` | `7` | |
| `repeat_interval` | `"4h"` | Onaylanmamış etkin alarmlar için hatırlatma; `"0s"` kapatır |

Yerleşik kurallar: `host_unreachable` (critical; kimlik doğrulama hataları
için de), `host_key_changed` (critical), `host_key_unknown` (warning),
`failed_units` (warning, birim başına bir alarm), `cert_expiry` (kalan güne
göre warning veya critical; denetim başarısız olursa warning).

### `[[alerts.rules]]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `id` | zorunlu | Benzersiz |
| `expr` | zorunlu | `<metrik> <işleç> <eşik> [for <süre>]` |
| `severity` | `"warning"` | `info`, `warning` veya `critical` |
| `summary` | yok | Üretilen açıklamanın başına eklenir |
| `hosts`, `groups`, `tags` | tüm sunucular | Kapsam; bir sunucu herhangi bir girdiyle eşleşirse dahil olur |

İşleçler: `>`, `>=`, `<`, `<=`, `==`, `!=`. Eşikler `K`, `M`, `G`, `T`
(1000'in kuvvetleri) ve `Ki`, `Mi`, `Gi`, `Ti` (1024'ün kuvvetleri)
soneklerini kabul eder. Örnek başına metrikler her örnek için (ör. her
bağlama noktası için) ayrı alarm üretir. Bir sunucuya ulaşılamadığı sürece
onun metrik alarmları çözülmez, durumlarını korur.

### Kurallarda kullanılabilen metrikler

| Metrik | Birim | Kapsam | Açıklama |
|---|---|---|---|
| `cpu_pct` | % | sunucu | Tüm çekirdeklerde CPU meşguliyeti |
| `cpu_iowait_pct` | % | sunucu | G/Ç beklemesinde geçen CPU zamanı |
| `cpu_steal_pct` | % | sunucu | Hipervizörün çaldığı CPU zamanı |
| `mem_used_pct` | % | sunucu | Geri kazanılabilir önbellek hariç kullanılan bellek |
| `swap_used_pct` | % | sunucu | Kullanılan swap |
| `load1`, `load5`, `load15` | | sunucu | Yük ortalamaları |
| `load1_per_core` | | sunucu | 1 dakikalık yükün çekirdek sayısına bölümü |
| `disk_used_pct` | % | bağlama noktası | Dosya sistemi doluluğu |
| `disk_inodes_used_pct` | % | bağlama noktası | Kullanılan inode oranı |
| `disk_util_pct` | % | aygıt | Blok aygıtı kullanım oranı |
| `net_rx_bytes` | B/sn | arayüz | Saniyede alınan bayt |
| `net_tx_bytes` | B/sn | arayüz | Saniyede gönderilen bayt |
| `net_errors` | 1/sn | arayüz | Saniyedeki alma ve gönderme hataları |
| `tcp_established` | | sunucu | Kurulu TCP bağlantıları |
| `process_count` | | sunucu | Süreç sayısı |
| `failed_units` | | sunucu | Başarısız systemd birimleri veya çökmüş OpenRC servisleri |
| `pending_updates` | | sunucu | Bekleyen paket güncellemeleri |
| `pending_security_updates` | | sunucu | Bekleyen güvenlik güncellemeleri |
| `reboot_required` | | sunucu | Yeniden başlatma gerekiyorsa 1 |
| `ssh_failed_logins_1h` | | sunucu | Son bir saatteki başarısız SSH girişleri |
| `uptime_secs` | sn | sunucu | Açılıştan bu yana geçen saniye |
| `clock_skew_secs` | sn | sunucu | Sunucu saati ile helmsight saati arasındaki fark |
| `containers_not_running` | | sunucu | Var olan ama çalışmayan konteynerler |

## `[[notify]]`

| Anahtar | Geçerli olduğu | Açıklama |
|---|---|---|
| `id` | hepsi | Benzersiz |
| `type` | hepsi | `webhook`, `slack` veya `email` |
| `min_severity` | hepsi | Varsayılan `warning` |
| `hosts`, `groups`, `tags` | hepsi | Yalnızca bu sunucuların alarmları (sertifikalar gibi sunucuya bağlı olmayan alarmlar kapsamsız kanallara gider) |
| `url` | webhook, slack | `https://` adresi (düz `http` yalnızca localhost için) |
| `headers` | webhook | Ek HTTP başlıkları; değerler gizli değer başvurusu olabilir |
| `smtp_host`, `smtp_port` | email | |
| `smtp_security` | email | `starttls` (varsayılan), `tls` veya localhost'taki bir aktarıcı için `none` |
| `smtp_username`, `smtp_password` | email | |
| `from`, `to` | email | Gönderen ve alıcılar |

Bildirimler bir alarm tetiklendiğinde, çözüldüğünde (tetiklenme bildirimi
gönderilmişse) ve hatırlatma olarak gönderilir. Susturulmuş alarmlar
kaydedilir ama bildirilmez. Başarısız teslimatlar üç kez yeniden denenir ve
alarmla birlikte listelenir. API'den deneme gönderin:
`POST /api/v1/notify/<id>/test`.

Webhook gövdesi:

```json
{
  "version": 1,
  "source": "helmsight",
  "status": "firing",
  "alert": {
    "id": 42,
    "rule": "disk-full",
    "host": "web-1",
    "instance": "/var",
    "severity": "critical",
    "summary": "disk_used_pct on /var is 93.1% (> 90.0%)",
    "value": 93.1,
    "started_at": "2026-09-30T16:56:12Z",
    "resolved_at": null,
    "acknowledged_by": null
  },
  "url": "https://monitor.example.com/alerts?id=42"
}
```

`status` değeri `firing`, `resolved`, `reminder` veya `test` olur.

## `[[certs]]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `endpoint` | zorunlu | `sunucu:port` |
| `server_name` | endpoint'teki sunucu | SNI adı ve doğrulanacak ad |
| `interval` | `"6h"` | |

Sertifika doğrulanmasa bile okunur; güven sorunları bağlantı hatalarından
ayrı gösterilir. Denetim helmsight sunucusundan yapılır; yani yalnızca
helmsight'ın erişebildiği uç noktalar denetlenebilir.

## `[[actions]]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `id` | zorunlu | Benzersiz |
| `label` | zorunlu | Düğme metni |
| `description` | `""` | Arayüzde gösterilir |
| `command` | zorunlu | Tam ve tek satırlık komut; yetki gerektiren komutlar `sudo -n ` ile başlamalıdır |
| `hosts`, `groups`, `tags` | zorunlu | Eylemin sunulduğu sunucular; en az bir girdi |
| `role` | `"operator"` | `operator` veya `admin` |
| `timeout` | `"60s"` | 1 saniye ile 1 saat arası |

## `[prometheus]`

| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `enabled` | `false` | `/metrics` uç noktasını açar |
| `token` | etkinse zorunlu | Taşıyıcı belirteç (bearer token) |

```yaml
scrape_configs:
  - job_name: helmsight
    scheme: https
    authorization:
      credentials_file: /etc/prometheus/helmsight.token
    static_configs:
      - targets: ["monitor.example.com"]
```

Sunulan metrikler `helmsight_` önekini taşır: `host_up`,
`host_alerts_firing`, `cpu_percent{mode}`, `memory_*_bytes`, `load1/5/15`,
`filesystem_{size,used,avail}_bytes{mount}`,
`disk_{read,written}_bytes_per_second{device}`,
`network_{receive,transmit}_bytes_per_second{interface}`,
`tcp_connections{state}`, `pending_updates`, `pending_security_updates`,
`reboot_required` ve daha fazlası; hepsi `host` etiketiyle.

## Komut satırı

| Komut | Açıklama |
|---|---|
| `serve [--local] [--listen ADRES]` | Sunucuyu çalıştırır. `--local` yalnızca bu makineyi `/proc` okuyarak izler (SSH ve yapılandırma gerekmez). |
| `init [--force]` | Etkileşimli olarak yapılandırma dosyasını, veri dizinini, anahtar dosyasını ve ilk yöneticiyi oluşturur |
| `user add <ad> [--role viewer\|operator\|admin] [--password-stdin]` | Yerel kullanıcı oluşturur |
| `user remove <ad>` | Kullanıcıyı siler (son yönetici silinemez) |
| `user reset-password <ad> [--password-stdin] [--reset-totp]` | Yeni parola belirler ve kullanıcının oturumlarını sonlandırır |
| `user list` | Kullanıcıları listeler |
| `hosts test [--trust] [adlar...]` | Sunuculara bağlanıp durumlarını bildirir; `--trust` bilinmeyen sunucu anahtarlarını etkileşimli olarak onaylatır |
| `config check` | Yapılandırmayı doğrular ve başvurulan gizli değerlerin var olduğunu denetler |
| `secret set <ad> [--stdin]` / `secret list` / `secret delete <ad>` | Şifreli gizli değerleri yönetir |
| `audit verify` | Denetim kaydının özet zincirini doğrular |

Genel seçenekler: `--config <yol>`, `--data-dir <dizin>` (`server.data_dir`
değerini geçersiz kılar; yapılandırma dosyası olmadan komutlar yerel kipin
varsayılanı olan `~/.local/share/helmsight` dizinini, root için
`/var/lib/helmsight` dizinini kullanır), `--log-format text|json`.

Hiç kullanıcı yokken `serve`, ilk yöneticiyi tarayıcıda oluşturmak için tek
kullanımlık bir kurulum bağlantısı (`/setup#token=...`) yazdırır.

## Ortam değişkenleri

| Değişken | Açıklama |
|---|---|
| `HELMSIGHT_CONFIG` | Yapılandırma dosyasının yolu |
| `HELMSIGHT_DATA_DIR` | `--data-dir` ile aynı |
| `HELMSIGHT_LOG` | Günlük süzgeci, ör. `info`, `debug`, `info,server=debug` |
| `SSH_AUTH_SOCK` | `ssh.use_agent = true` olduğunda kullanılan ssh-agent soketi |

## HTTP API

Web arayüzü, `/api/v1` altındaki sürümlü JSON API'nin aynısını kullanır.
OpenAPI belgesi `/api/v1/openapi.json` adresinde sunulur. Kimlik doğrulama
için `POST /api/v1/auth/login` ile alınan oturum çerezini kullanın; durum
değiştiren istekler `GET /api/v1/auth/me` yanıtında dönen `X-CSRF-Token`
başlığını gerektirir. Canlı güncellemeler `/api/v1/events` adresinde
sunucu gönderimli olaylar olarak sunulur. Duvar ekranları
`/api/v1/display/state` adresini `Authorization: Bearer <belirteç>`
başlığıyla çağırır. `/healthz` kimlik doğrulama olmadan `ok` döndürür.
