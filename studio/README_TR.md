# Mallfunction AI3D Studio — M1 Desktop Foundation

**Durum (2026-10-14):** Kaynak kod ve otomatik adapter testleri hazır. Resmî Pixal3D/TRELLIS.2 workflow'ları çalışan uygulamaya bağlandı; **Windows üzerindeki gerçek Tauri derlemesi, ComfyUI API kuyruğu ve Blender entegrasyonu henüz uçtan uca doğrulanmadı.** Bu aşama kurulum/entegrasyon testi bekleyen ilk geliştirici sürümüdür, production release değildir.

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

## M1.1 hotfix (2026-10-14): API-export, hata tanısı, büyük viewport

- Pixal3D/TRELLIS.2 üretiminin kaynağı artık **ComfyUI API Export JSON**; varsayılan kaynak kullanıcıdan doğrulanan 63 node'lu gerçek `Save3DAdvanced #322` grafıdır.
- `ComfyUI API JSON İçe Aktar` düğmesinden kendi API JSON'unu ekle. `LoadImage`, `Save3DAdvanced` ve bağlantılı şekil/doku zinciri korunur. Üretim esnasında UI -> API yeniden derlemesi yapılmaz.
- `Save3DAdvanced.viewport_state` nesne olarak korunur. Sadece maske üretimlerini reddetmek ve hata düğümünü göstermek için doğrulama/tanı eklendi.
- ComfyUI Desktop RTX 5080 için raporlanan adres: `http://127.0.0.1:8191`. Settings -> Station etiketi `5080` olabilir.
- Eski başarısız ComfyUI geçmişini yeni üretim yapmadan incelemek için kök klasörde `06_Diagnose-ComfyUI-Job.ps1` kullan:
  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File .\06_Diagnose-ComfyUI-Job.ps1 -ComfyUrl "http://127.0.0.1:8191"
  ```
  `reports/ComfyHistory_*.json` oluşur. Bu dosyada yerel yollar ve kaynak görsel adları olabilir; herkese açık GitHub'a yükleme.
- Üstteki gereksiz büyük başlık küçültüldü; GLB/3D Inspector görüntüleyici daha büyük ve tam genişlik moduna geçebiliyor.
- Yeni CI testi: API-export regression, Windows Rust birim testleri. Gerçek RTX 5080 üretim kabulü henüz bu düzeltmeyle yapılmadı.

Ayrıntılar: `docs/AI3D_STUDIO_M1_1_HOTFIX_2026-10-14.md`.
