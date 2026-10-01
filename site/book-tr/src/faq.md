# Sık sorulan sorular

**Neden ajan (agent) kullanılmıyor?**
Ajanların her sunucuya kurulması, güncellenmesi, yapılandırılması ve
güvenilmesi gerekir; ayrıca saldırı yüzeyini genişletirler. SSH zaten her
sunucuda vardır, zaten sıkılaştırılmıştır ve zaten denetlenmektedir.
helmsight buna yalnızca okuma yapan bir oturum ekler.

**Sunucuya ne kadar yük bindirir?**
Her 5 saniyede bir, `/proc` altındaki dosyaları okuyan ve birkaç hafif aracı
çalıştıran kısa bir kabuk betiği: birkaç milisaniyelik CPU zamanı. Bekleyen
güncellemeler gibi pahalı denetimler 6 saatte bir çalışır. Aralıklar
[`[collect]`](configuration.md#collect) bölümünden değiştirilebilir.

**Merkez sunucu ne kadar kaynak ister?**
helmsight tek bir süreçtir; sunucu başına bir kalıcı SSH oturumu ve
kompakt metrik satırları tutar. Birkaç yüz sunucunun küçük bir sanal
makinede rahatça izlenmesi beklenir.

**root yetkisi gerekir mi?**
Hayır. Yetkisiz, yalnızca bu işe ayrılmış bir hesap kullanın. Birkaç ayrıntı
grup üyeliği gerektirir (başarısız girişler için günlük erişimi,
konteynerler için Docker soketi); helmsight okuyamadığı her şey için
gerekçesiyle birlikte "n/a" gösterir.

**Sunuculara gerçekten hiçbir şey yazmıyor mu?**
helmsight'ın komutları hiçbir şey yazmaz; bunu altı dağıtımda çalışan bir
entegrasyon testi denetler. SSH girişleri sunucu tarafından her zamanki gibi
kaydedilir (wtmp, lastlog, journal ve Ubuntu'da hesabın ev dizininde bir
kerelik `pam_motd` işareti). `dnf` ve `yum` her zaman günlük dosyası
yazdığı için RHEL ailesindeki sunucularda bekleyen güncellemeler yalnızca
`collect.dnf_updates = true` ile listelenir.

**Hangi dağıtımlar destekleniyor?**
Debian, Ubuntu, RHEL ve türevleri (Rocky, Alma), Fedora, openSUSE ve
Alpine. SSH sunucusu ve POSIX kabuğu olan her Linux'ta çalışması beklenir;
eksik araçlar "n/a" olarak görünür.

**Linux dışı sistemleri (BSD, macOS, Windows) izleyebilir miyim?**
Hayır. Toplama betiği Linux'un `/proc` dosya sistemine dayanır.

**Atlama sunucusu (jump host / bastion) kullanabilir miyim?**
Henüz değil. İçe aktarılan SSH yapılandırmasında `ProxyJump` olan sunucular
uyarıyla atlanır.

**Kaç sunucuyu kaldırabilir?**
Veri toplama eşzamanlıdır ve sunucu başına bir kalıcı oturum kullanır;
aynı anda toplanan sunucu sayısı `collect.max_parallel` ile sınırlanır.
Depolama her örnek için sunucu başına tek ve kompakt bir satır kullanır.

**Veriler nerede?**
Veri dizininde: `helmsight.db` (SQLite), `secret.key`, `known_hosts` ve
`tls/`. Anahtar dosyasını veritabanından ayrı yedekleyin; saklanan gizli
değerler onsuz çözülemez. Ayrıntılar için
[Kurulum → Yedekleme](installation.md#yedekleme).

**Verileri dışarıya aktarabilir miyim?**
Evet: JSON API ve OpenAPI belgesi, Prometheus uç noktası ve alarmlar için
webhook'lar var. Bkz. [API ve entegrasyonlar](api.md).

**Koyu tema var mı?**
Evet. Arayüz işletim sisteminin tercihine uyar; üst çubuktan
değiştirebilirsiniz.

**Arayüz Türkçe mi?**
Hayır, arayüz şimdilik İngilizcedir. Bu belgeler arayüzdeki adları
Türkçe karşılıklarıyla birlikte verir.

**Üretimde kullanabilir miyim?**
helmsight 0.x sürümündedir: test edilmiştir, ancak üretimde henüz az
kullanılmıştır ve bağımsız bir güvenlik denetiminden geçmemiştir. Önce
kritik olmayan sunucularda deneyin, [güvenlik modelini](security.md)
okuyun ve sorumluluğun kullanıcıda olduğunu unutmayın.

**Bir hata veya güvenlik açığı buldum.**
Hataları [GitHub Issues](https://github.com/enderkus/helmsight/issues)
üzerinden bildirin. Güvenlik açıklarını ise herkese açık bir kayıt açmadan,
[güvenlik modelinde](security.md#güvenlik-açığı-bildirmek) anlatıldığı gibi
gizli olarak bildirin.
