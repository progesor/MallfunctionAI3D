# Mallfunction — ComfyUI → Unity 3D Asset Pilot v0.1.4

**Hazırlanma tarihi:** 2026-10-14  
**Durum:** ComfyUI Desktop 0.40.0, RTX 4070 Ti SUPER, **43/43 node + 7/7 model PASS**. PLAYBOX Fox Plush Pixal3D pilotundan **7.998 triangle / 1 materyal / 1024 PBR texture içeren gerçek GLB** üretildi; GPU ölçümü tamamlandı. Blender LOD ve Unity kabulü bekliyor. İkinci RTX 5080 istasyonu bu pilotta kullanılmadı.  
**v0.1.1 düzeltmesi:** Windows PowerShell 5.1 ortamında audit scriptinin `param()` içinden `$PSScriptRoot` kullanması nedeniyle oluşan `Join-Path` hatası giderildi. ComfyUI Desktop kullanıcısı için `-DockerContainer` verilmesi gerekmez.
**v0.1.2 düzeltmesi:** PowerShell 5.1 `/object_info` yanıtını kimi ortamlarda nesne yerine `Length` özelliği üzerinden okuduğu için 0/29 yanlış negatifi oluştu. Artık HTTP JSON açıkça parse ediliyor; gerekirse `/object_info/{node_class}` tekil sorgularına düşüyor. Doğrulanamayan düğümler `unknown` olarak işaretleniyor, `missing` sayılmıyor.

**Güvenlik ilkesi:** Var olan çalışan Docker/WSL/ComfyUI kurulumu ve kurulu model ağırlıkları değiştirilmez; model ağırlıkları indirilmez; workflow'lar yalnızca bu ayrı klasöre hazırlanır. Batch üretim **etkin değildir**.


### İlk Pixal3D Fox Plush — gerçek koşu PASS

- 14 Ekim 2026'da orijinal tilki referansıyla **başarılı GLB/PBR üretimi** yapıldı. GLB: 3,339,600 byte; 7,998 triangle; 6,252 vertex; UV + normal + 1 materyal + 1024x1024 PBR texture.
- GPU peak %100 / 15,014 MiB toplam VRAM kullanımı; GPU'yu tam yükte kullanmak **kullanıcının kabul ettiği koşul**. Üretim çoğunlukla **tekli**, batch varsayılanı kapalı.
- Ayakta lokal deformasyon, yüzeyde texture/bake izleri mevcut; prototip için kabul edilebilir. Dokulu GLB'de ilk analizde görünen 168 index adası büyük ölçüde UV seam vertex ayrımı; konuma göre birleşik analizde 2 kapalı kabuk var. Kaynak GLB'ye müdahale yok.
- Ayrıntılı teknik/görsel kayıt: docs/AI3D_PLAYBOX_FOX_FIRST_RUN_2026-10-14.md.
- **Yeni pratik komut (Blender PATH'te olmasa da):** `.\04_Prepare-LODs.ps1 -DryRun` ve ardından `.\04_Prepare-LODs.ps1`. Otomatik kurulum yolunu bulamazsa `-BlenderPath "C:\GERCEK\YOL\blender.exe"` ile çalıştır. Üretici kaynak GLB'yi değiştirmez.

### Üretim kararı — 14 Ekim 2026

- **Aktif istasyon:** RTX 4070 Ti SUPER 16 GB; ComfyUI Desktop 0.40.0; Windows 11; 64 GB RAM.
- **İkinci istasyon:** RTX 5080 16 GB; bu istasyonun ComfyUI sürümü, model cache durumu ve node uyumluluğu ayrıca teyit edilecek. **İstasyon değişimi zorunlu değil.**
- **Son denetim (15:54):** 43/43 çalıştırılabilir node ve 7/7 model ismi API ile doğrulandı. Bulk `/object_info` parsing, `Background` / `background` anahtar çakışmasından dolayı tekil node sorgularına düştü; 43/43 PASS. **Tekrar denetim gerekli değil.**
- **Üretim durumu:** Bu pilotun ilk Pixal3D GLB, PBR ve GPU ölçümleri tamamlandı; Blender LOD/Unity kabulü bekliyor.
- **Bir sonraki iş:** Yeni Pixal3D üretimi gerekmiyor. `git pull`, `04_Prepare-LODs.ps1 -DryRun`, `04_Prepare-LODs.ps1` ile Blender LOD testini yap; hata/raporları kontrol et.
- **Ham raporlar GitHub'a eklenmiyor**; sonuç özeti `docs/AI3D_AUDIT_2026-10-14.md`, istasyon planı `docs/WORKSTATIONS.md`.

## 1) Kurulum denetimi (aktif RTX 4070 istasyonunda tamamlandı)

ZIP'i Windows'ta ComfyUI **dışında** bir klasöre aç (örnek: `C:\Works\MallfunctionAI3D`). Çalışan ComfyUI arayüzün açık olsun. Windows PowerShell'de:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\01_Inspect-ComfyUI.ps1 -ComfyUrl "http://127.0.0.1:8188"
```

Docker container adını biliyorsan ve container içindeki PyTorch/CUDA sürümlerini de kaydetmek istiyorsan:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\01_Inspect-ComfyUI.ps1 -ComfyUrl "http://127.0.0.1:8188" -DockerContainer "COMFY_CONTAINER_ADI"
```

Port farklıysa `-ComfyUrl` değiştir. Betik ComfyUI API'sinden `/system_stats`, `/object_info`, `/models/*` verilerini okur, ayrıca `nvidia-smi`, Docker ve WSL envanterini alır. Bir komut mevcut değilse bunu raporlar ve diğer kontrollere devam eder. Modelleri veya node'ları değiştirmez. `reports/Comfy_Audit_<tarih>.zip` dosyasını geri paylaş. Ham rapor yerel dosya yollarını ve kurulu model isimlerini içerebilir; herkese açık yerlere yükleme.

**Karar kapısı geçti:** 43/43 node ve 7/7 model mevcut. Bu komutu aynı istasyonda yeniden çalıştırmak zorunda değilsin. RTX 5080 istasyonuna geçildiğinde ayrı audit yapılır.

## 2) Resmî workflow ve iki pilot kopyası GitHub'da hazır

**Önerilen yol:** Repoda `upstream/official_pixal3d_trellis2.json` ve `workflows/Mallfunction_PLAYBOX_FoxPlush_{pixal3d,trellis2}_pilot_v01.json` dosyaları artık mevcut. `git pull` yapman yeterli. **Bu repoda `02_Prepare-Pilot.ps1` çalıştırma:** var olan workflow'ları korumak için üretici zaten üzerine yazmayı reddedecek.

Script sadece workflow'ların henüz bulunmadığı bağımsız/boş bir kopyada kullanılabilir. Böyle bir kopyada PowerShell:

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
powershell -NoProfile -ExecutionPolicy Bypass -File .\03_Monitor-Pilot.ps1 -ComfyUrl "http://127.0.0.1:8188" -MaxSeconds 1800 -Station "4070ti-super"
```

4. Ölçüm başladıktan sonra **yalnızca bu workflow'u** ComfyUI'da Queue Prompt ile başlat. Ölçüm aracı ComfyUI kuyruğu etkinleştiğinde başlatılan işi gözlemler, kuyruk 8 saniye boşaldıktan sonra raporu kaydeder.
5. ComfyUI Desktop için ilk çıktıyı gerçek `C:\Users\proge\AppData\Local\Comfy-Desktop\ComfyUI-Shared\output\3d\Mallfunction\` klasöründe ara. Çalışma grafı `Save3DAdvanced` ile GLB üretimini hedefler. Çıktı oluşturulmadıysa kaydı ve hata mesajını gönder; çalışan sistemi güncelleme.

**Parametreler:** Pixal/TRELLIS anahtarları `false`/`true`; yapı çözünürlüğü `32`; upsample `1024`; remesh `512`; decimate `8000` hedef yüz; unwrap/bake texture `1024`; normal map `1024`; AO `1024`, 32 örnek. Yüksek detaylı mesh normal/AO bake'in kaynak tarafında kalır. Bu optimizasyon *sadece ilk denemedir* ve OOM ihtimalini sıfırlamaz.

**GPU raporları:** `reports/GPU_Pilot_<tarih>.csv` ve `.json`; `station` alanı `4070ti-super` veya `5080` olarak istasyonları ayırır. Peak VRAM, GPU kullanım yüzdesi, host RAM ve kuyruk aktivitesi gerçek makineden alınır. VRAM diğer uygulamaları da içerir. Ölçüm kuyruk süresi yaklaşık değerdir; özel bir benchmark koşusu değildir.

## 4) GLB yapısı ve Blender/Unity hazırlığı

ComfyUI'dan çıkan `.glb` dosyasını önce hafif yapısal denetimden geçir:

```powershell
py -3 .\tools\validate_glb.py "D:\Assets\PLAYBOX_FoxPlush_pixal3d.glb" --target-triangles 8000
```

GLB denetimi: mesh/üçgen, UV (`TEXCOORD_0`), normal, materyal sayısı, gömülü resimler ve bağlanan base-color / metallic-roughness / normal / occlusion haritaları. Yanına `*.qc.json` yazar. **Geometrinin görsel doğruluğu, UV overlap, non-manifold ve tangent-space artefaktlarını doğrulamaz.** Bu nedenle yapısal rapor tek başına kalite kabulü sayılmaz.

**Windows / ComfyUI Desktop / Blender GUI kullanıcıları için önerilen komut:** Blender uygulaması PATH'e ekli olmak zorunda değil. Yeni başlatıcı standart Blender kurulum klasörlerini ve mevcut PLAYBOX Pixal3D GLB'sini otomatik arar:

```powershell
cd C:\Works\MallfunctionAI3D
git pull
.\04_Prepare-LODs.ps1 -DryRun
.\04_Prepare-LODs.ps1
```

Blender otomatik bulunamazsa gerçek blender.exe yolunu ver (örnek, dosyanın gerçekten bulunduğunu doğrula):

```powershell
.\04_Prepare-LODs.ps1 -BlenderPath "C:\Program Files\Blender Foundation\Blender 4.5\blender.exe"
```

Daha önceki bir testin üzerine yazmamak için çıktı klasörü tarih/saat eklenerek oluşturulur. Tam log `reports/FoxBlenderTest_*/Blender_Run.log` içine kaydedilir. Kaynak GLB değişmez; LOD görsel kabulü ayrıca gereklidir.

**Manuel blender.exe çağrısı (alternatif):**

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

`python -m unittest discover -s tests -v`: 11 sentetik test geçti (workflow edit invariants, SHA, basic GLB parsing). Yerel GPU/Pixal3D üretimi: **PASS**; Blender komut satırı üzerinden LOD çıktıları henüz doğrulanmadı. 43/43 node + 7/7 model API uyumluluğu ayrıca gerçek Windows ComfyUI Desktop ortamında doğrulandı; bunlar pilot üretim başarısı anlamına gelmez.
