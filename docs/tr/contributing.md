# Katkıda bulunmak

Yardımınız için teşekkürler. Bu belge geliştirme ortamını, her değişikliğin
geçmesi gereken denetimleri ve helmsight'ı güvenli tutan kuralları anlatır.

*[English version](https://github.com/enderkus/helmsight/blob/main/CONTRIBUTING.md)*

Sorular, hata bildirimleri ve önerilerinizi Türkçe veya İngilizce olarak
[GitHub Issues](https://github.com/enderkus/helmsight/issues) üzerinden
iletebilirsiniz. Kod, kod yorumları ve commit mesajları İngilizce yazılır.

## Geliştirme ortamı

Gereksinimler: Rust stable (bkz. `rust-toolchain.toml`), Node.js 22, npm ve
entegrasyon testleri için Docker.

```sh
(cd web && npm ci && npm run build)   # program web/dist dizinini gömer
cargo build
./target/debug/helmsight serve --local --data-dir /tmp/helmsight-dev
```

Arayüz üzerinde çalışırken arka ucu ve Vite geliştirme sunucusunu yan yana
çalıştırın; Vite `/api` isteklerini `http://127.0.0.1:8080` adresine
yönlendirir (`HELMSIGHT_DEV_BACKEND` ile değiştirilebilir):

```sh
cargo run -- serve --config dev.toml
(cd web && npm run dev)
```

Arayüz derlenmemişse `cargo build` yerine basit bir yer tutucu sayfa gömer
ve uyarı verir; böylece yalnızca Rust tarafında çalışmak için Node.js
gerekmez.

## Proje yapısı

| Yol | İçerik |
|---|---|
| `crates/collect` | Uzak POSIX betiği (`src/remote.sh`), çıktısının ayrıştırıcıları ve oran hesapları. Saf kod, G/Ç yok. |
| `crates/transport` | SSH oturumları (russh), sunucu anahtarı doğrulama, yerel çalıştırma |
| `crates/store` | SQLite şeması, özetlemeli metrikler, envanter farkları, alarmlar, kullanıcılar, denetim kaydı |
| `crates/alerts` | Kural değerlendirme ve bildirim kanalları |
| `crates/common` | Yapılandırma şeması ve doğrulaması, gizli değerler, roller, kural ifadeleri |
| `crates/server` | HTTP API, kimlik doğrulama, veri toplama motoru ve arka plan işleri |
| `crates/helmsight` | Komut satırı programı |
| `web` | Svelte ve TypeScript arayüz |
| `tests/docker` | Örnek çıktılar ve entegrasyon testleri için konteyner imajları |
| `scripts` | Bakım betikleri |
| `site` | Web sitesi ve belgeler (mdBook; `site/book` İngilizce, `site/book-tr` Türkçe) |

## Denetimler

Her commit şunları geçmelidir:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
(cd web && npm run check && npm run lint && npm test && npm run build)
```

Entegrasyon testleri sshd konteynerleri başlatır ve veri toplamayı uçtan
uca doğrular; izlenen sunucuya hiçbir şey yazılmadığı da buna dahildir:

```sh
HELMSIGHT_DOCKER_TESTS=1 cargo test -p transport --test docker
# altı dağıtımın hepsi:
HELMSIGHT_DOCKER_TESTS=1 HELMSIGHT_DOCKER_DISTROS="debian ubuntu rocky fedora opensuse alpine" \
  cargo test -p transport --test docker
```

## Ayrıştırıcı örnek çıktıları

`crates/collect/tests/fixtures` dizini Debian, Ubuntu, Rocky, Fedora,
openSUSE ve Alpine'den root ve yetkisiz kullanıcı olarak alınmış gerçek
betik çıktılarını içerir. `remote.sh` dosyasını değiştirdikten sonra
bunları yeniden üretin:

```sh
scripts/capture-fixtures.sh            # tüm dağıtımlar
scripts/capture-fixtures.sh alpine     # yalnızca biri
```

Örnek çıktılardaki farkı gözden geçirin; bunlar test takımının bir
parçasıdır.

## Kod kuralları

- **Uzak çıktıya güvenilmez.** Ayrıştırıcılar rastgele baytları kabul
  etmeli, asla çökmemeli (`collect` crate'i `unwrap`, `panic` ve
  indekslemeyi yasaklar), sakladıklarının boyutunu sınırlamalı ve
  gerekçesiyle "n/a" durumuna düşmelidir. Gerçek çıktıyla bir test ekleyin
  ve `tests/fuzz.rs` içindeki özellik tabanlı testlerin kapsamasını
  sağlayın.
- **Uzak betik salt okunurdur.** Yalnızca POSIX `sh`, geçici dosya yok, her
  yönlendirme `/dev/null`'a (bir birim testi bunu zorunlu kılar), her araç
  isteğe bağlı. `shellcheck -s sh` ile denetleyin.
- **Rastgele komut yok.** Bir sunucuda çalıştırılmak üzere komut, argüman
  veya yol kabul eden bir API asla eklemeyin.
- **Uzak veriyi asla HTML olarak işlemeyin.** `{@html}` yok, `innerHTML`
  yok; ESLint ikisini de reddeder. Arayüzü sıkı CSP ile uyumlu tutun: satır
  içi betik veya `style="..."` özniteliği yok (sınıflar veya `style:`
  yönergeleri kullanın).
- **Gizli değerleri asla günlüğe yazmayın**: parolalar, belirteçler veya
  toplanan çıktı dahil.
- **Arayüz metinleri** kısa, olgusal ve tekniktir. Hata mesajları ne
  olduğunu ve sonra ne yapılacağını söyler. Durum hiçbir zaman yalnızca
  renkle anlatılmaz.

## Commit'ler ve pull request'ler

- [Conventional Commits](https://www.conventionalcommits.org/tr/) kullanın:
  `feat(collect): parse /proc/pressure`,
  `fix(ui): keep table header sticky`.
- Commit'leri küçük ve derlenebilir tutun; her commit yukarıdaki denetimleri
  geçer.
- Kullanıcıyı etkileyen değişiklikler için `CHANGELOG.md` dosyasında
  "Unreleased" başlığını, davranış veya yapılandırma değiştiğinde de
  belgeleri güncelleyin. Belgelerde bir değişiklik yaptığınızda İngilizce
  ve Türkçe sürümleri birlikte güncellemeye çalışın; Türkçesini
  yazamıyorsanız pull request'te belirtmeniz yeterlidir.

## Projeyi yeniden adlandırmak

Ürün adı üç yerde tanımlıdır: `crates/helmsight/Cargo.toml` içindeki program
crate'inin adı (ve `[[bin]]` adı), `crates/common/src/lib.rs` içindeki
`PRODUCT_NAME` (çerez adları, veritabanı dosyasının adı, Prometheus metrik
önekleri) ve `web/src/lib/brand.ts` içindeki `PRODUCT_NAME`.

## Güvenlik sorunlarını bildirmek

Güvenlik açıkları için lütfen herkese açık kayıt açmayın; bkz.
[güvenlik modeli](https://enderkus.github.io/helmsight/tr/docs/security.html#güvenlik-açığı-bildirmek).
