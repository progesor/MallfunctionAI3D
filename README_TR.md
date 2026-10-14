# Mallfunction — ComfyUI → Unity 3D Asset Pilot v0.1

**Hazırlanma tarihi:** 2026-10-14  
**Durum:** Araç paketi ve sentetik testler hazır. Kullanıcının PC'sindeki ComfyUI, RTX 5080, Blender ve gerçek fox-plush varlığı **henüz bu paketle çalıştırılmadı**.  
**Güvenlik ilkesi:** Var olan çalışan Docker/WSL/ComfyUI kurulumu ve kurulu model ağırlıkları değiştirilmez; model ağırlıkları indirilmez; workflow'lar yalnızca bu ayrı klasöre hazırlanır. Batch üretim **etkin değildir**.

## 1) Önce mevcut kurulumu incele

ZIP'i Windows'ta ComfyUI **dışında** bir klasöre aç (örnek: `C:\Works\MallfunctionAI3D`). Çalışan ComfyUI arayüzün açık olsun. Windows PowerShell'de:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\01_Inspect-ComfyUI.ps1 -ComfyUrl "http://127.0.0.1:8188"
```

Docker container adını biliyorsan ve container içindeki PyTorch/CUDA sürümlerini de kaydetmek istiyorsan:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\01_Inspect-ComfyUI.ps1 -ComfyUrl "http://127.0.0.1:8188" -DockerContainer "COMFY_CONTAINER_ADI"
```

Port farklıysa `-ComfyUrl` değiştir. Betik ComfyUI API'sinden `/system_stats`, `/object_info`, `/models/*` verilerini okur, ayrıca `nvidia-smi`, Docker ve WSL envanterini alır. Bir komut mevcut değilse bunu raporlar ve diğer kontrollere devam eder. Modelleri veya node'ları değiştirmez. `reports/Comfy_Audit_<tarih>.zip` dosyasını geri paylaş. Ham rapor yerel dosya yollarını ve kurulu model isimlerini içerebilir; herkese açık yerlere yükleme.

**İlk karar kapısı:** 29 temel node'un ve 7 ortak model ağırlığının durumu raporda netleşsin. Eksik olanları topluca indirme/güncelleme yapmadan birlikte değerlendireceğiz. Eğer API `localhost` üzerinden erişilemiyorsa önce Docker yayınlanan portunu belirleyeceğiz.

## 2) Resmî workflow'dan iki bağımsız pilot üret

Bu adımı denetimden sonra uygula. Aynı ZIP klasöründeki PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\02_Prepare-Pilot.ps1
```

Bu işlem, sadece **resmî workflow JSON'unu** Comfy-Org'un sabitlenmiş commit'inden `upstream/` klasörüne indirir (3D model ağırlığı indirmez). Git blob SHA-1 değeri kontrol edilir. `workflows/` içine aşağıdaki 2 yeni dosyayı oluşturur:

- `Mallfunction_PLAYBOX_FoxPlush_pixal3d_pilot_v01.json`
- `Mallfunction_PLAYBOX_FoxPlush_trellis2_pilot_v01.json`

Her birinin yanında `*.audit.json` bulunur: grafın orijinal commit'i, model modu, değiştirilen parametreler, hazırlık durumu. Bağlantılar eklenmez/silinmez. Var olan pilot çıktısı varsa **üzerine yazmayı reddeder**.

**GitHub'a erişilemiyorsa:** Kullanmakta olduğun ComfyUI'dan resmî `Pixal3D & TRELLIS.2: Image to Model` şablonunu JSON olarak `upstream/official_local.json` adına kaydet ve şunu çalıştır:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\02_Prepare-Pilot.ps1 -Source ".\upstream\official_local.json" -AllowOtherSource
```

Alternatif sürüm için `--allow-other-source` yalnızca topolojik asgari kontrolleri geçirir; resmî pinlenmiş revizyon kadar güçlü bir garanti vermez. Görseli henüz ComfyUI'da yüklemediysen `LoadImage` düğümünün beklediği isim `PLAYBOX_FoxPlush_reference.png` olarak görünür. Kaynak dosyanın ComfyUI'ya yüklenmesi ayrıca gerekir.

## 3) Fox Plush — önce **tek bir Pixal3D** çalıştır

**Referans kaynak:** Daha önce Meshy'de kullanılan PLAYBOX tilki pelüşünün **orijinal** tek ürün PNG'si. Bu ZIP'te referans bulunmuyor; burada mevcut kullanıcı ekleri insan karakter konseptleriydi. Başka bir tilki referansıyla kıyaslamayı değiştirmeyelim.

1. `workflows/Mallfunction_PLAYBOX_FoxPlush_pixal3d_pilot_v01.json` dosyasını ComfyUI penceresine sürükle/yükle.
2. `LoadImage` düğümünden **gerçek** PLAYBOX tilki PNG'sini seç/yükle. Düğümde resmin görüntülendiğini kontrol et. Yeniden yüklediğinde dosya adı değişebilir; gerçek dosya seçimi referans adından daha önemlidir.
3. Tercihen işlemi kuyruğa göndermeden önce Windows'ta GPU ölçümünü başlat:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\03_Monitor-Pilot.ps1 -ComfyUrl "http://127.0.0.1:8188" -MaxSeconds 1800
```

4. Ölçüm başladıktan sonra **yalnızca bu workflow'u** ComfyUI'da Queue Prompt ile başlat. Ölçüm aracı ComfyUI kuyruğu etkinleştiğinde başlatılan işi gözlemler, kuyruk 8 saniye boşaldıktan sonra raporu kaydeder.
5. İlk çıktıyı `ComfyUI/output/3d/Mallfunction/` altında ara. Çalışma grafı `Save3DAdvanced` ile GLB üretimini hedefler. Çıktı oluşturulmadıysa kaydı ve hata mesajını gönder; çalışan sistemi güncelleme.

**Parametreler:** Pixal/TRELLIS anahtarları `false`/`true`; yapı çözünürlüğü `32`; upsample `1024`; remesh `512`; decimate `8000` hedef yüz; unwrap/bake texture `1024`; normal map `1024`; AO `1024`, 32 örnek. Yüksek detaylı mesh normal/AO bake'in kaynak tarafında kalır. Bu optimizasyon *sadece ilk denemedir* ve OOM ihtimalini sıfırlamaz.

**GPU raporları:** `reports/GPU_Pilot_<tarih>.csv` ve `.json`. Peak VRAM, GPU kullanım yüzdesi, host RAM ve kuyruk aktivitesi gerçek makineden alınır. VRAM diğer uygulamaları da içerir. Ölçüm kuyruk süresi yaklaşık değerdir; özel bir benchmark koşusu değildir.

## 4) GLB yapısı ve Blender/Unity hazırlığı

ComfyUI'dan çıkan `.glb` dosyasını önce hafif yapısal denetimden geçir:

```powershell
py -3 .\tools\validate_glb.py "D:\Assets\PLAYBOX_FoxPlush_pixal3d.glb" --target-triangles 8000
```

GLB denetimi: mesh/üçgen, UV (`TEXCOORD_0`), normal, materyal sayısı, gömülü resimler ve bağlanan base-color / metallic-roughness / normal / occlusion haritaları. Yanına `*.qc.json` yazar. **Geometrinin görsel doğruluğu, UV overlap, non-manifold ve tangent-space artefaktlarını doğrulamaz.** Bu nedenle yapısal rapor tek başına kalite kabulü sayılmaz.

Blender'da isteğe bağlı LOD taslağı (ilk gerçek modelden **sonra**):

```powershell
blender -b --python .\blender\prepare_lods.py -- --input "D:\Assets\PLAYBOX_FoxPlush_pixal3d.glb" --outdir ".\reports\FoxBlenderTest"
```

Blender komutu 8K / 5K / 3K **yaklaşık hedef üçgenli** ayrı GLB'ler ve `Blender_LOD_Report.json` çıkarır. Kaynak GLB'yi değiştirmez. Parçaları birleştirir; yeni normal/AO bake yapmaz. Normal haritası düşük LOD'da bozulursa gerçek bir yeniden bake ve/veya UV düzeltmesi gerekir. Gerçek ölçüler onaylanmadan otomatik ölçeklendirme yapmaz; bilinen yükseklik için `--height-m 0.35` gibi **bilinçli** değer verilebilir.

**Unity kabulü daha sonra:** URP materyal eşleştirmesi, tek asset/prefab, gerekli LODGroup, basit collider; 2m/5m/10m görünüm ve 100/1000 adet tekrarda Frame Debugger/Profiler. Bu paket Unity sahnesinde test yaptığını iddia etmez.

## 5) Karşılaştırma ve karar kapıları

Pixal3D ilk başarılı GLB + GPU sonucu gelmeden TRELLIS.2 koşusuna veya batch üretime geçme. Sonra **aynı** görselle TRELLIS.2 grafını çalıştır; Hunyuan3D 2.1 shape-only kaynakla da geometri kıyasla. Aynı seed numarası farklı üreticilerde aynı rastgele örnek veya kaliteyi garanti etmez.

İlk kabul için şunlar kontrol edilmeli: tilkinin kulak/kuyruk/göz/ayak silueti; arkadan tutarlılık; herhangi bir kopuk parça; UV ve PBR; ters normaller ve ağır non-manifold; 8K/5K/3K kamera testi; gömülü texture/GLB dosya boyu; GPU peak VRAM; jenerasyon süresi ve Unity render maliyeti. İlk eşikler sabit üretim standardı değil, test başlangıçlarıdır.

## Sabitlenen kaynak ve lisans notları

- Resmî grafik (sabit commit): https://github.com/Comfy-Org/workflow_templates/blob/3e89967349dd95fa68ce3b36966f41d642af8ea7/templates/3d_pixal3d_trellis2_image_to_model.json
- Resmî Pixal3D kullanım rehberi: https://docs.comfy.org/tutorials/3d/pixal3d
- Resmî TRELLIS.2 kullanım rehberi: https://docs.comfy.org/tutorials/3d/trellis2
- Pixal3D kaynak MIT (üçüncü taraf bileşenleri ayrı): https://github.com/TencentARC/Pixal3D
- Microsoft TRELLIS.2 kaynak/model MIT; bazı NVIDIA bağımlılıkları farklı lisanslı: https://github.com/microsoft/TRELLIS.2
- Hunyuan3D 2.1 Tencent Community License ayrı koşullar ve coğrafi sınırlar içerir: https://github.com/Tencent-Hunyuan/Hunyuan3D-2.1/blob/main/LICENSE
- Çıktıları oyunda dağıtmadan önce indirilen **ağırlıkların** ve gerekli bileşenlerin hakları ayrıca doğrulanmalı. Bu dosya hukuki lisans izni değildir.

## Paket test durumu

`python -m unittest discover -s tests -v`: 5 sentetik test geçti (workflow edit invariants, SHA, basic GLB parsing). Yerel GPU/Blender gerçek çalıştırma durumu: **bekliyor**. Testler ComfyUI sürüm uyumluluğunu kanıtlamaz.
