# Mallfunction AI3D Studio — M1.3 Workbench

**Durum (2026-10-14):** RTX 5080 istasyonunda Studio üzerinden referansla 3D üretimi, GLB görüntüleme ve Blender LOD kullanıcı tarafından çalışır durumda doğrulandı. **M1.3 yalnızca arayüz/yerleşim yenilemesidir**; mevcut başarılı üretim koduna müdahale etmez. Yeni arayüzün Windows üzerinde kullanıcı kabul testi bekleniyor. Ayrıntılar: `docs/WORKBENCH_UI.md` ve `../docs/AI3D_STUDIO_M1_3_UI_WORKBENCH_2026-10-14.md`.

## Hedef

Yerel Windows arayüzünde referans yükle → workflow ve kalite seç → ComfyUI'ye gönder → durumu takip et → GLB'yi arşive indir → PBR/Clay/Wireframe/Normal modlarında aç → Blender ile ayrı LOD üret. ComfyUI, Blender, mevcut modellere ve pilot scriptlerine dokunmaz.

## İlk Windows çalıştırma

1. `feat/desktop-studio-foundation` branch'ine geç: `git fetch origin && git switch feat/desktop-studio-foundation`. Repoda commitlenmemiş değişikliklerin varsa önce güvenli şekilde commit/stash et.
2. Ana repo klasöründe `powershell -NoProfile -ExecutionPolicy Bypass -File .\05_Start-Studio.ps1 -CheckOnly` çalıştır. Node.js LTS ve Rust stable + Microsoft C++ Build Tools / WebView2 gereklidir. Eksikse script değişiklik yapmadan durur.
3. `powershell -NoProfile -ExecutionPolicy Bypass -File .\05_Start-Studio.ps1` çalıştır. İlk çalıştırma yalnız `studio/node_modules` içine npm bağımlılıklarını indirir ve Tauri derlemesini yapar. Mevcut ComfyUI Desktop açık kalmalıdır.
4. Studio > Settings ekranından ComfyUI `http://127.0.0.1:8188`, Blender 5.1 yolu ve istersen arşiv klasörünü belirle. Ayarlar istasyon bazlı tutulur.
5. AI Generate: referans PNG seç, Pixal3D pilotunu seç, Game preset ve Generate. Ardından Auto archive + GLB viewer. Var olan tilki GLB'sini yeniden üretmeden test etmek için `GLB İçe Aktar` kullanılabilir.

### Tasarım ve güvenlik

- `studio/src/lib/workflow.ts` API adaptörü `/object_info` metadata'sından node input şemasını öğrenir, sadece Save3DAdvanced'e bağlı node'ları kuyruklar. Eksik node/input varsa `/prompt` öncesinde durur. Pinned UI grafı asla değiştirilmez.
- Rust backend (`src-tauri/src/comfy.rs`) HTTP arayüzüyle üretimi başlatır; websocket yerine ilk M1'de `history` + `queue` polling ile takip eder. API hatası açıkça gösterilir.
- Görseller `/upload/image` ile çakışmayan benzersiz adla yüklenir (`overwrite=false`). Model cache'e müdahale edilmez.
- Arşiv SQLite + immutable `original.glb` + manifest + generation JSON kullanır. Aynı modele ait LOD varyantları ayrı alt klasörde tutulur.
- `read_asset_glb` yalnız arşive kaydedilmiş UUID asset ID ile çalışır. GLB boyut sınırı vardır. Viewer yüklenmiş GLB'yi memory üzerinden işler, güvenilmeyen model uzantılarını doğrudan internete açmaz.
- Blender arka planda `-b` ile açılır; `blender/prepare_lods.py` sürümü uygulama ile gelir. Kaynak GLB değiştirilmez; normal/AO yeniden bake **henüz yoktur**.
- GPU telemetri `nvidia-smi` çıktısıdır ve **tüm GPU kullanımını** gösterir, sadece ComfyUI kullanımını değil.
- `Texture Lab`, `AI Image` ve `Unity Export` modülleri arayüzde kapalıdır; API altyapısı ve modül kayıt sistemi genişletilebilir. Uygulama bu modüller hazırmış gibi davranmaz.
- Haricî ComfyUI adresi kullanıcı tarafından açıkça girilir. LAN erişimi kullanılacaksa ağ güvenliği ve kimlik doğrulama sorumluluğu ayrıca değerlendirilmeli; varsayılan loopback'tir.

## Kaynak ağacı

- `studio/src/App.tsx`: masaüstü çalışma alanı
- `studio/src/components/ModelViewport.tsx`: Three.js / glTF PBR viewer
- `studio/src/lib/bridge.ts`: Tauri IPC tipleri ve dosya seçiciler
- `studio/src/lib/workflow.ts`: sürüm uyumlu ComfyUI UI → API graph adaptörü
- `studio/src/lib/modules.ts`: modül manifestleri ve gelecekteki genişletme noktaları
- `studio/src-tauri/src/comfy.rs`: HTTP / upload / queue / download
- `studio/src-tauri/src/storage.rs`: SQLite/immutable asset archive + GLB manifest
- `studio/src-tauri/src/blender.rs`: blender.exe algılama, background LOD, log
- `studio/docs/ARCHITECTURE.md`: daha geniş uygulama hedefleri ve karar kapıları

## Testler

- `cd studio && npm run test:adapter`: saf TypeScript adapter testleri + gerçek pilot JSON smoke testi, Node.js 22 gerekir.
- `cd studio && npm run build`: TypeScript ve Vite (npm bağımlılıkları yüklüyse).
- `cd studio/src-tauri && cargo check && cargo test`: Windows/Rust ortamında backend doğrulaması.
- **Tam E2E kabul henüz bekleniyor:** Windows Tauri derlemesi, /object_info uyumu, /upload/image, /prompt, /history, GLB dosyası, Blender LOD; bu testler yapılana kadar 'uygulama tam çalışıyor' denmeyecek.

## M1.1 (14 Ekim 2026) — ComfyUI API JSON ve geniş 3D viewport

- ComfyUI API Export artık üretimin asıl kaynağıdır; UI/LiteGraph -> API dönüşümünde `Save3DAdvanced.viewport_state` kaybı önlenir.
- `ComfyUI API JSON İçe Aktar` butonu ile aynı Pixal3D/TRELLIS.2 grafının ComfyUI'den kaydedilmiş API formatını seçebilirsin. Dosya kopyalanır, orijinal değişmez.
- İşlem bitip GLB oluşmazsa `Tanı Kaydı Görüntüle` hata veren ComfyUI node'unu raporlar.
- Inspector toolbar'daki genişlet simgesi 3D görünümü tam alana açar; başlık ve aktivite çubuğu daraltıldı.
- RTX 5080 istasyonunda ComfyUI `http://127.0.0.1:8191` adresinde çalışıyor; GPU raporundaki eski `4070ti-super` istasyon etiketini Settings'te `5080` yap.
- Eski `06_Diagnose-ComfyUI-Job.ps1` salt okunur `/history` araştırması yapabilir.

Ayrıntılı kayıt: `docs/AI3D_STUDIO_M1_1_HOTFIX_2026-10-14.md`.

## M1.2 — Kaynak workflow sadakati ve uygulama kullanımı

Önceki M1.1 sürümü kullanıcı API export grafındaki tüm KSampler seed'lerini tek bir değerle değiştiriyordu; Pixal3D ve TRELLIS.2 sonuçlarının bozulmasına sebep olabilecek bu davranış kaldırıldı. **Varsayılan seçim `Workflow aynen`**: yüklenen görsel adı dışında hiçbir kaynak ayara dokunmadan tam API JSON'u ComfyUI'ye gönderir. Bu mod 3D üretim süresi/kalitesini ComfyUI UI koşusuyla karşılaştırmak içindir; gerçek üretim hâlâ PC üzerinde test gerektirir.

`Özel ayarlar` modunda triangle/bake/texture ayarları kullanılır. Seed'ler ancak ayrı `Tüm KSampler seed'lerini değiştir` seçeneğiyle değişir. Üretim raporunda değişen alanlar kayıtlıdır; backend izin verilmeyen graph değişikliklerini engeller.

Referans seçimi artık PNG/JPEG/WEBP görsel thumbnail'ı gösterir. Arşivdeki model `Arşivden kaldır` ile **çöp arşivine** taşınabilir, orijinal kullanıcı dosyası silinmez. 3D görüntüleyici için genişletme simgesi ve küçültülmüş kontrol panelleri vardır.

Test: `npm run test:adapter`, `npm run build`, `cargo test --lib`. Gerçek RTX 5080/ComfyUI karşılaştırması tamamlanmadan tam düzeldi kabul edilmez.

Bkz: `docs/AI3D_STUDIO_M1_2_FIDELITY_UX_2026-10-14.md`.

## M1.3 — Baştan düzenlenmiş çalışma alanı

- Üç sütun şablonunun odak modunda aktif kalmasına neden olan CSS çakışması temizlendi. Görüntüleyici tam genişlikte açılır; Esc veya aynı düğmeyle eski düzene döner.
- Sol ve sağ paneller ayrı ayrı açılıp gizlenebilir. Referans görsel seçici küçültülerek 3D görüntüleme alanı büyütüldü.
- Üret butonu ayarlar uzun olsa da sabit görünür. Her panel bağımsız kaydırılır; GPU telemetrisi inspector altında sabit durur.
- Üst başlık ve iş takip kartı sadeleştirildi; referans ve arşiv kontrolleri korundu.
- Gerçek React arayüzünü farklı pencere boyutlarında test eden Playwright kontrolleri eklendi (`npm run test:ui`). CI Windows üzerinde React/Vite ve Rust kontrolleriyle birlikte çalışır.
- **Workflow aynen** üretim yolu, model seed'leri, ComfyUI ayarları, arşiv ve Blender pipeline değişmedi. UI kontrolü için tekrar GPU üretimi yapılmasına gerek yok.

## M2 — PBR Texture Inspector (ilk aşama)

Mevcut varlığı **Model Arşivi** üzerinden seçtikten sonra sağdaki **Model Bilgileri → PBR Texture Inspector** bölümünde GLB'ye gömülü gerçek PBR haritaları açılır. Base Color ve Normal orijinal renkli görünür; Roughness (G), Metallic (B), AO (R) packed haritadan gri kanal olarak ayrılır. Kartı tıklayarak haritayı büyük açabilir, **Packed orijinali göster** seçeneğiyle karşılaştırabilirsin. Birden fazla materyal varsa materyal seçici görüntülenir.

Bu aşamada texture düzeltme, yeniden bake veya sıkıştırma yapmıyoruz; **kaynak GLB her zaman korunur**. Referans üretim/ComfyUI workflow davranışı, Blender LOD ve M1.3 tam ekran görünümü değiştirilmedi. Kontrol adımları `docs/TEXTURE_INSPECTOR.md` içinde.

## M2.1 — PBR Texture Export / Optimize

Kullanıcı RTX 5080'de GLB'den ayrıştırılan tüm PBR map'lerin doğru açıldığını doğruladı.
**Model Bilgileri → PBR Texture Inspector → Texture Export** altında seçili materyal
haritaları ayrı PNG dosyaları olarak kaydedilebilir. Kaynak / 1024 / 512 / 256 px
uzun kenar profilleri vardır. Roughness (G), Metallic (B), AO (R) ayrı gri kanal;
Base Color ve Normal ayrı renkli PNG olarak dışa aktarılır.

Her işlem, Studio'nun **immutable orijinal GLB'sini değiştirmeden** yeni sürümlü
`texture_exports` klasörüne kaydedilir. Manifest PNG SHA256, kaynak GLB SHA256,
çözünürlük, materyal ve kaynak kanal bilgisini içerir. Son ihracatlar arayüzden
izlenebilir. LOD önizlemesindeyken export kapalıdır; SRC seçilmelidir.

**GPU sıkıştırma ve Unity otomatik materyal ataması bu aşamada yapılmaz.** Ayrıntılar:
`docs/TEXTURE_EXPORT_M2_1.md`. Gerçek Windows/RTX 5080 çalıştırma testi beklemektedir.
