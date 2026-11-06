Bir sonraki ezan vaktine kalan süreyi gösteren masaüstü uygulaması. Vakitler Diyanet tablosundan gelir.

## macOS'ta kurulum

Aşağıdaki **Assets** bölümünden `.dmg` dosyasını indir. Uygulama ücretli Apple sertifikasıyla imzalı olmadığı için macOS ilk açılışı engeller. Bu adımlar bir kez yapılır:

1. `.dmg` dosyasını aç ve **Ezan Vakti**'yi `Applications` klasörüne sürükle.
2. Uygulamayı aç. "Ezan Vakti açılamadı" ya da "doğrulanamadı" uyarısı çıkarsa pencereyi kapat.
3. **Sistem Ayarları → Gizlilik ve Güvenlik** bölümüne gir, **Güvenlik** başlığına kadar aşağı in.
4. Ezan Vakti'nin yanındaki **Yine de Aç** düğmesine bas ve parolanla ya da Touch ID ile onayla.

Güvenlik ayarını genel olarak değiştirmen gerekmez, izin yalnızca bu uygulama için verilir.

Uygulama Dock'ta görünmez, ekranın üstündeki **menü çubuğuna** yerleşir. İlk açılışta menü çubuğundaki `Konum seç` yazısına tıkla.

"Hasarlı" uyarısı çıkarsa Terminal'de şunu çalıştırıp yeniden aç:

```sh
xattr -cr "/Applications/Ezan Vakti.app"
```

## Windows'ta kurulum

Aşağıdaki **Assets** bölümünden `-setup.exe` dosyasını indir ve çalıştır. "Windows kişisel bilgisayarınızı korudu" ekranı çıkarsa **Ek bilgi → Yine de çalıştır**'a bas.

## Güncelleme

Yeni sürümler uygulama içinden gelir. **v0.1.0 kullanıyorsan** bu mekanizma sende yok, bu sürümü bir kez elle kur.
