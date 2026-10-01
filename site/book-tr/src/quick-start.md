# Hızlı başlangıç

Bu sayfa helmsight'ı birkaç dakikada çalışır hâle getirmenin iki yolunu
anlatır: tek bir makinede SSH olmadan denemek ve SSH ile gerçek sunucuları
izlemek.

## Tek bir makinede denemek

Bir Linux makinede yerel kip (local mode), `/proc` dosyalarını doğrudan
okuyarak makinenin kendisini izler. SSH'ye ve yapılandırma dosyasına gerek
yoktur:

```sh
./helmsight serve --local
```

helmsight tek kullanımlık bir kurulum bağlantısı yazdırır:

```text
  No users exist yet. Create the first administrator:

    http://127.0.0.1:8080/setup#token=…
```

Bağlantıyı açın, bir kullanıcı adı ve en az 12 karakterlik bir parola
seçin; ardından panele girersiniz. Veriler `~/.local/share/helmsight`
dizininde (root olarak çalıştırıldığında `/var/lib/helmsight`) saklanır.

Bağlantıdaki belirteç (token) adres çubuğunun `#` işaretinden sonraki
kısmındadır; bu kısım tarayıcıdan sunucuya gönderilmez ve günlüklere
yazılmaz. Henüz hiç kullanıcı yoksa helmsight her başlangıçta yeni bir
bağlantı üretir.

Yerel kip `/proc` okuduğu için yalnızca Linux'ta çalışır. Diğer
platformlarda aşağıda anlatıldığı gibi sunucuları SSH ile izleyin.

## Sunucuları SSH ile izlemek

Dört adım yeterlidir:

```sh
./helmsight init                    # yapılandırma dosyası, anahtar dosyası, ilk yönetici
$EDITOR helmsight.toml              # [[hosts]] kayıtlarını ekleyin
./helmsight hosts test --trust      # SSH'yi deneyin ve sunucu anahtarlarını onaylayın
./helmsight serve
```

1. **`init`** size birkaç soru sorar (dinleme adresi, veri dizini, SSH
   kullanıcısı, SSH özel anahtarının yolu, ilk yöneticinin adı ve
   parolası). Ardından yorum satırlarıyla açıklanmış bir `helmsight.toml`
   yazar, veri dizinini ve gizli değerleri şifreleyen `secret.key`
   dosyasını oluşturur. SSH anahtarını kendisi üretmez; nasıl
   üreteceğiniz bir sonraki sayfada anlatılıyor.
2. **`helmsight.toml`** dosyasına izlenecek sunucuları ekleyin:

   ```toml
   [[hosts]]
   name = "web-1"
   address = "10.0.0.11"
   groups = ["web"]
   ```

3. **`hosts test --trust`** her sunucuya bağlanır, ilk kez görülen sunucu
   anahtarlarının parmak izini gösterip onayınızı ister ve sonucu tek
   satırda yazar. Bu adımdan önce her sunucuda izleme hesabını oluşturmuş
   olmanız gerekir; ayrıntılar [Sunucuları izlemeye almak](fleet-setup.md)
   sayfasında.
4. **`serve`** web sunucusunu ve veri toplayıcıları başlatır.

Tarayıcıda `http://127.0.0.1:8080` adresini açın ve `init` sırasında
oluşturduğunuz yöneticiyle giriş yapın. Sunucular ilk veri toplanana kadar
birkaç saniye **Pending** (bekliyor) durumunda görünür.

## Arayüze başka makinelerden erişmek

helmsight varsayılan olarak yalnızca `127.0.0.1` adresinde dinler; yani
arayüze yalnızca aynı makineden erişilebilir. Başka makinelere açmanın iki
yolu var:

**1. Doğrudan dinlemek.** `[server]` bölümünde
`listen = "0.0.0.0:8443"` ayarlayın. TLS kendiliğinden açılır: ya
`[server.tls] mode = "files"` ile kendi sertifikanızı verin ya da
helmsight'ın kendinden imzalı bir sertifika üretmesine izin verin.
Üretilen sertifikanın parmak izi başlangıçta günlüğe yazılır; tarayıcının
uyarısını geçmeden önce bu parmak iziyle karşılaştırın.

```toml
[server]
listen = "0.0.0.0:8443"
public_url = "https://monitor.example.com:8443"

[server.tls]
mode = "files"
cert = "/etc/helmsight/tls/fullchain.pem"
key = "/etc/helmsight/tls/privkey.pem"
```

**2. Ters vekil sunucu (reverse proxy) arkasında.** helmsight loopback
adresinde kalır, TLS'i nginx, Caddy veya Traefik gibi bir vekil sunucu
sonlandırır. Bu durumda `public_url` ve `trusted_proxies` ayarlayın:

```toml
[server]
listen = "127.0.0.1:8080"
public_url = "https://monitor.example.com"
trusted_proxies = ["127.0.0.1"]
```

Örnek bir nginx yapılandırması (sunucu gönderimli olaylar için tamponlamayı
kapatmak önemlidir):

```nginx
server {
    listen 443 ssl;
    server_name monitor.example.com;
    # ssl_certificate ... ; ssl_certificate_key ... ;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

`public_url` OpenID Connect için zorunludur; ayrıca bildirimlerdeki
bağlantılarda ve başka sitelerden gelen istekleri reddeden köken (origin)
denetiminde kullanılır.

## Sonraki adımlar

- [Sunucuları izlemeye almak](fleet-setup.md): izleme hesabı, sunucu
  listesi ve anahtar onayı.
- [Alarmlar ve bildirimler](alerts.md): disk doluluğu gibi kurallar ve
  Slack/e-posta bildirimleri.
- [Güvenlik modeli](security.md): helmsight'ın sunucularda tam olarak ne
  çalıştırdığı.
