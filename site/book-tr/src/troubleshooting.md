# Sorun giderme

İşe `helmsight config check` ve `helmsight hosts test` ile başlayın. İlki
yapılandırmayı doğrular; ikincisi her sunucuya bağlanır ve her sunucu için,
arayüzdeki durumlarla aynı olan tek satırlık bir sonuç yazar.

## Sunucu "Down" görünüyor

helmsight, `ssh.connect_timeout` süresi içinde bir TCP bağlantısı açamadı
veya SSH el sıkışmasını tamamlayamadı.

- helmsight sunucusundan `ssh -p <port> monitor@<adres> true` komutunu
  deneyin.
- İki makine arasındaki güvenlik duvarlarını ve sunucunun `address` ile
  `port` değerlerini denetleyin.
- Ulaşılamayan sunucular 5 dakikaya kadar artan aralıklarla yeniden
  denenir; düzelen bir sunucunun geri gelmesi birkaç dakika sürebilir.

## Sunucu "Auth failed" görünüyor

SSH sunucusu helmsight'ın sunduğu bütün anahtarları reddetti.

- Açık anahtar sunucuda `~monitor/.ssh/authorized_keys` dosyasında mı?
  Dosyanın izinleri 600 ve sahibi hesabın kendisi mi?
- `from="…"` seçeneği, sunucunun gördüğü adresle eşleşiyor mu? NAT
  arkasında bu, çevrilmiş adrestir.
- Hesap kilitli mi veya kabuğu `/usr/sbin/nologin` mi? helmsight çalışan
  bir POSIX kabuğuna (`/bin/sh`) ihtiyaç duyar. Alpine'de `adduser -D` ile
  oluşturulan hesaplar kilitlidir; bkz.
  [Sunucuları izlemeye almak](fleet-setup.md#2-her-sunucuda-izleme-hesabını-oluşturun).
- helmsight süreci anahtar dosyasını okuyabiliyor mu? systemd birimiyle
  çalışırken dosyanın `helmsight` grubu tarafından okunabilir olması
  gerekir.
- Sunucudaki SSH günlüğü genellikle nedeni açıkça yazar:
  `journalctl -u ssh -u sshd --since "10 min ago"` veya
  `/var/log/auth.log`, `/var/log/secure`.
- Kimlik doğrulama hatalarından sonra helmsight yeniden denemeden önce 1–5
  dakika bekler; böylece fail2ban gibi araçları tetiklemez.

## "Key not trusted" veya "Key changed"

- **Key not trusted**: sunucu yeni. `helmsight hosts test --trust`
  çalıştırın veya parmak izini sunucuda
  `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` ile karşılaştırdıktan
  sonra arayüzde **Host keys** sayfasından onaylayın.
- **Key changed**: veri toplama bilerek durduruldu. Sunucu yeniden
  kurulduysa veya anahtarları yenilendiyse yeni parmak izini sunucuda
  doğrulayın ve **Host keys** sayfasından onaylayın. Değişikliği
  açıklayamıyorsanız olası bir ortadaki adam (man-in-the-middle) saldırısı
  olarak ele alın.

## Bir görünümde "n/a" yazıyor

"n/a" her zaman sunucunun bildirdiği nedenle birlikte gösterilir. Sık
görülenler:

| Neden | Çözüm |
|---|---|
| system journal not readable | Hesabı `systemd-journal` grubuna (Debian/Ubuntu'da `adm` de olur) ekleyin |
| docker: permission denied | Hesabı `docker` grubuna ekleyin; yalnızca root'a eşdeğer erişimi kabul ediyorsanız |
| no container runtime found | Docker veya Podman kurulu değil; yapılacak bir şey yok |
| systemctl: Failed to connect to bus | Sunucuda D-Bus çalışmıyor (konteynerlerde sık görülür) |

Dinlenen portların sahibi olan süreçler yalnızca izleme hesabının kendi
süreçleri için görünür; bu, yetkisiz kullanıcılar için çekirdeğin koyduğu
bir kısıttır.

## Rocky, Alma, RHEL veya Fedora'da bekleyen güncelleme görünmüyor

`dnf` ve `yum` salt okunur sorgularda bile günlük dosyası yazdığı için
helmsight bunları siz `collect.dnf_updates = true` ayarlamadıkça
çalıştırmaz. Güncellemeler 6 saatte bir denetlenir
(`collect.updates_interval`).

## Kurulum bağlantısını kaybettim

Hiç kullanıcı olmadığı sürece `helmsight serve` her başlangıçta yeni bir
kurulum bağlantısı yazdırır. Ya da yöneticiyi komut satırından oluşturun:

```sh
helmsight user add alice --role admin
```

## Parolamı unuttum veya TOTP cihazımı kaybettim

```sh
helmsight user reset-password alice --reset-totp
```

Bu komut yeni bir parola belirler, TOTP kaydını kaldırır ve kullanıcının
oturumlarını sonlandırır.

## Girişte "Cross-origin request refused" hatası

Tarayıcının gönderdiği `Origin`, helmsight'ın gördüğü adresle eşleşmiyor.
Ters vekil sunucu arkasındaysanız `server.public_url` değerini tam dış
adres olarak ayarlayın (ör. `https://monitor.example.com`) ve vekil
sunucunun `Host` başlığını korumasını sağlayın.

## Tek oturum açma (SSO) çalışmıyor

- `server.public_url` ayarlanmış olmalı ve
  `<public_url>/api/v1/auth/oidc/callback` kimlik sağlayıcıda yönlendirme
  adresi (redirect URI) olarak kayıtlı olmalıdır.
- `role_map` içinde eşleşen bir değeri olmayan kullanıcılar, `default_role`
  ayarlanmadıkça reddedilir. Kimlik sağlayıcının belirtecinde
  `role_claim` ile belirtilen alanı denetleyin.

## Port kullanımda

Başlangıçta adresin kullanımda olduğuna dair bir hata alırsanız aynı portu
başka bir süreç (veya çalışmaya devam eden eski bir helmsight) kullanıyordur:

```sh
sudo ss -ltnp 'sport = :8080'
```

Ya o süreci durdurun ya da `--listen` veya `server.listen` ile başka bir
port seçin.

## Daha fazla ayrıntı almak

Sunucuyu daha ayrıntılı günlükle çalıştırın:

```sh
HELMSIGHT_LOG=debug helmsight serve
```

Günlüklerde gizli değerler ve toplanan çıktı hiçbir zaman bulunmaz. Bir
sorun bildirirken `helmsight --version` çıktısını, etkilenen sunucunun
dağıtımını ve `helmsight hosts test <ad>` çıktısını ekleyin.
