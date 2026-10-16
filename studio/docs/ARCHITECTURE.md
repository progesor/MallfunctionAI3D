# AI3D Studio — Modüler mimari ve kararlar (M1)

## Temel akış

```
Source Reference (user-selected PNG/JPG/WEBP)
    ↓ Rust ComfyUI HTTP upload (unique filename, overwrite=false)
Versioned workflow registry (Pixal3D / TRELLIS.2)
    ↓ TypeScript version-aware UI graph adapter + live /object_info
ComfyUI /prompt → /queue + /history polling → /view GLB
    ↓ Rust validates glTF2 bytes & archives copies
SQLite catalog + immutable original.glb + manifest + run JSON
    ↓ Tauri IPC base64 (M1; use streaming protocol for large assets later)
Three.js GLTFLoader (PBR / Clay / Wireframe / Normals)
    ↓ optional Blender subprocess (background)
LOD0 / LOD1 / LOD2 + QC JSON + full log stored under asset
```

## Katman ilkeleri

**Modüller:** `src/lib/modules.ts` altında `enabled` ve `pipelineInput/Output` ile tanımlanır. UI asla etkin olmayan modülün işlemini taklit etmez. Yeni modüller `PipelineProvider` sözleşmesi üzerinden kurulacak: `image generation`, `texture processing`, `mesh processing`, `export` gibi aileler. M1'de sadece 3D workflow'ları aktiftir.

**ComfyUI adapter:** `src/lib/workflow.ts` yalnız seçili/pinned grafın bilinen ayarlarını düzenler. Node class input şeması ComfyUI'dan okunur; uyumsuz node'lar güvenli biçimde reddedilir. Basit JSON formundan tahmini node üretimi yapılmaz. Muted node'lar desteklenene kadar reddedilir. Eski scriptler ve `workflows/` yerinde kalır.

**Arşiv ve sürüm geçmişi:** `catalog.sqlite3` uygulama özel veri klasöründedir. Var olan asset silinmez, `assets/<uuid>/original.glb` immutable. Uygulama kullanıcı seçimiyle başka arşiv klasörünü yeni asset'ler için kullanabilir; eski kayıtlar absolute path içerir, otomatik dosya taşıma yapılmaz. Tek istasyonda kalıcı, iki istasyonda bağımsız arşiv (senkronizasyon ayrı karar). Filesystem kaynak yolu + SHA256 saklanır.

**Veri kaybını önleme:** Orijinal ComfyUI çıktısı ve yüklenen kullanıcının GLB'si **asla yerinde değiştirilmez**. İlk kayıt kopya, Blender çıktıları her seferinde UUID alt klasörü, eski LOD sonuçları yerinde kalır. İlk M1'de in-place texture bake yoktur.

**Blender:** Yazılım yolu kullanıcı tarafından belirtilebilir; otomatik `Program Files/Blender Foundation` algılama desteklenir. Bir işlemin başarısı sadece exit 0 ile değil, 3 GLB + JSON beklenen çıktıların varlığıyla kontrol edilir. Ayrıntılı stdout/stderr loglanır. `prepare_lods.py` UV'leri decimate sırasında korur; normal/AO rebake henüz desteklenmez.

**PBR inspector:** `GLTFLoader.parseAsync(ArrayBuffer)` ile embed texture'lı GLB. M1 modları PBR/Clay/Wireframe/Normals; Texture Lab için ayrı texture haritası inceleme, kanal packing (metallic/roughness/AO), renk uzayı, JPEG/PNG/KTX2, mipmaps, normal map yeniden bake ve kanal optimizasyonu sonraki modülde değerlendirilecek. Tek ortak materyal/texture varyantı oyun export adımında değerlendirilecek.

## Kabul sırası

M1.1: Yerel Windows Tauri derleme/görünür UI; ComfyUI 4070 istasyonu bağlantısı.
M1.2: Orijinal PLAYBOX PNG + Pixal3D Game preset ile UI'dan tek üretim, otomatik GLB yakalama ve viewer.
M1.3: Kaynak GLB import; arşiv yeniden açılışta mevcut; Blender LOD; 3 model ve rapor.
M2: Gelişmiş 3D/texture denetimi, UV seam analizi ve harita görünümleri; materyal paylaşımı ve optimize export.
M3: Oyun profile göre Blender mesh/material/texture bake, Unity prefab/LOD/scale doğrulaması.
M4: Görsel üretimi modülü, parametrik workflow registry, sınırlı queue ve istasyonlar arası arşiv senkronizasyonu.

## Bilinen M1 sınırları

- Windows/Rust derleme **henüz gerçekleştirilmedi** (tasarım geliştirme ortamında Rust cargo ve npm registry erişimi yok). Source/config doğrulaması ile gerçek Windows E2E ayrı.
- ComfyUI native `Save3DAdvanced` çıktısı `/history` altında GLB descriptor olarak bekleniyor; ilk gerçek UI'dan üretimle doğrulanmalı. Eğer ComfyUI 0.40.0 save node sadece dosyaya yazıyor ama history descriptor üretmiyorsa **sınırlı güvenli yerel çıktı watcher** ekleyeceğiz. Sabit path varsayımıyla rastgele dosya taşımıyoruz.
- Node şema uyumluluğu canlı `/object_info` gerektirir. Eğer bazı ComfyUI frontend-only widget'leri farklı adlandırılmışsa adaptör hatayı gösterir; hiçbir paket, model veya node güncellenmez.
- UI örneklerin 1–3 MB GLB boyutu için IPC base64 yeterlidir; 100 MB üstü önizleme daha sonra stream-based Tauri asset protocol ile ele alınacak.
- GPU kullanım ölçümleri anlık `nvidia-smi` genel GPU değeridir; süreç bazlı profilleme değildir.
- Görsel üretimi, shader normal rebake, Unity asset bundle ve çoklu GPU otomasyonu **M1 kapsamı dışındadır**.
