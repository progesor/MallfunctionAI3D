# Mallfunction AI3D Studio — Ayrıntılı Durum Raporu
**Rapor tarihi:** 15 Ekim 2026 · Türkiye saati  
**Temel kod anlık görüntüsü:** \`feat/desktop-studio-foundation\` · \`09307179be07647cddd9c3d9ee5c21abf8b03139\` (**belgeler commitlenmeden önceki HEAD**)  
**Açık PR:** [#1 — Draft: M1 Modüler AI3D Studio masaüstü uygulaması](https://github.com/progesor/MallfunctionAI3D/pull/1) → \`main\`. **Henüz merge edilmedi.**  
**Rapor yaklaşımı:** Aşağıda kullanıcı tarafından gerçek istasyonda doğrulananlar, otomatik testlerle doğrulananlar ve yalnızca yazılmış olan kodlar özellikle ayrı gösteriliyor.

## 1. Projenin amacı ve kararlar

Mallfunction'ın çok sayıda tekrar eden, performans duyarlı ürün/prop varlığı için yerel, ağırlıklı olarak tekli üretime odaklanan bir 3D asset production uygulaması geliştiriliyor. Son ürün; referans görsel → ComfyUI Pixal3D/TRELLIS.2 → PBR/GLB → yerel asset arşivi → Blender LOD → oyun/Unity paketi iş akışını uygulama içinden yönetmeli.

Kesinleşmiş kullanıcı tercihleri:
- **Tekil ve kontrollü üretim ana yol.** Otomatik büyük batch zorunlu değil; kullanıcı gerektiğinde küçük seri çalıştırabilir.
- **Çalışan ComfyUI Desktop ve mevcut model ağırlıkları değiştirilmez.** İlk başarılı workflow birebir saklanır, kullanıcı açıkça seçmedikçe seed/kalite değerleri değiştirilmez.
- Kaynak PNG/GLB, ham LOD, texture, koşu kaydı ve sürümler saklanır; orijinaller asla sessizce ezilmez.
- Mevcut iki Windows geliştirme istasyonu desteklenir. Yerel arşiv/sürüm/bağlantılar ayrı izlenir.
- Uygulama **modüler** kalır; ComfyUI görsel üretimi, gelişmiş texture optimizasyonu, başka 3D motorlar ve optional batch ileride eklenebilir.
- **Yeni ve en yüksek öncelik:** Unity'ye neredeyse manuel işlem gerektirmeyen, **URP ve HDRP profilli**, otomatik mask map + materyal + LODGroup + prefab içeren gerçek aktarım.
- Son kullanıcı onayı: Studio'da AI üretimi düzeltildi; 3D Inspector, Blender LOD ve Texture Inspector ile Texture Export işlevleri denemelerde sorunsuz bildirildi.

## 2. Repo, teknoloji ve çalışma alanı

| Öğe | Mevcut durum |
|---|---|
| GitHub | https://github.com/progesor/MallfunctionAI3D |
| Aktif dal | \`feat/desktop-studio-foundation\` |
| Ana dal | \`main\`; mevcut PR taslak |
| Studio | Tauri 2 + React/TypeScript + Three.js + Vite |
| Yerel backend | Rust + SQLite (rusqlite) + dosya arşivi |
| Üretim | ComfyUI HTTP API; API-format workflow, kuyruk/history, GLB indirme |
| İşleme | Blender arka plan işçisi ve \`blender/prepare_lods.py\` |
| PBR harita işlemleri | TypeScript görüntü/kanal ayırma; Rust arşivli PNG export |
| Unity | **M2.2 URP staging kodu var; Unity Editor otomatik prefab importu yok** |
| Mevcut sürüm numarası | \`studio/src-tauri/Cargo.toml\` → **0.2.2** |
| Çalıştırma | \`powershell -NoProfile -ExecutionPolicy Bypass -File .\05_Start-Studio.ps1\` |

Ana dosyalar: \`studio/src/App.tsx\`, \`studio/src/components/ModelViewport.tsx\`, \`TextureInspector.tsx\`, \`UnityExportPanel.tsx\`, \`studio/src/lib/api-workflow.ts\`, \`src-tauri/src/comfy.rs\`, \`storage.rs\`, \`texture_export.rs\`, \`unity_export.rs\`, \`blender.rs\`, \`studio/e2e/*\`.

## 3. İki istasyon ve ölçülen kaynaklar

| Konu | İstasyon A | İstasyon B |
|---|---|---|
| GPU | RTX 4070 Ti SUPER | RTX 5080 |
| VRAM | 16 GB | 16 GB |
| ComfyUI | Desktop 0.40.0 | Desktop 0.40.0 |
| Bağlantı | \`http://127.0.0.1:8188\` | \`http://127.0.0.1:8191\` |
| ComfyUI output | Desktop shared output klasörü | \`F:\ComfyUI\output\` (audit) |
| Model isimleri | 7/7 | 7/7 |
| Gerekli node türleri | 43/43 | 43/43 |
| GPU peak, ilgili ölçüm | 15.014 MiB | 12.696 MiB |
| Kuyruk etkinlik penceresi | Yaklaşık 68 sn | Yaklaşık 96 sn |
| Açıklama | İlk Fox Pixal3D GLB/PBR üretimi gerçekten doğrulandı | ComfyUI/GPU ölçümü gerçek; Studio üretim/Inspector/Export için kullanıcının başarılı deneyim bildirimi var |

GPU peak'leri **GPU genelinin** ölçümüdür, yalnız ComfyUI süreci değildir. RTX 5080 JSON içindeki \`station: 4070ti-super\` yanlış etiketlenmişti; donanım alanında RTX 5080 görünüyor. İki koşunun sürelerini birebir model benchmark'ı olarak sunmayın.

## 4. İlk test asset: PLAYBOX Fox Plush

**Orijinal referans:** \`Meshy_AI_image_0860363f-5fd5-4b9b-a2f3-8ae99c9fd3b4_0.png\`; SHA256 \`e77015dcad28f82a3ec7680ea0282a4cb33c1beb6a6071e5e50a6a33be4064e6\`.  
**Pixal3D GLB:** \`PLAYBOX_FoxPlush_pixal3d_pilot_v01_00001.glb\`; SHA256 \`9447201e9522788b70b3c8580ffba2ac515042a23defe6a104c207a24e176388\`.

Doğrulanmış:
- **7.998 triangle**, **6.252 vertex**, 1 mesh, 1 PBR materyal, UV0/normaller mevcut.
- 1024×1024 Base Color, metal/roughness, normal ve AO bilgisi; **3 gömülü PNG** ile 5 PBR kanal görünümü.
- Dosya **3.339.600 byte**; kaynak model korunarak LOD0/LOD1/LOD2 elde edildi (**7.998 / 5.000 / 3.000 tri**).
- Pelüşte ayak/gövde birleşiminde yerel deformasyon ve texture/bake izleri var; kullanıcı ilk prototip için kabul etti.
- UV dikişlerinde vertex çoğalması sebebiyle indeks bazlı 168 ada tespiti yanılgı yaratır. Yalnız aynı konumlu noktaların **analiz kopyasında** birleştirildiği incelemede 2 geometrik kabuk saptandı. Orijinale otomatik weld uygulanmadı.
- Üç LOD'un ayrı tam dokulu GLB toplamı **9.379.404 byte**; M2.2 geometri-only prototipinde **553.296 byte** (~%94.1 daha küçük **yalnız mesh dosyaları**). Shared texture'ların dosya boyu bu kıyasa dahil değildir.

## 5. Tamamlanan kilometre taşları

| Aşama | İçerik | Kanıt düzeyi |
|---|---|---|
| İlk araştırma/pilot | Pixal3D, TRELLIS.2 native grafik, 16 GB preset ve ComfyUI audit | Gerçek Desktop audit + Pixal3D test |
| M1 Studio | Masaüstü UI, ComfyUI API, SQLite arşiv, GLB, Blender LOD | Kullanıcının gerçek üretim/LOD bildirimi |
| M1.1 | API JSON import, tanı kaydı | Testler; sonraki gerçek üretimde M1.2 gerekli oldu |
| M1.2 | **Workflow aynen** modu; ayrı KSampler seed'leri/ayarları korunuyor | Kod testleri + kullanıcının “üretim düzeldi” bildirimi |
| M1.3 | 3D Inspector alanı, geniş görünüm, panel düzeni | Browser UI testleri ve kullanıcı onayı |
| M2 | PBR Texture Inspector, gerçek GLB kanalları | Gerçek Fox GLB yapısal testi + kullanıcı onayı |
| M2.1 | 5 map PNG export, 1024/512/256 vb., arşiv versiyonlama | 2026-10-14 başarılı Windows CI + kullanıcı “her şey yolunda” onayı |
| M2.2 | **URP staging export**, geometry-only GLB ve ortak texture'lar | Rust birim testleri PASS; **tam CI FAIL / gerçek Unity kabulü yok** |

## 6. M2.2'nin tam olarak yaptığı / yapmadığı

**Şu an kodda mevcut:** \`studio/src-tauri/src/unity_export.rs\` ve \`studio/src/components/UnityExportPanel.tsx\`. Yalnız tek materyalli statik GLB için daha önce oluşturulmuş 3 Blender LOD ve 5 ayrı PBR PNG sürümü gerekir. Sonuç klasörü \`.../assets/<id>/unity_exports/urp_DATE_UUID/\` içinde:
- \`Meshes/LOD0.glb\`, \`LOD1.glb\`, \`LOD2.glb\`: gömülü dokuları çıkarılmış mesh (hala GLB).
- \`Textures/BaseColor.png\`, \`Normal.png\`, \`MetallicSmoothness.png\`, \`Occlusion.png\`.
- \`unity_asset_manifest.json\`, \`UNITY_URP_SETUP.txt\`.
- URP materyal alanlarının önerileri; orijinaller korunur.

**Henüz yok:** Unity-native FBX seçeneği, HDRP MaskMap profil ve shader otomasyonu, Unity Editor importer, otomatik \`.mat\`, \`LODGroup\`, collider, \`.prefab\`, \`.unitypackage\`, Unity proje klasörüne tek tık aktarım ve tekrar içe almada güncelleme politikası. Dolayısıyla “Unity-ready staging”, “Unity’de kullanıma hazır prefab” ile eş anlamlı değildir.

**Kod sınırları:** yalnız statik/tek materyal, opaque; skin/animation, emission, çoklu materyal, bazı glTF uzantıları ve sıkıştırmalı mesh'ler bilinçli olarak reddediliyor. Blender/Unity görsel kabul ve gerçek sahne FPS testi henüz yok.

## 7. CI durumu — yeni sohbetin ilk blokajı

**İncelenen HEAD:** \`09307179be07\`; [Actions run #38644764542](https://github.com/progesor/MallfunctionAI3D/actions/runs/38644764542) → **FAIL**.  
Aynı koşuda:
- Workflow converter / frontend unit testleri: **PASS**.
- React/Three.js TypeScript + Vite build: **PASS**.
- Windows Rust/Tauri typecheck: **PASS**.
- Rust unit: **16 PASS, 0 FAIL**.
- Playwright/browser: **6 PASS, 1 FAIL, 2 flaky**; testler tek worker'da çalıştı.
- Kalıcı görünen hata: \`e2e/texture-inspector.spec.ts\` içinde \`getByText('5 PNG kaydedildi')\` beklemesi **30 sn test timeout**, tekrar denemede de başarısız. \`unity-export.spec.ts\` başarı ekranı ve \`workbench.spec.ts\` 1480 genişleme testleri ilk denemelerde zaman aşımına uğrayıp yeniden çalıştırmada geçti.

**Yorum:** M2.2'nin Rust kodu derleniyor ve test geçiyor; ancak tam CI yeşil değil. Sebep yalnızca bir “Unity shader hatası” değil, **gerçek tarayıcı regresyonu / olası render-yük-zamanlama problemi**. Test süresini körlemesine büyütmek veya testi kaldırmak kabul yöntemi değil. Playwright trace, screenshot ve işçi kaynak tüketimi incelenmeli.

Son tam yeşil run: [#38633128833](https://github.com/progesor/MallfunctionAI3D/actions/runs/38633128833) · \`ca062066\` (M2.1).

## 8. Başlıca riskler ve sınırlamalar

1. Unity aktarımı iki pipeline için henüz doğrulanmadı; URP staging bile **GLB + manuel import**.
2. URP ve HDRP stok Lit aynı shader property isimlerini kullanmıyor. Ayrı materyal oluşturma profilleri gerekecek.
3. Kaynak \`Roughness\` → Unity \`Smoothness=1-Roughness\`; glTF AO kaynağı ile Unity AO hedef kanalının farkı doğrulanmalı.
4. Normal map OpenGL (+Y) verisinin Unity'deki import/tangent uzayı karşılığı gerçek mesh üzerinde kontrol edilmeli; gelişigüzel flip yapılmamalı.
5. LOD0/1/2 aynı texture setini paylaşmalı; modellerin pivot/ölçek, mesh normal/UV bütünlüğü ve geçişleri doğrulanmalı.
6. 60–70 bin **toplam sahne objesi**, aynı anda 70 bin render/fizik aktivitesi anlamına gelmez. Gerçek Mallfunction Profiler kabulü daha sonra gereklidir.
7. Kullanıcı ağırlıklı tekli üretim ister; batch, otomatik indirme, Unity projesinde sessiz üzerine yazma bu kapsamda olmamalı.
8. \`studio/README_TR.md\` ve bazı eski dokümanların M2.2'yi temsil etmeyen tarihsel ifadeleri var; yeni devir setini kaynak kabul ederek, sonra toplu güncelleme yapılmalı.

## 9. Sonuç ve karar

**M2.1 başarılı temel; M2.2 Unity staging uygulama kodu mevcut ama CI kırmızı ve oyun prefab teslimi tamamlanmadı.** Yeni sohbetin ilk işi:
1. M2.2 regresyonlarını/CI'yi düzelt.
2. Unity proje/pipeline bilgilerini kontrol eden importer mimarisini kur.
3. FBX veya Unity-native mesh + doğru URP/HDRP packed mask → ortak \`.mat\` + \`LODGroup\` + collider + \`.prefab\` ile **tek işlemde import** üret.
4. Testi gerçek Fox/Unity projesinde geçir; ondan sonra yeni özellik.

## Kaynaklar

- Mevcut M2.2 repo belgesi: \`docs/AI3D_STUDIO_M2_2_UNITY_GAME_EXPORT_2026-10-15.md\`
- İlk gerçek test: \`docs/AI3D_PLAYBOX_FOX_FIRST_RUN_2026-10-14.md\`
- İstasyon notu: \`docs/WORKSTATIONS.md\`
- Unity URP channel packing: https://docs.unity3d.com/6000.0/Documentation/Manual/urp/shaders-in-universalrp-channel-packed-texture.html
- Unity HDRP mask map: https://docs.unity3d.com/Packages/com.unity.render-pipelines.high-definition@17.0/manual/Mask-Map-and-Detail-Map.html
