# Alarmlar ve bildirimler

## Kurallar

Kurallar yapılandırma dosyasında tanımlanır:

```toml
[[alerts.rules]]
id = "disk-full"
expr = "disk_used_pct > 90 for 10m"
severity = "critical"
summary = "Dosya sistemi neredeyse dolu"
tags = ["env:prod"]          # isteğe bağlı kapsam: hosts, groups, tags
```

`expr` şu biçimdedir: `<metrik> <işleç> <eşik> [for <süre>]`. Kural, koşul
sürenin tamamı boyunca sağlandığında tetiklenir (firing) ve koşul artık
sağlanmadığında çözülür (resolved). `disk_used_pct` gibi örnek başına
(per-instance) metrikler her bağlama noktası için ayrı alarm üretir. Tüm
metrikler [yapılandırma başvurusunda](configuration.md#kurallarda-kullanılabilen-metrikler)
listelenir; arayüzde **Alerts → Rules** altında da görünür.

### Kural yazmak

- **İşleçler**: `>`, `>=`, `<`, `<=`, `==`, `!=`.
- **Eşik birimleri**: `K`, `M`, `G`, `T` (1000'in kuvvetleri) ve `Ki`,
  `Mi`, `Gi`, `Ti` (1024'ün kuvvetleri). Örneğin `net_rx_bytes > 100M`
  saniyede 100 MB demektir.
- **Süre** (`for`) kısa dalgalanmalarda gereksiz alarmı önler. Süre
  verilmezse koşul ilk sağlandığında alarm tetiklenir.
- **Kapsam**: `hosts`, `groups` ve `tags` listelerinden herhangi biriyle
  eşleşen sunucular kurala dahil olur. Hiçbiri yazılmazsa kural tüm
  sunuculara uygulanır.
- **Önem derecesi**: `info`, `warning` (varsayılan) veya `critical`.

Bir sunucuya ulaşılamadığı sürece onun metrik alarmları çözülmez,
durumlarını korur. Böylece kesinti sırasında "disk dolu" alarmı yanlışlıkla
kapanıp yeniden açılmaz.

### Örnek kurallar

```toml
# CPU 15 dakika boyunca %90'ın üstünde
[[alerts.rules]]
id = "cpu-high"
expr = "cpu_pct > 90 for 15m"

# Çekirdek başına yük 2'nin üstünde
[[alerts.rules]]
id = "load-high"
expr = "load1_per_core > 2 for 10m"

# Bellek ve swap baskısı
[[alerts.rules]]
id = "mem-high"
expr = "mem_used_pct > 95 for 5m"
severity = "critical"

# Inode tükenmesi (disk doluluğundan bağımsız olarak dosya oluşturmayı engeller)
[[alerts.rules]]
id = "inodes"
expr = "disk_inodes_used_pct > 90 for 10m"

# Sanal makinede CPU çalınması (hipervizör aşırı yüklü)
[[alerts.rules]]
id = "steal"
expr = "cpu_steal_pct > 10 for 15m"

# Bekleyen güvenlik güncellemesi olan üretim sunucuları
[[alerts.rules]]
id = "security-updates"
expr = "pending_security_updates > 0"
severity = "info"
tags = ["env:prod"]

# Yeniden başlatma gerektiren sunucular
[[alerts.rules]]
id = "reboot"
expr = "reboot_required == 1 for 1d"
severity = "info"

# Kaba kuvvet SSH denemeleri
[[alerts.rules]]
id = "ssh-attempts"
expr = "ssh_failed_logins_1h > 50"

# Saat kayması (TLS, Kerberos ve günlük sıralamasını bozar)
[[alerts.rules]]
id = "clock-skew"
expr = "clock_skew_secs > 30"

# Durmuş konteynerler
[[alerts.rules]]
id = "containers"
expr = "containers_not_running > 0 for 5m"
groups = ["app"]
```

### Yerleşik kurallar

Yapılandırma gerektirmeyen yerleşik kurallar şunları kapsar:

| Kural | Önem | Ne zaman |
|---|---|---|
| `host_unreachable` | critical | Sunucuya `alerts.unreachable_after` (varsayılan 2 dakika) boyunca ulaşılamadığında veya kimlik doğrulama başarısız olduğunda |
| `host_key_changed` | critical | Sunucu anahtarı onaylanan anahtardan farklı olduğunda |
| `host_key_unknown` | warning | Sunucu anahtarı henüz onaylanmadığında |
| `failed_units` | warning | Başarısız her systemd birimi veya çökmüş OpenRC servisi için ayrı ayrı |
| `cert_expiry` | warning / critical | TLS sertifikasının bitişine `cert_warning_days` (21) / `cert_critical_days` (7) gün kaldığında; denetim başarısız olursa warning |

## Alarmlarla çalışmak

- **Acknowledge** (onayla, operatör): alarmla birinin ilgilendiğini
  gösterir; alarm çözülene kadar hatırlatmalar durur.
- **Silence** (sustur, operatör): bir kuralı, bir sunucuyu veya ikisini
  belirli bir süre için, bir gerekçeyle susturur. Susturulan alarmlar yine
  kaydedilir ama bildirilmez. Planlı bakım çalışmalarında kullanışlıdır.
- Çözülen alarmlar arayüzde yedi gün, veritabanında ise `retention.events`
  süresince görünür kalır.

## Bildirimler

```toml
[[notify]]
id = "ops"
type = "slack"                 # webhook, slack veya email
url = "secret:slack-webhook"   # şifreli saklanır: helmsight secret set slack-webhook
min_severity = "warning"
```

Bildirimler bir alarm tetiklendiğinde, çözüldüğünde ve onaylanmadığı
sürece her `alerts.repeat_interval` (varsayılan 4 saat) aralığında
hatırlatma olarak gönderilir. Teslimatlar başarısız olursa yeniden denenir
ve her alarmla birlikte kaydedilir. Yöneticiler
`POST /api/v1/notify/<id>/test` ile deneme bildirimi gönderebilir.

### Kanal örnekleri

**Slack** (veya Slack uyumlu webhook kabul eden Mattermost, Rocket.Chat
gibi araçlar):

```sh
helmsight secret set slack-webhook     # Webhook adresini girin
```

```toml
[[notify]]
id = "slack-ops"
type = "slack"
url = "secret:slack-webhook"
min_severity = "warning"
```

**E-posta**:

```toml
[[notify]]
id = "mail-oncall"
type = "email"
smtp_host = "smtp.example.com"
smtp_port = 587
smtp_security = "starttls"
smtp_username = "helmsight@example.com"
smtp_password = "secret:smtp"
from = "helmsight@example.com"
to = ["oncall@example.com"]
min_severity = "critical"
```

**Genel webhook** (kendi otomasyonunuz, olay yönetim araçları):

```toml
[[notify]]
id = "hook"
type = "webhook"
url = "https://hooks.example.com/helmsight"
headers = { Authorization = "secret:hook-token" }
groups = ["db"]                # yalnızca db grubunun alarmları
```

Webhook'a gönderilen JSON belgesi
[yapılandırma başvurusunda](configuration.md#notify) açıklanmıştır.

Bir kanala `hosts`, `groups` veya `tags` eklerseniz yalnızca o sunucuların
alarmları gönderilir. Sertifika alarmları gibi bir sunucuya bağlı olmayan
alarmlar yalnızca kapsamsız kanallara gider.
