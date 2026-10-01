# helmsight

**Tüm sunucularınızı tek ekrandan izleyin. Sunuculara hiçbir şey kurmayın.**

helmsight, Linux sunucu filoları için ajansız (agentless) ve kendi
altyapınızda çalışan bir web paneli. Merkezdeki bir makinede çalışan tek bir
program, sunucularınıza sıradan SSH ile bağlanır; metrikleri ve envanteri
yalnızca okuma yapan komutlarla toplar, geçmişi gömülü bir SQLite
veritabanında saklar ve hızlı bir web arayüzü sunar. İzlenen sunuculara
hiçbir şey kurulmaz, kopyalanmaz ve yazılmaz.

> **Durum: erken geliştirme aşaması (0.x).** helmsight yeni bir proje.
> Debian, Ubuntu, Rocky, Fedora, openSUSE ve Alpine üzerinde test ediliyor;
> ancak üretim ortamında henüz az kullanıldı ve bağımsız bir güvenlik
> denetiminden geçmedi. Yapılandırma seçenekleri, API ve veritabanı biçimi
> 0.x sürümleri arasında değişebilir. Önce kritik olmayan sunucularda
> deneyin, veri dizininin yedeğini alın ve yaygın olarak kullanmadan önce
> [güvenlik modelini](security.md) ve
> [toplama betiğini](https://github.com/enderkus/helmsight/blob/main/crates/collect/src/remote.sh)
> inceleyin. Veri toplama yalnızca okuma yapar; isteğe bağlı Eylemler
> (Actions) özelliği ise sizin yapılandırdığınız komutları çalıştırır, bu
> yüzden dikkatle etkinleştirin. helmsight
> [MIT lisansı](https://github.com/enderkus/helmsight/blob/main/LICENSE)
> altında, olduğu gibi ve hiçbir garanti verilmeden sunulur. Kullanımdan
> doğan sorumluluk kullanıcıya aittir.

![Filo görünümü](images/fleet.png)

## Neler sunar

- **Filo görünümü**: her sunucu için durum, CPU, bellek, yük, en dolu disk,
  ağ trafiği, çalışma süresi ve alarmlar; gruplar, etiketler, arama ve
  küçük eğilim grafikleri (sparkline).
- **Sunucu ayrıntısı**: canlı ve geçmişe dönük grafikler, çekirdek başına
  CPU ısı haritası, süreçler, dinlenen portlar, servisler, konteynerler ve
  sistem bilgileri.
- **Geçmiş**: otomatik özetlemeyle (rollup) 90 güne kadar veri.
- **Sapma (drift) takibi**: dünden beri ne değişti; paketlerin, portların,
  servislerin, çekirdeğin ve işletim sisteminin sunucudan sunucuya ve
  referans sunucuya (baseline) göre karşılaştırılması.
- **Güvenlik görünümü**: başarısız SSH girişleri, bekleyen güvenlik
  güncellemeleri, beklenmeyen portlar, yeniden başlatma gerektiren
  sunucular ve TLS sertifikalarının bitiş tarihleri.
- **Alarmlar**: süre ve kapsam tanımlı kurallar, onaylama (acknowledge),
  susturma (silence), webhook, Slack veya e-posta ile bildirim.
- **Eylemler** (isteğe bağlı): önceden onaylanmış komutlar; onay penceresi
  ve yalnızca eklenebilen (append-only) bir denetim kaydı ile.
- **Kullanıcılar ve roller**, TOTP ile iki adımlı doğrulama, OpenID Connect
  ile tek oturum açma (SSO).
- **Duvar ekranı** (NOC ekranları için), OpenAPI belgeli JSON API,
  sunucu gönderimli olaylar (SSE) ve Prometheus uç noktası.

## Nasıl çalışır

1. helmsight her sunucuyla **kalıcı bir SSH oturumu** açar. Her birkaç
   saniyede bir bu oturum üzerinde `sh -s` çalıştırır ve kısa bir POSIX
   kabuk betiğini standart girdi üzerinden gönderir. Betik uzak sunucuda
   bir dosyaya yazılmaz ve süreç listesinde görünmez.
2. Betik yalnızca okuma yapar: `/proc` dosyaları, `df`, `ps`, `ss`,
   `systemctl`, paket veritabanı gibi kaynaklar. Bir araç eksikse ya da
   izin yoksa hata vermek yerine gerekçesiyle birlikte "n/a" (veri yok)
   döndürür.
3. Çıktı merkezde, boyutu sınırlandırılarak Rust kodu ile ayrıştırılır ve
   SQLite'a yazılır. Eski veriler dakikalık ve beş dakikalık özetlere
   dönüştürülür.
4. Web arayüzü programın içine gömülüdür. Tarayıcı yalnızca helmsight'ın
   sunduğu kodu çalıştırır; uzak sunuculardan gelen metin hiçbir zaman HTML
   olarak gösterilmez.

Ağır sayılabilecek kontroller (bekleyen güncellemeler, paket envanteri)
daha seyrek aralıklarla çalışır. Hangi verinin ne sıklıkla toplandığı
[güvenlik modeli](security.md#helmsightın-izlenen-sunucularda-çalıştırdıkları)
sayfasında komut komut listelenmiştir.

## Tasarım ilkeleri

1. **Ajansız.** İzlenen sunuculara hiçbir şey kurulmaz veya yazılmaz.
2. **Varsayılan olarak salt okunur.** Bir sunucuyu yalnızca isteğe bağlı
   Eylemler özelliği değiştirebilir ve yalnızca bir yöneticinin
   yapılandırma dosyasına yazdığı komutlarla.
3. **Rastgele komut çalıştırma yok.** Terminal yok, komut kutusu yok.
4. **Varsayılan olarak güvenli.** Sunucu anahtarları doğrulanır, sunucu
   yalnızca yerel adreste (loopback) dinler, gizli değerler şifreli
   saklanır.
5. **Uzak çıktıya güvenilmez.** Boyutu sınırlanır, Rust ile ayrıştırılır ve
   asla HTML olarak işlenmez.
6. **Düşük yük.** Sunucu başına tek kalıcı SSH oturumu ve birkaç saniyede
   bir kısa bir kabuk betiği.
7. **Taşınabilir.** Debian, Ubuntu, RHEL/Rocky/Alma, Fedora, openSUSE ve
   Alpine (BusyBox).
8. **Tek dosya.** Web arayüzü gömülüdür; kurulum tek bir dosyayı
   kopyalamaktan ibarettir.

## Bu belgeler hakkında

Belgeler Türkçe ve [İngilizce](https://enderkus.github.io/helmsight/docs/)
olarak sunulur; her sayfanın üst çubuğundaki bağlantı aynı sayfayı diğer
dilde açar. helmsight'ın arayüzü İngilizcedir. Menü ve düğme adları
arayüzde göründükleri gibi, kalın yazıyla ve gerektiğinde Türkçe
karşılıklarıyla verilir; örneğin **Host keys** (sunucu anahtarları).
Yapılandırma anahtarları, komutlar ve API yolları değişmeden, kod olarak
yazılır.

Devam etmek için [Kurulum](installation.md) sayfasına geçin veya doğrudan
[Hızlı başlangıç](quick-start.md) ile başlayın.
