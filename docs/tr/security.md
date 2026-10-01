# Güvenlik modeli

Bu belge helmsight'ın izlenen sunucularda ne yaptığını, neyi yapamayacağını,
en az yetkiyle nasıl çalıştırılacağını ve bir güvenlik açığının nasıl
bildirileceğini anlatır.

*[English version](https://enderkus.github.io/helmsight/docs/security.html)*

## Tehdit modeli

helmsight'ta üç taraf vardır:

1. **Merkez sunucu** helmsight programını çalıştırır. SSH özel
   anahtarlarını, veritabanını (metrikler, envanter, kullanıcılar,
   oturumlar, denetim kaydı) ve saklanan gizli değerleri şifreleyen bir
   anahtar dosyasını tutar.
2. **İzlenen sunuculara** SSH ile bağlanılır. Çıktılarına güvenilmez: ele
   geçirilmiş bir sunucu ya da bir süreç adını, bir günlük satırını veya bir
   dosya adını kontrol eden yetkisiz bir yerel kullanıcı, helmsight'a
   kötü niyetli veri göndermeye çalışabilir.
3. **Kullanıcılar** web arayüzüne HTTP(S) üzerinden bir rolle erişir:
   viewer (izleyici), operator (operatör) veya admin (yönetici). Duvar
   ekranları iptal edilebilir, salt okunur belirteçler kullanır.

helmsight şu güvenceleri verecek şekilde tasarlanmıştır:

- **Ele geçirilmiş bir izlenen sunucu tarayıcıya veya merkez sunucuya
  saldıramaz.** Çıktının boyutu sınırlanır (çalıştırma başına 16 MiB) ve
  bozuk girdide asla çökmeyen (panic) Rust koduyla ayrıştırılır. Bu, gerçek
  çıktının kesilmiş ve bozulmuş hâlleriyle çalışan fuzz benzeri testlerle
  denetlenir. Değerler HTML olarak değil, her zaman metin olarak gösterilir.
  Tarayıcı yalnızca programın sunduğu kodu çalıştırır; İçerik Güvenliği
  Politikası (Content-Security-Policy) satır içi betikleri, satır içi
  stilleri, çerçeveye alınmayı ve üçüncü taraf kaynakları yasaklar.
  Betik çıktısındaki bölüm işaretleri, uzak süreç listesinde hiçbir zaman
  görünmeyen ve her çalıştırmada değişen rastgele bir değer (nonce) içerir;
  böylece bir süreç veya günlük satırı sahte bir bölüm üretemez.
- **Ele geçirilmiş bir izlenen sunucu diğer sunuculara ulaşamaz.** Her
  sunucunun kendi SSH oturumu vardır; hiçbir şey yönlendirilmez (ajan
  yönlendirme yok, port yönlendirme yok).
- **Bir arayüz kullanıcısı rastgele komut çalıştıramaz.** Terminal, dosya
  yöneticisi veya komut girişi yoktur. Eylemler yapılandırma dosyasındaki
  sabit komut metinleridir; API yalnızca bir eylem kimliği ve bir sunucu
  adı kabul eder ve kullanıcının rolünün ve sunucunun buna izin verdiğini
  denetler.
- **Ağdaki bir saldırgan bir sunucunun kimliğine bürünemez.** SSH sunucu
  anahtarları OpenSSH `known_hosts` kurallarıyla doğrulanır. Bilinmeyen
  anahtarlar, bir yönetici parmak izini onaylayana kadar (veya
  `accept_new_host_keys` açık değilse) reddedilir. Değişen bir anahtar her
  zaman veri toplamayı durdurur, kritik bir alarm üretir ve yeni parmak
  izinin bir yönetici tarafından doğrulanmasını gerektirir.

Kapsam dışı: merkez sunucuda root yetkisi elde eden bir saldırgan SSH
anahtarlarını ve veritabanını okuyabilir. O makineyi buna göre koruyun.

### Pratikte ne anlama gelir

- helmsight'ın izleme anahtarı çalınırsa saldırgan yalnızca yetkisiz
  `monitor` hesabıyla ve (`from=` kullandıysanız) yalnızca helmsight
  sunucusunun adresinden giriş yapabilir. Bu yüzden `restrict,from=`
  seçeneklerini atlamayın.
- Bir sunucu ele geçirilirse en kötü ihtimalle o sunucuya ait yanlış
  metrikler gösterilir; saldırgan bunu helmsight'a veya diğer sunuculara
  sıçramak için kullanamaz.
- Merkez sunucu filonun tamamına okuma erişimi olan bir makinedir;
  bir sıçrama sunucusu (bastion) kadar özenle korunmalıdır: güncel tutun,
  yalnızca gerekli portları açın ve arayüze erişimi sınırlayın.

## helmsight'ın izlenen sunucularda çalıştırdıkları

helmsight her turda sunucunun kalıcı SSH oturumunda bir kanal açar ve
`sh -s` çalıştırır; toplama betiği standart girdi üzerinden gönderilir.
Betik
[`crates/collect/src/remote.sh`](https://github.com/enderkus/helmsight/blob/main/crates/collect/src/remote.sh)
dosyasındadır; kullanmadan önce inceleyin. Yalnızca okuma işlemleri içerir:

| Grup | Aralık | Okuduğu |
|---|---|---|
| metrics | 5 sn | `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/uptime`, `/proc/diskstats`, `/proc/net/dev`, `/proc/mounts`, `/proc/net/tcp{,6}`, `/proc/[pid]/stat`, `df -Pk`, `df -Pi`, `ps -o pid,user,args`, `date` |
| medium | 60 sn | `ss -ltunp` (veya `netstat`, veya `/proc/net/*`), `systemctl list-units` veya `rc-status`, `docker`/`podman ps` ve `stats --no-stream`, `who` |
| inventory | 15 dk | `/etc/os-release`, `uname`, `/proc/cpuinfo`, `lscpu`, `systemd-detect-virt`, DMI üretici ve ürün bilgisi, yeniden başlatma işaretleri, `rpm -q --last kernel-core`, `dpkg-query -W`, `rpm -qa` veya `apk info -v`, `systemctl list-unit-files` veya `rc-update show` |
| auth | 5 dk | Son denetimden bu yana `journalctl _COMM=sshd`, ya da `/var/log/auth.log`, `/var/log/secure` veya `/var/log/messages` dosyalarının sonu |
| updates | 6 sa | `apt-get -s dist-upgrade` (yalnızca benzetim, kilit almaz), `zypper --no-refresh list-updates`, `apk version` ve yalnızca etkinleştirilmişse `dnf -C check-update` |

Betiğin özellikleri:

- Yalnızca POSIX `sh`; dash, bash ve BusyBox ash altında çalışır.
- Bütün çıktı yönlendirmeleri `/dev/null`'a gider (bir birim testi bunu
  zorunlu kılar).
- Eksik veya izin verilmeyen araçlar hata değil, gerekçesiyle birlikte
  "n/a" üretir.
- Uzun sürebilecek komutlar, varsa `timeout` ile sınırlandırılır.
- Betik uzak sunucuda bir dosyaya kaydedilmez; standart girdiden okunur ve
  süreç listesinde yalnızca `sh -s` olarak görünür.

### Sunucuda neler yazılır

helmsight'ın komutları hiçbir şey yazmaz. Entegrasyon test takımı Debian,
Ubuntu, Rocky, Fedora, openSUSE ve Alpine üzerinde yetkisiz bir kullanıcı
olarak oturum açar, tüm toplama gruplarını çalıştırır ve `/proc`, `/sys`,
`/dev`, `/run` ve `/var/log` dışında hiçbir dosyanın değişmediğini doğrular.

Her SSH girişinde olduğu gibi, oturum açmanın yan etkisi olarak yine de
şunlar olur:

- Sunucunun giriş kayıtları oturumu kaydeder (`wtmp`, `lastlog`, journal
  veya syslog).
- Ubuntu'da `pam_motd`, izleme hesabının ilk girişinde ev dizininde
  `~/.cache/motd.legal-displayed` dosyasını oluşturur.

Sizin denetiminizdeki istisnalar:

- **dnf/yum**: `dnf` ve `yum` her zaman kendi günlük dosyalarını ve yetkisiz
  kullanıcılar için `/var/tmp` altında bir önbellek yazar. Bu yüzden RHEL
  ailesindeki sunucularda bekleyen güncellemeler yalnızca
  `collect.dnf_updates = true` olduğunda listelenir.
- **root olarak apt**: root hesabıyla izleme yapıyorsanız (önerilmez),
  `apt-get -s` `/var/cache/apt` altındaki ikili paket önbelleğini
  yenileyebilir.
- **Eylemler** tam olarak bir yöneticinin yapılandırdığı komutları
  çalıştırır ve bunlar sunucuyu değiştirebilir. Yapılandırılmadıkça
  kapalıdır.

## İzleme hesabını oluşturmak

İzlenen her sunucuda:

```sh
# authorized_keys için ev dizini olan yetkisiz bir hesap.
useradd --create-home --shell /bin/sh monitor
install -d -m 700 -o monitor -g monitor ~monitor/.ssh

# Anahtarı kısıtlayın: PTY yok, yönlendirme yok, yalnızca helmsight sunucusundan.
echo 'restrict,from="10.0.0.5" ssh-ed25519 AAAA... helmsight' \
  > ~monitor/.ssh/authorized_keys
chown monitor:monitor ~monitor/.ssh/authorized_keys
chmod 600 ~monitor/.ssh/authorized_keys
```

`restrict` port, ajan ve X11 yönlendirmesini ve PTY ayırmayı kapatır;
helmsight bunların hiçbirine ihtiyaç duymaz. `10.0.0.5` yerine helmsight
sunucusunun adresini yazın.

Hesaba parola atamayın; giriş yalnızca anahtarla yapılmalıdır.
Sunucularınızda `sshd_config` içinde `PasswordAuthentication no`
kullanmanız da önerilir.

İsteğe bağlı grup üyelikleri; her biri görünürlüğü artırır:

| Grup | Sağladığı | Not |
|---|---|---|
| `systemd-journal` (Debian/Ubuntu'da `adm` de olur) | başarısız SSH giriş geçmişi | günlüklere salt okunur erişim |
| `docker` | konteyner listesi ve kaynak kullanımı | **root'a eşdeğerdir**; yalnızca bunu kabul ediyorsanız verin |

Bunlar olmadan helmsight ilgili görünümde gerekçesiyle birlikte "n/a"
gösterir.

helmsight'ın kullandığı SSH anahtarı yalnızca ona ayrılmalı ve ed25519
türünde olmalıdır
(`ssh-keygen -t ed25519 -f /etc/helmsight/id_ed25519 -N ""`). RSA
anahtarları da çalışır; ancak kullanılan RSA kütüphanesinde özel anahtar
işlemlerinde bilinen bir zamanlama yan kanalı vardır (RUSTSEC-2023-0071),
bu yüzden ed25519'u tercih edin. Anahtarlar dosyalardan
(`ssh.identity_files`, yalnızca helmsight kullanıcısının okuyabileceği
şekilde) veya ssh-agent'tan yüklenebilir. Parolalı anahtarlar ssh-agent'a
yüklenmelidir.

### Sunucu anahtarlarına güvenmek

Yeni sunucular, anahtarlarına güvenilene kadar reddedilir. İki yol vardır:

- her parmak izini gösterip onay isteyen `helmsight hosts test --trust`
  komutunu çalıştırın; veya
- anahtarı arayüzde **Host keys** sayfasından onaylayın. Yönetici, sunucuda
  `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` komutunun yazdırdığı
  parmak izini yapıştırmalıdır; onay ancak sunulan anahtarla eşleşirse
  gerçekleşir.

Onaylanan anahtarlar `<data_dir>/known_hosts` dosyasında saklanır. Mevcut
OpenSSH dosyaları `ssh.known_hosts_files` ile salt okunur olarak
kullanılabilir. Sunucularınızın anahtarlarını zaten bir yapılandırma
yönetim aracıyla dağıtıyorsanız bu dosyayı göstermek en güvenli yoldur.

## Eylemler

Eylemler izlenen sunucuları değiştiren tek özelliktir ve yapılandırılmadıkça
kapalıdır.

- Komutları bir yönetici yapılandırma dosyasına yazar. Arayüz argüman
  geçiremez ve şablon kullanılmaz.
- Her eylem hedef sunucularını, gruplarını veya etiketlerini açıkça
  listelemelidir.
- Arayüz, tam komutu ve sunucuyu bir onay penceresinde gösterir.
- Yetki gerektiren komutlar `sudo -n` kullanmalıdır. sudoers'ta tam olarak
  bu komutlara izin verin
  ([examples/sudoers](https://github.com/enderkus/helmsight/blob/main/examples/sudoers));
  asla kabuk, düzenleyici, joker karakter veya `ALL` izni vermeyin.
- Her çalıştırma denetim kaydına iki kez yazılır: başlamadan önce ve çıkış
  kodu ile kısaltılmış çıktısıyla birlikte bittiğinde.
- Denetim kaydı yalnızca eklenebilir. Veritabanı tetikleyicileri güncelleme
  ve silmeyi reddeder; her kayıt bir öncekine zincirlenmiş bir SHA-256
  özeti taşır. `helmsight audit verify` (veya arayüzdeki
  **Verify integrity**) kayıtlarla oynanıp oynanmadığını tespit eder.

## Merkez sunucu

- helmsight'ı ayrı bir sistem kullanıcısıyla,
  [examples/helmsight.service](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.service)
  dosyasındaki sıkılaştırılmış systemd birimiyle çalıştırın.
- Veri dizini 0700, veritabanı, anahtar dosyası ve kendinden imzalı TLS
  anahtarı 0600 izinleriyle oluşturulur. helmsight başka kullanıcıların
  okuyabildiği bir anahtar dosyasını kullanmayı reddeder.
- Yapılandırmadaki gizli değerler düz metin yerine başvuru olarak yazılabilir
  (`secret:<ad>`, `env:<DEĞİŞKEN>`, `file:<yol>`). Saklanan gizli değerler
  ve TOTP tohumları, ilk çalıştırmada oluşturulan `<data_dir>/secret.key`
  ile XChaCha20-Poly1305 kullanılarak şifrelenir. Anahtar dosyasını
  veritabanından ayrı yedekleyin.
- helmsight gizli değerleri, parolaları, oturum belirteçlerini veya veri
  toplama çıktılarını hiçbir zaman günlüğe yazmaz.

## Web uygulaması güvenliği

- Varsayılan olarak `127.0.0.1` adresinde dinler. Başka adreslerde TLS
  kendiliğinden açılır: verilen sertifikalarla veya SHA-256 parmak izi
  başlangıçta günlüğe yazılan kendinden imzalı bir sertifikayla. Herkese
  açık bir adreste düz HTTP kullanmak
  `server.tls.allow_insecure_http = true` gerektirir; bu, TLS'i sonlandıran
  bir ters vekil sunucunun arkasındaki kurulumlar içindir
  (`server.public_url` değerini vekil sunucunun `https://` adresine,
  `server.trusted_proxies` değerini de onun adresine ayarlayın).
- Parolalar Argon2id ile özetlenir (19 MiB, 2 yineleme); var olmayan
  kullanıcılar yanlış parolalarla aynı sürede yanıtlanır, böylece kullanıcı
  adları yanıt süresinden tahmin edilemez. En kısa parola 12 karakterdir.
- İsteğe bağlı TOTP (RFC 6238); kodlar ikinci kez kullanılamaz. Yöneticilerin
  TOTP kaydı zorunlu tutulabilir (`auth.require_totp_for_admins`).
- OpenID Connect, PKCE, state ve nonce ile yetkilendirme kodu akışını
  kullanır. Roller yapılandırılabilir bir alandan (claim) okunur; eşleşen
  rolü olmayan kullanıcılar, `default_role` ayarlanmadıkça reddedilir.
- Oturum çerezleri rastgele 256 bitlik belirteçlerdir; özetlenmiş olarak
  saklanır, `HttpOnly`, `SameSite=Strict`, TLS ile `Secure` ve `__Host-`
  önekiyle gönderilir. Oturumlar `auth.session_ttl` sonunda ve
  `auth.idle_timeout` kadar hareketsizlikten sonra sona erer.
- Durum değiştiren istekler oturuma özgü `X-CSRF-Token` başlığını gerektirir
  ve `Origin` başlığı başka bir siteyi gösterdiğinde reddedilir.
- Oturum açma denemeleri kullanıcı adı ve istemci adresi başına
  sınırlandırılır.
- Yanıtlar `Content-Security-Policy` (satır içi kod yok, çerçeveye alma yok),
  `X-Content-Type-Options`, `Referrer-Policy: no-referrer`,
  `Cross-Origin-Opener-Policy`, `Permissions-Policy` ve TLS kullanıldığında
  HSTS başlıklarını taşır. API yanıtları önbelleğe alınmaz.
- Duvar ekranı belirteçleri özetlenmiş olarak saklanır, yalnızca sabit bir
  grup kümesine okuma erişimi verir, adresin `#` kısmında taşınır (sunuculara
  ve günlüklere gönderilmez) ve her an iptal edilebilir.
- Prometheus uç noktası, bir taşıyıcı belirteç (bearer token)
  yapılandırılmadıkça kapalıdır.

## Kurulum öncesi güvenlik kontrol listesi

- [ ] helmsight ayrı bir sistem kullanıcısıyla, systemd birimiyle veya
      konteynerde çalışıyor.
- [ ] İzleme anahtarı yalnızca helmsight'a ait bir ed25519 anahtarı ve
      dosyası başkaları tarafından okunamıyor.
- [ ] `authorized_keys` satırında `restrict,from="…"` var.
- [ ] İzleme hesabı `docker` grubunda değil (ya da bunun root'a eşdeğer
      olduğu bilinerek eklendi).
- [ ] Sunucu anahtarları parmak izi karşılaştırılarak onaylandı;
      `accept_new_host_keys` yalnızca güvenilir bir ağda açık.
- [ ] Arayüz loopback'te ve bir TLS vekil sunucusunun arkasında ya da
      doğrudan TLS ile sunuluyor; `public_url` ayarlı.
- [ ] Yöneticiler için TOTP zorunlu (`auth.require_totp_for_admins = true`)
      veya giriş SSO ile yapılıyor.
- [ ] Gizli değerler `secret:`, `env:` veya `file:` başvurularıyla
      yazılmış; yapılandırma dosyasında düz metin parola yok.
- [ ] `secret.key` veritabanından ayrı yedekleniyor.
- [ ] Eylemler kullanılıyorsa sudoers yalnızca tam komutlara izin veriyor.

## Güvenlik açığı bildirmek

Güvenlik sorunlarını lütfen deponun Security sekmesindeki GitHub
**Report a vulnerability** düğmesiyle gizli olarak bildirin. Herkese açık
bir kayıt (issue) açmayın. Sürümü (`helmsight --version`), bir açıklamayı ve
sorunu yeniden üretme adımlarını ekleyin. Bildirimler Türkçe veya İngilizce
yapılabilir.

Bildirimleri üç iş günü içinde yanıtlamayı ve düzeltmeyi ve güvenlik
duyurusunu olabildiğince çabuk yayımlamayı hedefliyoruz. Bildirenler, aksini
tercih etmedikçe duyuruda anılır.
