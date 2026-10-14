# Studio M1.2 — ComfyUI production fidelity + UX hotfix (2026-10-14)

## Kullanıcıdan gelen hata

RTX 5080 istasyonunda Studio'nun gömülü veya içe aktarılan API workflow'u 7–13 dakika sonunda hatalı/bozuk model üretirken, aynı workflow ComfyUI'nin kendi penceresinde yaklaşık 2 dakikada doğru model üretti. Referans görselin adı yerine küçük resmi, arşivden silme ve daha büyük 3D viewport isteniyor. Burada kullanıcı tarafından bir ComfyUI execution history veya Studio tarafından üretilen bozuk GLB yüklenmedi; dolayısıyla süre farkının tek sebebini uzaktan kesin teşhis etmek mümkün değil.

## Kodda doğrulanan hata

Studio M1.1 `prepareApiProduction()` kaynak API grafını **sessizce** değiştiriyordu:

- Farklı işlevlerdeki KSampler #3, #12, #18, #23 seed'lerini tek bir değere dönüştürüyordu. Kullanıcının doğrudan export ettiği 63 düğümlü workflow'un orijinal seed seti **56 / 43 / 42 / 42** idi.
- Seçili preset `target_face_count`, texture çözünürlüğü, remesh, upsample, AO ve motor anahtarını her çalışmada yeniden yazıyordu.
- GLB save için gerekli olmayan API preview çıktılarını buduyor, böylece gönderilen prompt kaynak workflow ile birebir olmuyordu.

Bu değişiklikler yalnızca performansı değil üretilecek geometrinin ve PBR sonucunun kendisini de değiştirebilir. Bu hata kesin olarak düzeltildi; gerçek GPU üretimi kullanıcı istasyonunda yeniden kabul testine ihtiyaç duyar.

## M1.2 üretim sözleşmesi

- **Varsayılan `Workflow aynen`**: API Export grafındaki hiçbir node, bağlantı, seed, bake ayarı, Switch, çıktı yolu veya önizleme çıkışı değiştirilmez. Yalnızca seçilen yerel referans görsel, ComfyUI `/upload/image` API'siyle benzersiz dosya adına yüklenir ve #122 `LoadImage.image` alanı değiştirilir. Görselin ham baytları dönüştürülmez.
- **`Özel ayarlar`**: kullanıcı özellikle seçtiğinde geometri/bake parametreleri değişebilir. Seed override ayrıca onaylanmadıkça dört KSampler orijinal seed'leriyle çalışır.
- `productionGraphChanges()` referans dışında farklı bir alan olup olmadığını gösterir; Rust IPC sınırı da `validate_submission_fidelity()` ile kaynak vs gönderilen graph farklarını zorunlu olarak kontrol eder, beklenmedik değişikliği üretim öncesinde reddeder.
- Bütün API çıktı düğümleri korunur. Arşive indirilen GLB yalnızca **`Save3DAdvanced #322`** tarihçe çıktısından alınır; preview-only node başka model diye arşivlenmez.
- `generation.json` manifestinde gönderilen prompt, orijinal prompt, seçili üretim modu, `changedInputs` ve referans kopyası korunur. Tanı ekranı seed'leri, değişen alanları ve #322 çıktısını raporlar.

## UX

- Seçilen referans, dosya adı altında değil **gerçek görsel thumbnail** olarak gösterilir. Salt okunur Rust komutu yalnızca seçilen dosyayı okur; otomatik resim değiştirme yoktur.
- Arşiv satırı ve seçili model inspector'da `Arşivden kaldır` eylemi, onay penceresi ile bulunur.
- Silme **soft delete** işlemi olarak tasarlandı: yalnızca Studio tarafından yönetilen `assets/<uuid>` klasörünü `trash/<uuid>` klasörüne atomik olarak taşır; dış kaynak GLB veya ComfyUI dosyaları hiç silinmez. Başarısız veritabanı işlemi sonrasında dosyalar geri taşınır. Trash klasöründen manuel kurtarma mümkündür, UI restore sonraki aşamaya bırakılmıştır.
- Inspector ağırlıklı daha büyük merkez görünümü, kompakt üst bölge, daha dar yan paneller, mod seçicisi ve kısa durum kartı.

## Kabul kapısı

1. `git pull` sonrası `05_Start-Studio.ps1` ile aynı RTX 5080 istasyonunda uygulamayı başlat, ComfyUI bağlantı adresi `http://127.0.0.1:8191` olarak kalsın.
2. Başarılı ComfyUI koşusunda kullandığın **aynı API JSON** ile aynı referansı seç ve yeni varsayılan **`Workflow aynen`** modunda tek bir model üret.
3. ComfyUI `/history` kaydında #322 GLB oluşumunu ve Studio arşivindeki modelin normal görüntülendiğini doğrula. Studio'nun `generation.json` dosyasında `changedInputs` **yalnızca `122.image`** olmalı.
4. Görsel thumbnail, arşivden kaldır/çöp arşivi, viewport genişliği, LOD işlemleri kontrol edilmeli.
5. Uygulama hâlâ bozuk GLB üretirse `generation.json`, `/history/<prompt_id>` tanı kaydı ve aynı referansla ComfyUI'de başarılı koşunun API JSON'u karşılaştırılmalı. Belge dışında hız/kalite kabulü iddia edilmez.

`main` dalı ve mevcut modeller/workflow'lar değiştirilmez; M1.2 feature branch üzerinden yürütülür.
