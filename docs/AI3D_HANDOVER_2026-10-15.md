# Mallfunction AI3D Studio — Yeni Sohbete Proje Devri
**Tarih:** 15 Ekim 2026  
**Devir türü:** Teknik/kullanıcı kararı + doğrulama durumu + ilk uygulama planı.  
**Kapsam:** M2.2 Unity staging sonrasında **Unity Native One-Click URP/HDRP** üretimine geçiş.

> **Bunu yeni sohbetin başlangıç kaynağı kabul et:** Açık istek, kod değişikliklerine hemen başlamadan mevcut branch/CI durumunu yeniden teyit etmek; ardından *önce* CI sorunu, *sonra* Unity-ready prefab aktarımı. Tekrar ihtiyaç dışı plan tartışması, ek AI GPU üretimi veya kozmetik UI değişikliği yapılmasın.

## A. Belgeler (önem sırası)

1. [Güncel durum raporu](./AI3D_STATUS_REPORT_2026-10-15.md) — *gerçek ölçüm vs yazılmış kod vs CI* ayrımı.
2. [Yol haritası / kilometre taşları](./AI3D_ROADMAP_2026-10-15.md) — P0 Unity URP/HDRP one-click önceliği.
3. [Unity URP/HDRP teknik şartname](./AI3D_UNITY_URP_HDRP_EXPORT_SPEC_2026-10-15.md) — FBX, packed mask, Unity import scripti, prefab, tests.
4. [M2.2 önceki staging belgesi](./AI3D_STUDIO_M2_2_UNITY_GAME_EXPORT_2026-10-15.md) — uygulanmış eski URP staging; **hedeflenen final Unity export DEĞİL**.
5. [İlk gerçek PLAYBOX sonuçları](./AI3D_PLAYBOX_FOX_FIRST_RUN_2026-10-14.md).
6. [İstasyonlar](./WORKSTATIONS.md) ve `studio/docs/ARCHITECTURE.md`.

## B. GitHub/çalışma dalı/başlangıç noktası

- Repo: https://github.com/progesor/MallfunctionAI3D
- Aktif geliştirme dalı: **`feat/desktop-studio-foundation`**
- PR: https://github.com/progesor/MallfunctionAI3D/pull/1 (**DRAFT, main'e merge yok**)
- **Kod incelemesi yapılan HEAD (belge commitlerinden önce):** `09307179be07647cddd9c3d9ee5c21abf8b03139`.
- M2.2 URP staging commit'i: `906f5b8a642e`; son kod düzeltmesi `09307179be07`.
- Son tam yeşil CI: `ca062066` / https://github.com/progesor/MallfunctionAI3D/actions/runs/38633128833 (M2.1).
- **Son incelenen M2.2 CI kırmızı:** https://github.com/progesor/MallfunctionAI3D/actions/runs/38644764542 — HEAD `09307179`; kod, Rust build ve 16 Rust testi PASS, Playwright browser testleri sorunlu.
- Belgeler bu kod SHA'sından sonraki commitlerde eklendi; yeni sohbette `origin/feat/desktop-studio-foundation` güncel HEAD yeniden alınmalı.

**Yerel PowerShell:**
```powershell
cd C:\Works\MallfunctionAI3D
git status
git fetch origin
git switch feat/desktop-studio-foundation
git pull --ff-only
powershell -NoProfile -ExecutionPolicy Bypass -File .\05_Start-Studio.ps1
```
`git status` kirli dal gösteriyorsa değişiklikleri asla silme; önce kullanıcıya dosyaları koruyacak yöntem öner. Klasör veya branch değişiklikleri onaysız destructive yapma.

## C. Mevcut ürün ve kullanıcı tarafından onaylananlar

- Tauri 2, Rust backend, React/Three.js, SQLite asset arşivi, GLB PBR görüntüleme, Blender 5.1+ LOD işleme var.
- ComfyUI Desktop Pixal3D üretimi Studio'da **“Workflow aynen”** düzeltmesi sonrasında çalışıyor (kullanıcının bildirdiği doğrulama). API JSON kaynaktır; `KSampler` seed'lerini sessizce tek değere çevirme.
- M1.3 workbench UI geniş mod ve paneller düzeltildi; kullanıcı kabul edip devam etti.
- GLB içindeki BaseColor/Normal/Metallic/Roughness/AO haritaları ve packed texture önizlemesi kullanıcı tarafından doğru görüldü.
- M2.1 Texture Export (orijinal/1024/512/256 çözünürlük, 5 map, manifest/arşiv sürümleri) kullanıcı tarafından “her şey yolunda” olarak onaylandı.
- Kullanıcı **büyük batch istemiyor**; çoğunlukla tek model, kontrollü üretim istiyor. RTX 4070/5080 GPU'ların %100 kullanımı tek başına sorun değildir. Kaynak dosyalar immutable olsun.
- AI görsel üretimi ileride eklenebilecek **ayrı modül**; şimdi öncelik Unity.
- Önceki Blender LOD scriptinde hard-coded Fox adları vardı; `blender/prepare_lods.py` artık `--name` ile genel varlık ismini kabul ediyor.

## D. Çalışan iki istasyon

| | A | B (son kullanılan) |
|---|---|---|
| GPU | RTX 4070 Ti SUPER, 16 GB | RTX 5080, 16 GB |
| ComfyUI | Desktop 0.40.0 | Desktop 0.40.0 |
| API | `http://127.0.0.1:8188` | `http://127.0.0.1:8191` |
| Modeller/nodes | 7/7, 43/43 | 7/7, 43/43 |
| ComfyUI output | Desktop shared dir | `F:\ComfyUI\output` |
| GPU ölçüm peak | 15.014 MiB | 12.696 MiB |
| GPU log caveat | GPU genelindeki kullanım | RTX 5080 raporunda station etiketi yanlışlıkla 4070 olabilir |

Unity Editor executable path ve hedef Unity project path henüz bu devirde **kesin doğrulanmadı**. Kullanıcıdan yalnız gerekirse sorulsun; otomatik algılama ilk hedef. Unity sürüm ve URP/HDRP aktif pipeline `ProjectSettings/ProjectVersion.txt`, `Packages/manifest.json` ve aktif GraphicsSettings üzerinden belirlensin.

## E. İlk test varlığı / karşılaştırma

**PLAYBOX Fox Plush**:
- Referans SHA256: `e77015dcad28f82a3ec7680ea0282a4cb33c1beb6a6071e5e50a6a33be4064e6`.
- İlk Pixal3D GLB SHA256: `9447201e9522788b70b3c8580ffba2ac515042a23defe6a104c207a24e176388`.
- 7.998 tris / 6.252 vertex / 1 material / 3 embedded images / 1024 PBR maps.
- Blender LOD0/1/2: **7998 / 5000 / 3000 tris**.
- Bacak/ayak deformasyonu ve bazı UV texture seam izleri kullanıcının bu ilk pilot için kabul ettiği kusurlardır; AI modeli gereksizce yeniden üretme.
- M2.2 geometry-only 3 LOD GLB toplamı **553.296 byte** (önceki gömülü PBR'li 3 GLB **9.379.404 byte**); ortak texture seti ayrıca yer kaplar. Bunlar **Unity prefab var** anlamına gelmiyor.

## F. Kod haritası — değiştirmeye nereden başlanmalı?

| Dosya | Görevi | Değişim ilkesi |
|---|---|---|
| `studio/src-tauri/src/unity_export.rs` | Mevcut URP staging, geometri-only GLB, eski texture map paketi | Unity exporter'ın çekirdeği; mask, FBX manifest ve profil eklenecek |
| `studio/src/components/UnityExportPanel.tsx` | Studio'da URP paket seçimi | Auto/URP/HDRP, hedef proje, import sonucu, status göster |
| `studio/src/lib/bridge.ts` | Tauri invoke tipleri ve API | Yeni export profilleri/status command ekle |
| `studio/src-tauri/src/lib.rs` | Rust Tauri command registration | Unity command ve importer bridge ekle |
| `studio/src-tauri/src/blender.rs` + `blender/prepare_lods.py` | Blender CLI LOD üretimi | FBX export işçisi, genel varlık adı, axis/pivot/scale |
| `studio/src-tauri/src/texture_export.rs` | 5 PBR map sürümü, SHA, güvenli owned path | Mask bake girdisi; kaynak üzerinde değişiklik yok |
| `studio/src-tauri/src/storage.rs` | SQLite, varlık id/sürüm | Unity export version/history ve import status |
| `studio/src/lib/api-workflow.ts` ve `src-tauri/src/comfy.rs` | ComfyUI üretim sadakati | **Şimdilik dokunma**, regresyon testlerini koru |
| `studio/e2e/texture-inspector.spec.ts` | CI'deki başarısız texture export testi | İlk CI işlerinden biri |
| `studio/e2e/unity-export.spec.ts` | Mevcut staging UI testi | Yeni one-click kabul senaryosu ekle |
| `studio/e2e/workbench.spec.ts` | Full-width layout testleri | Mevcut UI güvence testi |
| `.github/workflows/studio-ci.yml` | Windows TS/Vite/Rust/Playwright CI | Flaky neden analizi; test kaldırmadan onar |
| `studio/src-tauri/Cargo.toml` | M2.2 `0.2.2` | Unity kabulü sonrası sürüm yükseltme |

**Yeni modül önerisi:** `unity/Packages/com.progesor.ai3d.importer/` (Unity Editor C# kodu) + Blender'da `export_unity_fbx.py` + Rust `unity_project.rs` (proje/sürüm/pipeline/izin kontrolü). Bunlar şu anda **mevcut değildir**.

## G. Kırmızı CI'nin somut nedeni

**Son incelediğimiz run:** `38644764542`.

- Windows typecheck ve Vite build **PASS**; Rust `cargo check` **PASS**; Rust 16 unit **PASS**.
- Playwright: 9 temel test; retry'larla `6 passed, 1 failed, 2 flaky`.
- **Kalıcı hata:** `e2e/texture-inspector.spec.ts:66` — `getByText('5 PNG kaydedildi')` 30 s test timeout (bir retry de başarısız).
- `e2e/unity-export.spec.ts:63`: `unity-export-success` bekleme ilk deneme timeout; retry PASS.
- `e2e/workbench.spec.ts:17` 1480×920 geniş mod ilk deneme timeout; retry PASS.
- Trace/screenshot CI artifact'lerini incele; gerekirse WebGL/software render, ağır canvas PNG encode, React state/event scheduling ölç. Test “pass” görünene kadar prod kodda ihtiyaç dışı geliştirmeyi durdur.
- Bug kesin olarak “Rust channel pack formülü bozuk” diye nitelendirilemez; kanallar Rust 16 testte geçiyor.

## H. M2.2'nin final değil staging olmasının sebebi

M2.2 çıktıları:
```text
<StudioArchive>/assets/<id>/unity_exports/urp_DATE_UUID/
  Meshes/LOD0.glb
  Meshes/LOD1.glb
  Meshes/LOD2.glb
  Textures/BaseColor.png
  Textures/Normal.png
  Textures/MetallicSmoothness.png
  Textures/Occlusion.png
  unity_asset_manifest.json
  UNITY_URP_SETUP.txt
```
Bu çıktılarda **FBX, Unity-native Material, Unity HDRP MaskMap, LODGroup, prefab ve gerçek `.unitypackage` yok**. Unity'de GLB için ayrı importer ve elle materyal/LOD kurulumu gerekir; tam hedefi karşılamaz.

## I. Yeni sohbetin ilk geliştirme siparişi — P0

**Birinci:** Kırmızı CI/texture browser timeout ve flaky testler; gereksiz refactor yok.  
**İkinci:** `Unity Target` (Editor/Project auto detect, sürüm ve render pipeline) + Blender FBX (LOD hierarchy, normals/UV, pivot/scale).  
**Üçüncü:** URP & HDRP doğru **packed RGBA mask** üretimi:
- **URP Lit:** `R=metal, G=AO, B=unused, A=smoothness`; aynı map Metallic ve Occlusion slotuna (stok URP Lit'te otomatik `_MaskMap` varsayma).
- **HDRP Lit:** `R=metal, G=AO, B=detail mask, A=smoothness`; HDRP `_MaskMap` slotuna.
- `smoothness=1-roughness`; kaynak glTF roughness/metal factor doğrula; sRGB packed map'de kapalı.
**Dördüncü:** Unity Editor plugin/importer bir kez kurulur; FBX+map → **tek ortak materyal + 3 LOD renderer + LODGroup + 1 basit collider + `.prefab`** otomatik. Opsiyonel `.unitypackage` Unity Editor API ile.
**Beşinci:** URP ve HDRP ayrı gerçek Unity test projelerinde Fox varlığını import et, prefabı sahneye sürükle, hiçbir elle map/LOD materyal kurma gerektirmediğini doğrula. Çıktı/rapor/CI yeşil olsun.

Sırf exporter kodu yazıldı diye **“Unity-ready” ilan etme**: kullanıcı için kabul, Unity projesinde çalışan prefabdır.

## J. Risk / güvenlik / karar kuralları

- Her işlem immutable source + yeni version klasörü; asla sessiz overwrite.
- Studio export menüsünden Unity hedefi seçilmeden başka projeye yazma yok. Kullanıcı başka oyunun reposuna dokunma izni vermemiş kabul et; yalnız seçtiği projede belirli güvenli klasöre yaz.
- `Unity AssetDatabase`/Importer projesine sürüm/pipeline uyumluluğu doğrulanmadan tehlikeli değişiklik yapma.
- Aynı GLB çok materyal içeriyorsa eksik destekleme yerine açık reddetme; multi-material sonraki sürüm.
- Importer, kaynak shader ve normal yeşil kanalın doğru olduğunu görsel olarak doğrulamalı; Unity'de farklı HDRP/URP materyal parametrelerini birbirinin yerine kullanma.
- 100/1000 instance Profiler testi Unity kabulünün ileriki alt adımı; 60–70K sahne objesi için otomatik ağır test çalıştırma.
- Yerel geliştirmede `main` sabit; mevcut feature branch / Draft PR korunmalı, merge kullanıcı onayıyla yapılmalı.
- Yeni kullanıcı girdisi olmadıkça soruları en aza indir, doğal varsayımlarla uygula; ama **hedef Unity proje kökü ve mevcut pipeline güvenle bulunamazsa bir kısa soru** sorulabilir.

## K. Yeni sohbete yapıştırılabilir başlangıç talimatı

```text
MallfunctionAI3D Studio projesine devam edelim.
Devir kaynağı:
docs/AI3D_HANDOVER_2026-10-15.md
docs/AI3D_STATUS_REPORT_2026-10-15.md
docs/AI3D_ROADMAP_2026-10-15.md
docs/AI3D_UNITY_URP_HDRP_EXPORT_SPEC_2026-10-15.md
Repo: https://github.com/progesor/MallfunctionAI3D
Aktif dal: feat/desktop-studio-foundation; Draft PR #1.

Öncelik P0: M2.2 son kırmızı Windows CI / Playwright regresyonunu düzelt.
Ardından kaynak GLB'yi koruyarak Blender FBX + URP/HDRP doğru packed Mask Map
+ Unity Editor importer ile otomatik .mat, 3 LOD/LODGroup, collider ve .prefab
oluşturan tek tık Unity export'a geç. Opsiyonel .unitypackage üretimini de planla.
URP stok Lit R metal/G AO/A smoothness (metal ve occlusion slotları);
HDRP Lit MaskMap R metal/G AO/B detail/A smoothness.
UI/UX ve çalışan ComfyUI 'Workflow aynen' üretimini bozma.
Yeni model üretimine/batch'e veya AI image modülüne sapma.
Yaptığın geliştirmeleri commit et, test ve gerçek Unity kabul sonuçlarını bildir.
```

## L. Birinci kabul oturumunda üretilecek kanıtlar

- Yeni branch HEAD + CI URL (tam yeşil).
- Unity proje version + pipeline raporu (URP ve HDRP).
- FBX(s) üç LOD triangle/UV/normals/axis test sonucu.
- URP/HDRP PNG mask kanal RGBA golden testleri, source SHA korunumu.
- Unity Importer için `.mat`, `.prefab`, `LODGroup`, collider, shared material test sonucu.
- Unity sahne karşılaştırma (PBR ve LOD0/1/2) ve kaydedip yeniden açma.
- Eğer `.unitypackage` üretildiyse boş projede import testi.
- Sadece bu kapılar geçerse Unity modülüne “oyunda kullanılabilir” etiketi.

**Özet:** 15 Ekim devrinin merkezinde **Unity'ye neredeyse sıfır manuel işle doğrudan kullanılır asset üretmek** var. Üretim motoru ve PBR Inspector çalışıyor; işimizin asıl eksiği şu anda Unity teslimi ve son CI regresyonu.
