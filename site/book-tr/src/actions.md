# Eylemler (Actions)

Eylemler, operatörlerin kabuk erişimi olmadan küçük ve önceden onaylanmış
bir dizi işi yapmasını sağlar; örneğin nginx'i yeniden başlatmak. Siz
yapılandırana kadar kapalıdır; hiç eylem tanımlanmamışsa eylemlerle ilgili
arayüz tamamen gizlenir.

Eylemler helmsight'ın izlenen sunucuları **değiştirebilen tek** özelliğidir.
Bu yüzden etkinleştirmeden önce bu sayfanın tamamını okuyun.

```toml
[[actions]]
id = "restart-nginx"
label = "Restart nginx"
description = "Web sunucusunu yeniden başlatır. Yaklaşık iki saniye sürer."
command = "sudo -n /usr/bin/systemctl restart nginx.service"
groups = ["web"]       # zorunlu: hosts, groups ve/veya tags
role = "operator"      # operator veya admin
timeout = "60s"
```

- Komut sabittir. Arayüz yalnızca bir eylem ve bir sunucu seçebilir;
  argüman geçiremez, komutta şablon veya değişken yoktur.
- Her eylem hedef sunucularını, gruplarını veya etiketlerini açıkça
  belirtmelidir; "tüm sunucular" varsayılanı yoktur.
- Çalıştırmadan önce bir pencere tam komutu ve sunucuyu gösterir ve onay
  ister.
- Çıktı (en fazla 64 KiB) arayüzde gösterilir; ilk 4 KiB'ı denetim
  kaydına da yazılır.
- Yetki gerektiren komutlar `sudo -n` ile başlamalıdır ve her sunucuda
  yalnızca o komuta izin veren bir sudoers kuralı bulunmalıdır:

```text
Cmnd_Alias HELMSIGHT_ACTIONS = /usr/bin/systemctl restart nginx.service
monitor ALL=(root) NOPASSWD: HELMSIGHT_ACTIONS
```

Bu kuralı `visudo -f /etc/sudoers.d/helmsight` ile ekleyin; `visudo`
sözdizimi hatalı bir dosyanın kaydedilmesini engeller. Örnek dosya:
[`examples/sudoers`](https://github.com/enderkus/helmsight/blob/main/examples/sudoers).

`-n` seçeneği sudo'nun parola sormasını engeller: kural yoksa komut beklemek
yerine hemen hata verir.

## Güvenli eylem tasarımı

- sudoers'ta **asla** kabuk (`/bin/sh`, `bash`), düzenleyici (`vi`,
  `nano`), sayfalayıcı (`less`), joker karakter (`*`) veya `ALL` izni
  vermeyin. Bunların her biri tam root erişimi anlamına gelir.
- `systemctl restart nginx.service` gibi tam yollu, argümanları sabit
  komutlar yazın. `systemctl restart *` gibi kurallar her servisi
  yeniden başlatmaya izin verir.
- Yeniden başlatma, önbellek temizleme veya bir durum sorgusu gibi geri
  alınabilir ve etkisi sınırlı işleri eyleme dönüştürün. Veri silen veya
  yapılandırma değiştiren işler için mevcut değişiklik yönetimi
  süreçlerinizi kullanın.
- Riskli eylemler için `role = "admin"` kullanın.
- İzleme hesabının sudo yetkisi yalnızca eylem tanımlanan sunucularda
  olmalıdır. Salt izleme yapılan sunucularda sudoers kuralı gerekmez.

Örnek, yetki gerektirmeyen bir eylem:

```toml
[[actions]]
id = "disk-usage"
label = "Show disk usage"
description = "Dosya sistemlerinin doluluğunu gösterir."
command = "df -h"
groups = ["web", "db"]
```

## Denetim kaydı (Audit log)

Her çalıştırma, başladığında ve bittiğinde denetim kaydına yazılır:
kullanıcı, sunucu, komut, çıkış kodu ve çıktının ilk 4 KiB'ı. Yöneticiler
kaydı **Audit log** sayfasında okur. Kullanıcı, sunucu anahtarı,
susturma, duvar ekranı belirteci ve gizli değer değişiklikleri de kaydedilir.

Kayıt yalnızca eklenebilir (append-only): veritabanı güncelleme ve silme
işlemlerini reddeder ve her kayıt bir öncekine zincirlenen bir SHA-256
özeti taşır. Arayüzdeki **Verify integrity** (bütünlüğü doğrula) düğmesi
veya `helmsight audit verify` komutu, kayıtlarla oynanıp oynanmadığını
tespit eder.

Bu zincir, veritabanı dosyasına doğrudan erişen birinin geçmiş bir kaydı
sessizce değiştirmesini veya silmesini fark edilir kılar; ancak merkez
sunucuda root yetkisi olan birinin kaydı tamamen yeniden yazmasını
engelleyemez. Denetim gereksinimleriniz sıkıysa kaydı düzenli olarak
dışarıya (ör. API üzerinden) aktarın.
