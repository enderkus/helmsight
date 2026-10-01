# Duvar ekranı

![Duvar ekranı](images/display.png)

Duvar ekranı, TV ve NOC ekranları için tam ekran, salt okunur bir
görünümdür: sunucu gruplarına göre düzenlenmiş büyük durum kutucukları,
etkin alarmlar ve bir saat; gezinme menüsü yoktur. Filo ekrana sığmadığında
gruplar arasında 20 saniyede bir döner. Durum yalnızca renkle değil, metin
ve simgeyle de gösterilir; uzaktan ve renk körlüğü olan kişiler tarafından
okunabilir.

Oturum açmışken `/display` adresini açın ya da ekranın bir hesaba ihtiyaç
duymaması için bir duvar ekranı belirteci (token) kullanın:

1. Yönetici olarak **Wall display** sayfasını açın ve bir belirteç
   oluşturun; ekranın görebileceği grupları (veya tüm sunucuları) seçin.
2. Yalnızca bir kez gösterilen adresi kopyalayın, örneğin
   `https://monitor.example.com/display#token=hsd_…`, ve ekranda açın.

Belirteç adresin `#` işaretinden sonraki kısmında (URL fragment) durur; bu
kısım web sunucularına gönderilmez ve günlüklerine yazılmaz. Belirteç
yalnızca seçilen gruplara okuma erişimi verir ve istendiği an iptal
edilebilir; iptal edildiğinde ekran belirtecin artık geçerli olmadığını
gösterir.

Belirteç kalıcıdır: ekranın tarayıcısı yeniden başlasa da aynı adres
çalışmaya devam eder. Bir ekran çalınır veya el değiştirirse yalnızca o
ekranın belirtecini iptal edin; diğer ekranlar etkilenmez. Her ekran için
ayrı bir belirteç oluşturmak bu yüzden iyi bir alışkanlıktır.

## Seçenekler

Adresin `#` kısmına `&` ile eklenir:

| Seçenek | Anlamı |
|---|---|
| `rotate=<saniye>` | Gruplar arası dönüş süresi (varsayılan 20) |
| `theme=dark` / `theme=light` | Renk şemasını zorlar |

Örnek: `https://monitor.example.com/display#token=hsd_…&rotate=30&theme=dark`

## Ekran kurulumu için ipuçları

- Tarayıcıyı kiosk kipinde açın, örneğin
  `chromium --kiosk --noerrdialogs "https://monitor.example.com/display#token=…"`.
- Ekranın uyku kipine geçmesini ve ekran koruyucuyu kapatın.
- Kendinden imzalı sertifika kullanıyorsanız ekran tarayıcısının
  sertifikaya güvenmesini sağlayın; aksi hâlde her yeniden başlatmada
  uyarı sayfası açılır.
- Bağlantı koparsa ekran verinin bayatladığını gösterir ve bağlantı
  geldiğinde kendiliğinden toparlanır.
