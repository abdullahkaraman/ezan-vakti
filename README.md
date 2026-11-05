# Ezan Vakti

Seçtiğin konuma göre bir sonraki ezan vaktine kalan süreyi gösteren küçük bir masaüstü uygulaması.
Vakitler Diyanet tablosundan gelir.

## İndir

**[Son sürümü indir](https://github.com/abdullahkaraman/ezan-vakti/releases/latest)**

| Sistem | Dosya |
| --- | --- |
| macOS 12+ (Apple Silicon ve Intel) | `Ezan.Vakti_*_universal.dmg` |
| Windows 10/11 (64 bit) | `Ezan.Vakti_*_x64-setup.exe` |

Uygulama ücretli bir geliştirici sertifikasıyla imzalı değildir, bu yüzden ilk açılışta sistem uyarır:

- **macOS:** DMG'den uygulamayı `Applications` klasörüne sürükle ve aç. Engellenirse
  `Sistem Ayarları → Gizlilik ve Güvenlik → Yine de Aç`.
- **Windows:** "Windows kişisel bilgisayarınızı korudu" ekranında `Ek bilgi → Yine de çalıştır`.

## Kullanım

- **macOS:** Menü çubuğunda `İkindi'ye 43 dk` görünür. İkona tıklayınca bugünün vakitleri ve konum
  seçici açılır, sağ tıkta `Çıkış` vardır. Dock'ta görünmez.
- **Windows:** Sistem tepsisi ikonun yanına yazı kabul etmediği için dakika ikonun içinde görünür
  (`43`, 100 dakikadan sonra saat olarak `2s`); tam metin ikonun üzerine gelince çıkar. İkon görev
  çubuğundaki `^` menüsünde gizliyse sürükleyip görev çubuğuna al.

## Çalıştırma

Gereksinimler: Xcode Command Line Tools veya Visual Studio C++ araçları, rustup, Node ve pnpm. Rust sürümü
`rust-toolchain.toml` ile sabittir.

```sh
pnpm install
pnpm dev     # geliştirme
pnpm build   # macOS: src-tauri/target/release/bundle/macos/Ezan Vakti.app
cd src-tauri && cargo test
```

## Veri

- Vakitler: `ezanvakti.emushaf.net`, Diyanet tablosunu ilçe bazında veren **resmi olmayan** bir
  ayna. Servis kapanır veya biçim değiştirirse uygulama yeni veri alamaz.
- Saat dilimi: Türkiye için sabit, diğer ülkelerde Open-Meteo Geocoding. Bulunamazsa Mac'in saati
  kullanılır ve pencerede yazar. API'nin `GreenwichOrtalamaZamani` alanı her ülkede `3.0` döndüğü
  için kullanılmaz.
- Vakitler ~30 gün, konum listeleri 30 gün yerelde saklanır. Yenileme, veri 7 günden aza inince
  yapılır. Başarısız deneme önbelleği silmez ve en erken 1 saat sonra tekrarlanır.

## Sürüm çıkarma

`v*` etiketi GitHub Actions'ta testleri çalıştırır, DMG ve Windows kurulumunu üretip taslak release
oluşturur. Dosyalar kontrol edilip taslak elle yayınlanır.

## Bilinen sınırlar

- Açılışta otomatik başlamaz; bildirim veya ezan sesi yoktur.
- Windows ikonu 100 dakikadan sonra yalnız saati gösterir; tam süre ipucu metnindedir.
- Çalışırken Mac'in saat dilimi değişirse, yalnız saat dilimi çözülememiş konumlar için yeniden
  başlatma gerekir.
