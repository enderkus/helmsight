# Web arayüzü

Arayüz işletim sisteminin açık veya koyu tema tercihine uyar; üst çubuktan
değiştirebilirsiniz. Her görünüm verisinin en son ne zaman güncellendiğini
gösterir; bayatlamış veri soluklaştırılır ve etiketlenir. Arayüz canlıdır:
sunucu durumları ve alarmlar sayfayı yenilemeden, sunucu gönderimli olaylarla
(SSE) güncellenir.

Kullanıcının rolüne göre görünenler değişir:

| Rol | Yapabildikleri |
|---|---|
| `viewer` (izleyici) | Tüm izleme verilerini görür |
| `operator` (operatör) | Ek olarak alarmları onaylar, susturur ve izin verilen eylemleri çalıştırır |
| `admin` (yönetici) | Ek olarak kullanıcıları, sunucu anahtarlarını, duvar ekranı belirteçlerini ve denetim kaydını yönetir |

## Filo (Fleet)

![Filo görünümü](images/fleet.png)

Filo görünümü her sunucuyu durumu, CPU ve bellek eğilim grafikleri, yükü,
en dolu dosya sistemi, ağ trafiği, çalışma süresi ve etkin alarmlarıyla
listeler. Tablo veya kart ızgarası olarak kullanılabilir.

- **Durum çipleri** duruma göre süzer. *Needs attention* (ilgi bekleyenler)
  OK olmayan, kısmen toplanan veya bayatlamış her şeyi bir arada gösterir.
- **Arama** (`/` tuşu) ad, adres, işletim sistemi, grup ve etiketlerde
  eşleşme arar; `group:web` ve `tag:env:prod` biçimleri birebir eşleşir.
- Durumlar farklı sorunları birbirinden ayırır:

| Durum | Anlamı |
|---|---|
| OK / Warning / Critical | Veri toplanıyor; etkin alarmların en ağırı |
| Down | Sunucu SSH üzerinden yanıt vermedi |
| Auth failed | SSH sunucusu yapılandırılan anahtarları reddetti |
| Key not trusted | Sunucu anahtarının bir yönetici tarafından onaylanması gerekiyor |
| Key changed | Sunucu anahtarı onaylanan anahtardan farklı; veri toplama durduruldu |
| Pending | İlk veri toplanması bekleniyor |
| *partial* etiketi | Bazı veriler toplanamadı (nedeniyle birlikte gösterilir) |

Bu ayrım bilinçlidir: "erişilemiyor" bir ağ veya donanım sorunu,
"kimlik doğrulama başarısız" bir hesap veya anahtar sorunu, "anahtar
değişti" ise olası bir güvenlik olayıdır. Her biri farklı bir ekibin farklı
bir müdahalesini gerektirir. Çözüm önerileri için
[Sorun giderme](troubleshooting.md) sayfasına bakın.

## Sunucu ayrıntısı

![Sunucu ayrıntısı](images/host.png)

Filo listesinde bir sunucuya tıklayınca açılır. Sekmeler:

- **Overview** (genel bakış): önce özet değerler, ardından CPU (user,
  system, iowait, steal), çekirdek başına ısı haritası, bellek ve swap,
  yük, bağlama noktası başına disk doluluğu, aygıt başına disk G/Ç, arayüz
  başına ağ trafiği ve TCP bağlantıları grafikleri. 15 dakikadan 30 güne
  kadar bir aralık ya da özel bir aralık seçin; imleç tüm grafiklerde eş
  zamanlı hareket eder. Her grafiğin bir tablo görünümü de vardır.
- **Processes** (süreçler): CPU ve bellek kullanımına göre en üstteki
  süreçler.
- **Network** (ağ): dinlenen portlar ve bunlara sahip süreçler, arayüz
  hızları ve TCP durumları.
- **Services** (servisler): önce başarısız birimler, ardından çalışan
  servisler.
- **Containers** (konteynerler): Docker veya Podman konteynerleri, durumları
  ve kaynak kullanımları.
- **System** (sistem): işletim sistemi, çekirdek, donanım, oturum açmış
  kullanıcılar ve veri toplama ayrıntıları.
- **Changes** (değişiklikler): paketlerde, portlarda, birimlerde, çekirdekte
  ve işletim sisteminde neyin değiştiği.
- **Security** (güvenlik): başarısız SSH girişleri, bekleyen güncellemeler
  ve referans sunucuda bulunmayan portlar.

### Grafikleri okumak

- **CPU** grafiğinde *iowait* diskin, *steal* ise sanallaştırma
  katmanının (hipervizör başka misafirlere CPU verdiği için bekleme)
  yarattığı gecikmeyi gösterir. Yüksek *steal* genellikle sanal makinenin
  barındığı fiziksel sunucunun aşırı yüklü olduğuna işaret eder.
- **Bellek** grafiğinde "kullanılan", geri kazanılabilir önbellek hariç
  tutularak hesaplanır; Linux'un boş belleği önbellek için kullanması
  "bellek dolu" olarak görünmez.
- **Yük** (load average) çekirdek sayısıyla birlikte okunmalıdır;
  `load1_per_core` metriği bunu alarm kuralları için hazır verir.
- Sunucuya ulaşılamayan dönemler grafikte boşluk olarak görünür; araya
  çizgi çekilmez.
- Uzun aralıklarda grafikler, aralığı kapsayan en ince çözünürlükten
  (ham veri, dakikalık veya beş dakikalık ortalamalar) çizilir. Bu nedenle
  30 günlük bir grafikte birkaç saniyelik ani yükselmeler yumuşar; kısa
  olayları incelemek için daha dar bir aralık seçin.

## Karşılaştırma (Compare)

![Karşılaştırma](images/compare.png)

Bir sunucuyu başka bir sunucuyla veya yapılandırılmış referans sunucusuyla
karşılaştırın. Yalnızca farklar gösterilir: paketler (ve sürümleri),
dinlenen portlar, etkin birimler, çekirdek ve işletim sistemi sürümü. Aynı
rolü üstlenen sunucuların zamanla birbirinden uzaklaşmasını (configuration
drift) yakalamanın en hızlı yolu budur.

## Güvenlik (Security)

![Güvenlik görünümü](images/security.png)

Filo genelindeki güvenlik sayfası şunları bir araya getirir: tüm
sunucuların başarısız SSH girişlerinin zaman içindeki grafiği ve sunucu
başına son 24 saatteki sayısı, bekleyen ve güvenlik güncellemeleri, yeniden başlatma gerektiren sunucular, her
sunucunun referans sunucusunda bulunmayan portlar ve helmsight sunucusundan
denetlenen TLS sertifikaları.

## Değişiklikler (Changes)

Filo genelinde envanter değişikliklerinin zaman çizelgesi; türe ve sunucuya
göre süzülebilir. "Dün gece ne değişti?" sorusunun cevabı buradadır: kurulan
veya güncellenen paketler, açılan veya kapanan portlar, etkinleştirilen
servisler, yeni çekirdek.

## Yönetim sayfaları

Yöneticiler ek olarak şu sayfaları görür:

- **Users**: yerel kullanıcılar, roller ve TOTP durumu.
- **Host keys**: onay bekleyen ve değişmiş sunucu anahtarları.
- **Wall display**: duvar ekranı belirteçleri.
- **Audit log**: eylemlerin ve yönetim işlemlerinin değiştirilemez kaydı.

Her kullanıcı kendi parolasını ve TOTP kaydını sağ üstteki kullanıcı
menüsündeki **Account** (hesap) sayfasından yönetir. Eylemler
yapılandırılmışsa operatörler ve yöneticiler menüde **Actions** sayfasını
da görür.

## Klavye kısayolları

| Tuşlar | İşlev |
|---|---|
| `/` | Aramaya odaklan |
| `g` ardından `f` | Filo |
| `g` ardından `s` | Güvenlik |
| `g` ardından `a` | Alarmlar |
| `g` ardından `c` | Değişiklikler |
| `?` | Kısayolları göster |
| `Esc` | Pencereyi kapat veya aramayı temizle |
