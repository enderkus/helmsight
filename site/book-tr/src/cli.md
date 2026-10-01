# Komut satırı

```text
helmsight [--config <yol>] [--data-dir <dizin>] [--log-format text|json] <komut>
```

| Komut | Açıklama |
|---|---|
| `serve [--local] [--listen ADRES]` | Web sunucusunu ve veri toplayıcıları çalıştırır. `--local` yalnızca bu makineyi izler. |
| `init [--force]` | Yapılandırma dosyasını, veri dizinini, anahtar dosyasını ve ilk yöneticiyi oluşturur |
| `user add <ad> [--role viewer\|operator\|admin] [--password-stdin]` | Yerel kullanıcı oluşturur |
| `user remove <ad>` | Kullanıcıyı siler; son yönetici silinemez |
| `user reset-password <ad> [--password-stdin] [--reset-totp]` | Yeni parola belirler ve kullanıcının oturumlarını sonlandırır |
| `user list` | Kullanıcıları listeler |
| `hosts test [--trust] [adlar...]` | Bağlantıyı, kimlik doğrulamayı ve sunucu anahtarlarını denetler |
| `config check` | Yapılandırmayı doğrular ve başvurulan gizli değerlerin var olduğunu denetler |
| `secret set <ad> [--stdin]` | Şifreli bir gizli değer saklar; `secret:<ad>` olarak başvurulur |
| `secret list` / `secret delete <ad>` | Saklanan gizli değerleri listeler veya siler |
| `audit verify` | Denetim kaydının özet zincirini doğrular |

Her komutun ayrıntılı yardımı için `helmsight <komut> --help` kullanın.

## Yapılandırma dosyasının bulunması

Yapılandırma dosyası şu sırayla aranır: `--config`, `$HELMSIGHT_CONFIG`,
`/etc/helmsight/helmsight.toml`, `./helmsight.toml`. Yapılandırma dosyası
olmadan (yerel kip) `--data-dir` ya da yerel kipin varsayılan dizini
(`~/.local/share/helmsight`, root için `/var/lib/helmsight`) kullanılır.

## Sık kullanılan örnekler

```sh
# Parolayı betikten vererek kullanıcı oluşturma (ör. otomasyon)
printf '%s\n' "$PAROLA" | helmsight user add alice --role operator --password-stdin

# TOTP cihazını kaybeden kullanıcı
helmsight user reset-password alice --reset-totp

# Yalnızca iki sunucuyu denemek
helmsight hosts test web-1 db-1

# Slack webhook adresini şifreli saklamak
helmsight secret set slack-webhook

# Gizli değeri bir dosyadan okumak
helmsight secret set smtp --stdin < /run/secrets/smtp

# Geçici olarak farklı bir adreste çalıştırmak
helmsight serve --listen 127.0.0.1:9090
```

`--password-stdin` ve `--stdin` seçenekleri parolanın kabuk geçmişine veya
süreç listesine düşmemesini sağlar; parolaları komut satırı argümanı olarak
vermenin bir yolu bilerek sunulmaz.

## Çıkış kodları

Komutlar başarıda `0`, hata durumunda sıfırdan farklı bir kodla çıkar.
`hosts test` en az bir sunucuya bağlanamazsa, `config check` yapılandırmada
hata bulursa, `audit verify` zincirde bir kırılma bulursa başarısız olur. Bu
sayede komutlar betiklerde ve CI hatlarında denetim olarak kullanılabilir.

## Günlükler

Günlük ayrıntısı `HELMSIGHT_LOG` ile ayarlanır, örneğin
`HELMSIGHT_LOG=debug` veya `HELMSIGHT_LOG=info,server=debug`.
`--log-format json` günlükleri, bir günlük toplama sistemine aktarmaya
uygun JSON satırları olarak yazar. Günlüklerde hiçbir zaman gizli değer,
parola, oturum belirteci veya toplanan çıktı bulunmaz.
