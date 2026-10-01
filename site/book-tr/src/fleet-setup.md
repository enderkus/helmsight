# Sunucuları izlemeye almak

Bu sayfa bir sunucu filosunu baştan sona izlemeye almayı adım adım anlatır.
Örneklerde helmsight'ın çalıştığı makinenin adresi `10.0.0.5`, izleme
hesabının adı `monitor`'dür.

## 1. helmsight için bir anahtar oluşturun

[Kurulum betiğini](installation.md#kurulum-betiği) veya
`helmsight init --generate-key` komutunu kullandıysanız anahtar zaten
vardır (`/etc/helmsight/id_ed25519`); 2. adımla devam edin.

helmsight'ın çalıştığı makinede (komutlar [Kurulum](installation.md#servis-olarak-çalıştırma)
sayfasındaki `helmsight` sistem kullanıcısının oluşturulduğunu varsayar):

```sh
sudo install -d -m 0750 -o root -g helmsight /etc/helmsight
sudo ssh-keygen -t ed25519 -f /etc/helmsight/id_ed25519 -N "" -C helmsight
sudo chgrp helmsight /etc/helmsight/id_ed25519
sudo chmod 0640 /etc/helmsight/id_ed25519
```

Yalnızca helmsight'a ayrılmış bir ed25519 anahtarı kullanın. RSA anahtarları
da çalışır; ancak kullanılan RSA kütüphanesinde bilinen bir zamanlama yan
kanalı bulunduğu için ed25519 tercih edilmelidir (ayrıntılar
[güvenlik modelinde](security.md#izleme-hesabını-oluşturmak)). Parolalı
anahtarlar ssh-agent'a yüklenmelidir (`ssh.use_agent = true`).

## 2. Her sunucuda izleme hesabını oluşturun

### `helmsight hosts bootstrap` ile

helmsight, aşağıdaki adımların hepsini bir sunucuda yapan bir betik
yazdırır: hesabı oluşturur, açık anahtarı `restrict,from=` ile kurar,
Alpine'de anahtarla girişin önündeki kilidi açar ve hesabı günlük grubuna
ekler. Yeniden çalıştırmak güvenlidir.

```sh
helmsight hosts bootstrap --from 10.0.0.5 | ssh root@web-1 sh
```

`--from`, helmsight sunucusunun izlenen sunucunun gördüğü adresidir (birden
çok adres veya ağ virgülle ayrılabilir, ör. `10.0.0.5,10.1.0.0/16`).
Verilmezse anahtar her adresten kabul edilir ve bir uyarı yazdırılır.
Betiği çalıştırmadan önce incelemek için bir dosyaya kaydedin:

```sh
helmsight hosts bootstrap --from 10.0.0.5 > monitor-account.sh
```

| Seçenek | Anlamı |
|---|---|
| `--from <adres>` | Anahtarın kabul edildiği adresler |
| `--user <kullanıcı>` | Oluşturulacak hesap (varsayılan: `ssh.user`) |
| `--key <yol>` | Açık anahtarı kurulacak özel anahtar (varsayılan: `ssh.identity_files` içindeki ilk anahtar) |
| `--no-journal` | Hesabı `systemd-journal` veya `adm` grubuna eklemez |

### Elle

İzlenecek her sunucuda root olarak:

```sh
useradd --create-home --shell /bin/sh monitor
install -d -m 700 -o monitor -g monitor ~monitor/.ssh
echo 'restrict,from="10.0.0.5" ssh-ed25519 AAAA… helmsight' > ~monitor/.ssh/authorized_keys
chown monitor:monitor ~monitor/.ssh/authorized_keys
chmod 600 ~monitor/.ssh/authorized_keys
```

- `ssh-ed25519 AAAA… helmsight` yerine `/etc/helmsight/id_ed25519.pub`
  dosyasının içeriğini yazın.
- `10.0.0.5` yerine helmsight sunucusunun, izlenen sunucunun gördüğü
  adresini yazın. NAT arkasındaysanız bu, çevrilmiş adrestir. `from=`
  sayesinde anahtar çalınsa bile başka bir makineden kullanılamaz.
- `restrict` port, ajan ve X11 yönlendirmesini ve terminal (PTY) ayırmayı
  kapatır. helmsight bunların hiçbirine ihtiyaç duymaz.
- Hesabın kabuğu çalışan bir POSIX kabuğu olmalıdır (`/bin/sh`);
  `/usr/sbin/nologin` ile veri toplanamaz.

Alpine'de `useradd` yerine `adduser -D -s /bin/sh monitor` kullanın. Alpine
yeni hesapları kilitli (`!`) oluşturur ve OpenSSH kilitli hesaplara
anahtarla girişi de reddeder. Hesabı parolayla giriş yapılamayacak ama
kilitli de sayılmayacak hâle getirin:

```sh
sed -i 's/^monitor:!/monitor:*/' /etc/shadow
```

`passwd -u` kullanmayın: BusyBox'ta boş parolalı bir hesap bırakabilir.

Çok sayıda sunucu için bu adımları Ansible, Salt veya bulut-init
(cloud-init) gibi mevcut araçlarınızla dağıtabilirsiniz. Örnek bir Ansible
görevi:

```yaml
- name: helmsight izleme hesabı
  ansible.builtin.user:
    name: monitor
    shell: /bin/sh
    create_home: true

- name: helmsight anahtarı
  ansible.posix.authorized_key:
    user: monitor
    key: "{{ lookup('file', 'files/helmsight.pub') }}"
    key_options: 'restrict,from="10.0.0.5"'
    exclusive: true
```

### İsteğe bağlı grup üyelikleri

Bazı veriler ek izin gerektirir. Bu izinler olmadan helmsight ilgili
görünümde "n/a" ve nedenini gösterir; başka hiçbir şey etkilenmez.

| Grup | Sağladığı | Not |
|---|---|---|
| `systemd-journal` (Debian/Ubuntu'da `adm` de olur) | Başarısız SSH giriş geçmişi | Günlüklere salt okunur erişim |
| `docker` | Konteyner listesi ve kaynak kullanımı | **root'a eşdeğerdir**; ancak bunu kabul ediyorsanız verin |

```sh
usermod -aG systemd-journal monitor
```

Sunucuya bunun dışında hiçbir şey kurulmaz.

## 3. Sunucularınızı tanımlayın

`helmsight.toml` içinde:

```toml
[ssh]
user = "monitor"
identity_files = ["/etc/helmsight/id_ed25519"]

[[hosts]]
name = "web-1"
address = "10.0.0.11"
groups = ["web"]
tags = ["env:prod", "role:web"]

[[hosts]]
name = "web-2"
address = "10.0.0.12"
groups = ["web"]
tags = ["env:prod", "role:web"]
baseline = "web-1"    # sapma ve yeni portlar web-1'e göre karşılaştırılır

[[hosts]]
name = "db-1"
address = "db-1.internal"
port = 2222
user = "observer"     # bu sunucu için farklı hesap
groups = ["db"]
```

**Gruplar ve etiketler nasıl seçilir?** Gruplar filoyu kabaca böler
(`web`, `db`, `edge`) ve filtrelerde, alarm kurallarında, eylemlerde,
bildirim kanallarında ve duvar ekranı belirteçlerinde kapsam olarak
kullanılır. Etiketler serbest biçimlidir ve genellikle `anahtar:değer`
şeklinde yazılır (`env:prod`, `dc:ist-1`, `team:payments`). Bir sunucu
birden çok gruba ve etikete sahip olabilir.

**Referans sunucu (baseline)** aynı işi yapan sunucular arasındaki farkları
yakalamak içindir. `web-2` için `baseline = "web-1"` yazdığınızda
**Compare** (karşılaştır) görünümü iki sunucu arasındaki paket, port,
servis, çekirdek ve işletim sistemi farklarını; güvenlik sayfası da web-2'de
dinlenen ama web-1'de bulunmayan portları gösterir.

### Sunucu listesini ayrı tutmak

Sunucular ayrı bir dosyada da tutulabilir (yapılandırma dosyasına göre
göreli yol):

```toml
hosts_file = "hosts.toml"
```

`hosts.toml` yalnızca `[[hosts]]` kayıtlarından oluşur. Bu, sunucu listesini
bir envanter aracından üretmeyi kolaylaştırır.

### `~/.ssh/config` dosyasından içe aktarmak

```toml
[ssh_config_import]
path = "~/.ssh/config"
hosts = ["web-*", "db-*"]
tags = ["source:ssh-config"]
```

`HostName`, `User`, `Port` ve `IdentityFile` dikkate alınır. Aynı adla
açıkça yazılmış bir `[[hosts]]` kaydı önceliklidir. `ProxyJump` kullanan
sunucular uyarıyla atlanır.

Yapılandırmayı `helmsight config check` ile doğrulayın. Her hata dosyayı,
satırı ve anahtarı belirtir:

```text
error: helmsight.toml:23:1: `hosts[1].name`: duplicate host name `web-1`
```

## 4. Sunucu anahtarlarını onaylayın

helmsight tanımadığı sunucu anahtarlarını reddeder. Bunları bir kez
onaylayın:

```sh
helmsight hosts test --trust
```

```text
HOST        STATUS             DETAIL
web-1       host key unknown   ssh-ed25519 SHA256:ldBw6qzC… ([10.0.0.11]:22)
  Trust this key? Compare with `ssh-keygen -lf` on the host (y/N) [N]: y
  trusted; retrying
web-1       ok                 Debian GNU/Linux 12 (bookworm); user monitor (72 ms)
```

Onaylamadan önce parmak izini sunucunun kendisinde karşılaştırın:

```sh
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

Anahtarlar arayüzde **Host keys** (sunucu anahtarları) sayfasından da
onaylanabilir; burada yönetici, sunucuda yazdırılan parmak izini yapıştırır
ve onay ancak sunulan anahtarla eşleşirse kabul edilir. Daha sonra
değişen bir anahtar veri toplamayı durdurur ve bir yönetici inceleyene kadar
kritik bir alarm üretir.

Sunucuların çok olduğu ve ağın güvenilir olduğu ortamlarda
`ssh.accept_new_host_keys = true` ile ilk görülen anahtarlar otomatik
onaylanabilir (OpenSSH'deki `accept-new` gibi). Değişen anahtarlar bu
ayarda da her zaman reddedilir.

## 5. Sunucuyu başlatın

```sh
helmsight serve
```

Sunucular ilk veri toplanana kadar, birkaç saniye boyunca **Pending**
(bekliyor) olarak görünür. Bir sunucu **Down**, **Auth failed** veya
**Key not trusted** durumunda kalırsa
[Sorun giderme](troubleshooting.md) sayfasına bakın.

## helmsight neleri okur

Her turda mevcut SSH oturumu üzerinden tek bir POSIX kabuk betiği çalışır:
`/proc` dosyaları, `df`, `ps`, `ss`, `systemctl`, paket veritabanı ve benzeri
salt okunur kaynaklar. Tam liste ve verilen güvenceler
[güvenlik modelinde](security.md#helmsightın-izlenen-sunucularda-çalıştırdıkları)
yer alır.
