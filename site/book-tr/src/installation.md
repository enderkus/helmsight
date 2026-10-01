# Kurulum

helmsight tek bir statik çalıştırılabilir dosyadır. Yalnızca merkezdeki
makinede çalışır; izlenen sunucularda bir SSH sunucusu ve POSIX uyumlu bir
kabuktan (`/bin/sh`) başka bir şey gerekmez.

## Gereksinimler

| Taraf | Gereksinim |
|---|---|
| Merkez makine | Linux, x86_64 veya aarch64. Statik derlendiği için glibc sürümü önemli değildir; Docker ile de çalıştırılabilir. |
| İzlenen sunucular | SSH sunucusu, `/bin/sh` ve izlemeye ayrılmış yetkisiz bir hesap. Ek paket gerekmez. |
| Ağ | Merkez makineden sunucuların SSH portuna erişim. Sunuculardan merkeze bağlantı gerekmez. |
| Tarayıcı | Güncel bir Firefox, Chrome, Edge veya Safari. |

Disk ihtiyacı sunucu sayısı ve saklama süreleriyle orantılıdır; saklama
süreleri [`[retention]`](configuration.md#retention) bölümünden ayarlanır.

## Kurulum betiği

systemd kullanan bir Linux sunucuda kurulum betiği her şeyi tek seferde
hazırlar:

```sh
curl -fsSLO https://enderkus.github.io/helmsight/install.sh
less install.sh            # root olarak çalıştırmadan önce okuyun
sudo sh install.sh
```

Betik birkaç soru sorar (web arayüzünün adresi, dış adres, izlenen
sunuculardaki hesap, bu sunucunun adresi, ilk yönetici ve parolası),
yapacaklarını listeler ve onay ister. Ardından:

1. bu makinenin mimarisine uygun sürüm arşivini indirir ve SHA-256
   sağlama toplamını `SHA256SUMS` ile doğrular; uyuşmazsa durur;
2. `/usr/local/bin/helmsight` dosyasını kurar ve `helmsight` sistem
   kullanıcısını oluşturur;
3. `helmsight init` çalıştırır; bu da `/etc/helmsight/helmsight.toml`
   dosyasını yazar, `/etc/helmsight/id_ed25519` SSH anahtarını, veri dizini
   `/var/lib/helmsight`'ı ve ilk yöneticiyi oluşturur;
4. sıkılaştırılmış systemd servisini kurar ve başlatır;
5. arayüzün adresini ve izlenen her sunucuyu hazırlayan komutu yazdırır
   (bkz. [Sunucuları izlemeye almak](fleet-setup.md)).

Güvenlik duvarına, TLS sertifikalarına veya ters vekil sunucuya dokunmaz;
mevcut bir yapılandırmanın, anahtarın veya veritabanının üzerine asla
yazmaz.

| Seçenek | Anlamı |
|---|---|
| `--version <etiket>` | Kurulacak sürüm, ör. `v0.2.0` (varsayılan: en son sürüm) |
| `--listen <adres>` | Web arayüzünün adresi ve portu (varsayılan `127.0.0.1:8080`) |
| `--public-url <url>` | Arayüzün dışarıdan erişilen `https://` adresi |
| `--ssh-user <kullanıcı>` | İzlenen sunuculardaki hesap (varsayılan `monitor`) |
| `--admin <ad>` | İlk yönetici (varsayılan `admin`) |
| `--from <adres>` | Bu sunucunun, izlenen sunucuların gördüğü adresi |
| `--archive <dosya>` / `--sums <dosya>` | İndirilmiş bir arşivden ve onun `SHA256SUMS` dosyasından kurar (çevrimdışı) |
| `--no-service` | systemd servisini kurmaz |
| `-y`, `--yes` | Soru sormaz; seçenekleri ve varsayılanları kullanır |
| `--dry-run` | Yalnızca yapılacakları gösterir |
| `--uninstall` | Programı ve servisi kaldırır; yapılandırmayı ve veriyi korur |

`--yes` ile parola sorulmaz: servis ilk yönetici için tek kullanımlık bir
kurulum bağlantısı yazdırır, betik de bunu sonda gösterir. Bu kullanım
cloud-init gibi otomasyonlarda da çalışır:

```sh
sh install.sh --yes --listen 0.0.0.0:8443 --public-url https://monitor.example.com --from 10.0.0.5
```

Betiği kurulu bir makinede yeniden çalıştırmak programı yükseltir, servisi
yeniden başlatır, yapılandırmayı ve veriyi korur. systemd olmayan
sistemlerde (ör. Alpine) programı ve yapılandırmayı kurar ve helmsight'ı
nasıl başlatacağınızı söyler.

## Hazır derlenmiş dosyalar

Her sürümde x86_64 ve aarch64 için statik Linux dosyaları ve bunların
SHA-256 sağlama toplamları yayımlanır:

```sh
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/helmsight-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/enderkus/helmsight/releases/latest/download/SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
tar xzf helmsight-x86_64-unknown-linux-musl.tar.gz
sudo install -m 0755 helmsight-x86_64-unknown-linux-musl/helmsight /usr/local/bin/
helmsight --version
```

ARM sunucularda (ör. AWS Graviton, Raspberry Pi 4/5 üzerinde 64 bit
Linux) `x86_64` yerine `aarch64` kullanın.

`sha256sum --check` satırı indirilen arşivin bozulmadığını ve sürüm
sayfasındaki dosyayla aynı olduğunu doğrular; `OK` çıktısını görmeden
devam etmeyin.

## Konteyner imajı

```sh
docker run -d --name helmsight -p 8443:8080 \
  -v helmsight:/var/lib/helmsight \
  -v /etc/helmsight:/etc/helmsight:ro \
  ghcr.io/enderkus/helmsight
```

Konteynerde `/etc/helmsight/helmsight.toml` dosyasında
`listen = "0.0.0.0:8080"` ve `data_dir = "/var/lib/helmsight"` ayarlayın.
Yerel olmayan bir adreste dinlediği için TLS kendiliğinden açılır; siz bir
sertifika vermezseniz kendinden imzalı (self-signed) bir sertifika üretilir
ve parmak izi başlangıçta günlüğe yazılır. Tarayıcıda
`https://<makine>:8443` adresini açın.

İmajda yalnızca program ve CA sertifikaları bulunur (kabuk bile yoktur). Bu
nedenle `--local` kipi konteynerde kullanılamaz ve komutları
`docker exec helmsight /helmsight <komut>` biçiminde çalıştırmanız gerekir,
örneğin:

```sh
docker exec -it helmsight /helmsight user add alice --role admin
docker exec -it helmsight /helmsight hosts test --trust
```

SSH anahtarını ve yapılandırmayı `/etc/helmsight` altına koyup salt okunur
bağlamak, veritabanını ise adlandırılmış bir birimde (`helmsight`) tutmak
önerilir. İmaj UID 65532 olan yetkisiz bir kullanıcıyla çalışır; bağlanan
dosyaların bu kullanıcı tarafından okunabilir olması gerekir.

## Kaynaktan derleme

Gereksinimler: Rust (stable), Node.js 22 ve npm.

```sh
git clone https://github.com/enderkus/helmsight.git
cd helmsight
(cd web && npm ci && npm run build)
cargo build --release
./target/release/helmsight --version
```

Web arayüzü derleme sırasında programın içine gömülür; bu yüzden önce
`web` dizininde arayüzü derlemeniz gerekir. Arayüz derlenmeden yapılan bir
`cargo build` uyarı verir ve yerine basit bir yer tutucu sayfa koyar.

Başka bir platformda (ör. macOS) statik bir Linux dosyası üretmek için
konteyner imajını derleyip dosyayı içinden kopyalayabilirsiniz:

```sh
docker build --platform linux/amd64 -t helmsight .
docker create --name hs helmsight
docker cp hs:/helmsight ./helmsight && docker rm hs
```

## Servis olarak çalıştırma

Depoda güvenlik açısından sıkılaştırılmış bir systemd birimi bulunur:
[`examples/helmsight.service`](https://github.com/enderkus/helmsight/blob/main/examples/helmsight.service).

```sh
# Ayrı bir sistem kullanıcısı
sudo useradd --system --home-dir /var/lib/helmsight --shell /usr/sbin/nologin helmsight

# Yapılandırma ve SSH anahtarı: root'a ait, helmsight grubu okuyabilir
sudo install -d -m 0750 -o root -g helmsight /etc/helmsight
sudo install -m 0640 -o root -g helmsight helmsight.toml /etc/helmsight/
sudo install -m 0640 -o root -g helmsight id_ed25519 /etc/helmsight/

sudo cp helmsight.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now helmsight
journalctl -u helmsight -f
```

Bu birim helmsight'ı yetkisiz bir kullanıcıyla, salt okunur bir dosya
sistemiyle, yalnızca kendisine ait bir durum dizini
(`/var/lib/helmsight`) ve kısıtlı bir sistem çağrısı kümesiyle çalıştırır.
Yapılandırmada `data_dir = "/var/lib/helmsight"` kullanın.

İlk kullanıcıyı oluşturmak için ya servis günlüğünde yazan kurulum
bağlantısını açın ya da şunu çalıştırın:

```sh
sudo -u helmsight helmsight --config /etc/helmsight/helmsight.toml user add alice --role admin
```

## Güncelleme

1. Yeni sürümün [değişiklik günlüğünü](changelog.md) okuyun; 0.x
   sürümlerinde uyumsuz değişiklikler olabilir.
2. Veri dizininin yedeğini alın (aşağıya bakın).
3. Yeni dosyayı eskisinin üzerine kurun ve servisi yeniden başlatın:
   `sudo systemctl restart helmsight`. Veritabanı şeması gerekiyorsa
   başlangıçta kendiliğinden güncellenir.

Konteyner kullanıyorsanız yeni imajı çekip konteyneri aynı birimlerle
yeniden oluşturmanız yeterlidir.

## Yedekleme

Veri dizininde şunlar bulunur:

| Dosya | İçerik |
|---|---|
| `helmsight.db` | Metrikler, envanter, kullanıcılar, alarmlar, denetim kaydı |
| `secret.key` | Saklanan gizli değerleri ve TOTP tohumlarını şifreleyen anahtar |
| `known_hosts` | Onaylanmış SSH sunucu anahtarları |
| `tls/` | Kendinden imzalı sertifika (üretildiyse) |

Tutarlı bir veritabanı yedeği için servisi kısa süreliğine durdurup
dosyaları kopyalayın ya da çalışırken SQLite'ın yedekleme komutunu
kullanın:

```sh
sqlite3 /var/lib/helmsight/helmsight.db ".backup '/yedek/helmsight.db'"
```

`secret.key` dosyasını veritabanından **ayrı** bir yerde saklayın: bu
anahtar olmadan saklanan gizli değerler çözülemez, ikisi birlikte ele
geçirilirse de gizli değerler açığa çıkar.

## Kaldırma

helmsight izlenen sunuculara hiçbir şey kurmadığı için kaldırma yalnızca
merkez makinede yapılır: servisi durdurun, programı, `/etc/helmsight`
dizinini ve veri dizinini silin. Sunuculardaki izleme hesabını ve
`authorized_keys` satırını da kaldırabilirsiniz.
