# Mallfunction AI3D Studio — Önceliklendirilmiş Yol Haritası
**Tarih:** 15 Ekim 2026  
**Yeni geliştirme yönü:** Her şeyden önce **tek tıkla Unity URP/HDRP uyumlu prefab paketleme ve içe aktarma**. AI motoru çeşitlendirme, batch, AI görsel üretimi ve diğer yenilikler sonra.

## Ürün hedefi ve başarı tanımı

Kullanıcı Studio'da arşivdeki varlığı seçer; **Export → Unity → URP veya HDRP** der; mümkünse hedef Unity proje yolunu bir kez belirtir. Sonuç: açılan Unity projesinde hazır **mesh + ortak materyal/texture + LODGroup + basit collider + prefab**. Kullanıcı materyal kanallarını elle birleştirmek, 3 LOD'u tek tek import etmek, yeni materyal yaratmak veya LOD renderer'ları elle atamak zorunda kalmaz. Son prefabı sahneye sürüklemesi yeterli olur.

Proje açık/kapalı, Unity sürümü ve pipeline bilinmiyorsa kullanıcıya anlaşılır kontrol/hata sunulur. Kaynak GLB/PNG/LOD sürümlerine asla yerinde müdahale edilmez. **Tek varlık / tek job** ana iş akışıdır.

### Durum seviyeleri

- **P0** — başka özelliklere geçmeden çözülmesi gereken Unity teslim hattı.
- **P1** — Unity teslimi üretim kalitesine taşıyan doğrulama ve tutarlılık.
- **P2** — kullanıcı talebindeki daha büyük Studio vizyonu (ileride).
- **P3** — nice-to-have/deneysel; performans kanıtı olmadan açılmaz.

## P0.0 — M2.2 bakım ve CI'yi yeşile döndürme

**Blokaj:** `09307179` Windows CI kırmızı; Rust/Tauri build ve 16 Rust testi geçerken Playwright Texture Export testi 30s timeout; Unity Export ve viewport e2e'de flaky davranış var.

Uygulama adımları:
1. [CI run #38644764542](https://github.com/progesor/MallfunctionAI3D/actions/runs/38644764542) trace ve screenshot'ları indir/incele.
2. Texture Inspector'ın görüntü çözme, canvas yükü, state güncelleme, loading ve timer davranışını ölç; kilitlenme varsa ayır. Teste yalnız süre ekleyerek geçme.
3. Real-browser senaryolarını aynı koşu üzerinde kararlı hale getir; `npm run test:adapter`, `npm run build`, `npm run test:ui`, `cargo check`, `cargo test --lib`.
4. M2.2 URP staging çıktısını Studio'da gerçek PLAYBOX kaydıyla üret ve kaynak hashlerini doğrula. Gerçek Unity'ye import edilmeden **staging** diye etiketle.

**Kabul kapısı G0:** Güncel HEAD'de **tam Windows CI yeşil**, tekrar koşusunda flaky olmayan UI testleri; eski AI workflow üretimi bozulmamış. M2.2 henüz prefab olmadığı açıkça belirtiliyor.

## P0.1 — Unity profil algılama ve güvenli hedef seçimi

- Studio ayarlarında `Unity Editor path` ve `Unity Project path` tercihe bağlı tutulacak (gerekirse kullanıcıdan sadece bir defa).
- Proje `ProjectSettings/ProjectVersion.txt`, `Packages/manifest.json`, GraphicsSettings/active pipeline incelenerek **Unity sürümü ve URP/HDRP** teyit edilecek. Paketin sürümüne göre tanımlı shader/property sözleşmesi seçilecek.
- `Auto` profil güvenle belirlenemiyorsa hata/manuel URP-HDRP seçimi; yanlış pipeline'a otomatik import asla yapılmasın.
- Çıktı hedefi doğrudan Project/Assets altında izole `Assets/MallfunctionAI3D/<AssetId>/<Version>/`; dışarıdaki path'e yazma korunacak. Proje sürüm kontrolü durumu kullanıcının işini engellemeyebilir ama hedef yol ve üzerine yazma teyidi gerekir.
- Unity Editor paketi/uzantısı, bir kez kurulup bütün varlıklarda kullanılabilir **modüler bir importer** olarak geliştirilecek (`com.progesor.ai3d.importer` benzeri UPM local package). Oyun özel `ItemDefinition` sınıfına zorunlu bağımlılık yok.
- Tekrar dışa aktarma için **aynı assetId + yeni sürüm**; Unity asset GUID'leri ve kullanıcı prefab değişiklikleri için `replace/update/new version` politikası açık seçilir.

**G1 kabul:** Yanlış projeye/sürümüne veya izin verilmeyen klasöre yazma yok; aktif Unity pipeline doğru saptanıyor ve test ediliyor.

## P0.2 — FBX tabanlı Unity-native geometri üretimi

**Karar:** GLB arşiv/dönüşüm formatı olarak kalır. Oyun tarafındaki *varsayılan teslim* Blender aracılığıyla **FBX** (Unity'nin yerleşik ModelImporter'ı) olacak. GLB/glTFast yalnız opsiyonel uyumluluk yolu.

- Aynı üç LOD geometri ve UV/normal/tangent bilgisi Blender 5.1 üzerinden tek birleşik `<asset>_LODs.fbx` (alt node'lar `LOD0`, `LOD1`, `LOD2`) veya testte gerekirse üç FBX olarak verilecek. Hangisinin Unity importta daha güvenilir olduğu gerçek testle kararlaştırılacak.
- Ölçek/axis/pivot için **Y-up Unity**, tutarlı transform, kaynak pivot ve explicit `heightMeters` opsiyonu; kaynaktan uydurma gerçek ölçü yok.
- Normaller/UV korunacak, yeni decimate/bake sadece açık istekte. Aynı asset farklı LOD'larda aynı yer/pivot ve dünya ölçeğine sahip olmalı.
- FBX'in kendisinin Unity `.mat` ve `LODGroup` oluşturması beklenmez; bunlar Editor importer'da yapılır. Yanlışlıkla “FBX hazır prefab” denmeyecek.
- Çoklu materyal/skin/animasyon, renderer submesh ve model özel collider ihtiyaçları sonraki kapsam; v1 tek materyalli, statik ürün.

**G2 kabul:** Fox FBX Unity ModelImporter ile ek glTF eklentisi olmadan sorunsuz yükleniyor; üç LOD silueti, ölçü/pivot, UV/normal, üçgen sayısı referansla eşleşiyor.

## P0.3 — Unity için gerçek URP ve HDRP kanal paketleyici

**Ayrı profiller zorunlu.** URP stok Lit ile HDRP Lit shader alan adları farklıdır. Mevcut M2.2 `MetallicSmoothness.png` (R metal A smoothness) ve ayrı `Occlusion.png` (G AO) üretir; bu **URP staging**, kullanıcının istediği tamamlanmış tek packed-mask aktarımı değil.

### Ortak kaynak dönüşümleri
- glTF'nin metallic: metalness kanalı, roughness: ayrı roughness, AO: occlusion; PBR GLB'de gerekli factor/strength varsayılanları ayrıca okunur.
- `smoothness = 1 - clamp(roughness × roughnessFactor)`, `metallic = clamp(metallic × metallicFactor)`; **AO aktarımında G kanal hedefi**; uygun sRGB/linear semantiği korunur.
- Base Color sRGB; mask data **linear (sRGB kapalı)**; Normal `Normal Map` tipi. Green inversion seçilebilir ama otomatik/varsayımsal invert yapılmaz.
- **URP/Lit** için önerilen aynı packed map: `URP_Mask.png` `R=Metallic, G=Occlusion, B=unused, A=Smoothness`. Aynı PNG hem Metallic hem Occlusion slotuna bağlanır; proje/Unity sürümündeki shader kontrol edilir. Stok URP/Lit doğrudan `_MaskMap` beklemez.
- **HDRP/Lit** için `HDRP_Mask.png`: `R=Metallic, G=AO, B=DetailMask, A=Smoothness`. `_MaskMap` alanına bağlanır. Detail map yoksa B için açıkça belgelenmiş sabit politika kullanılır; testte görsel etkisi doğrulanır.
- Farklı shader graph varsa `Shader Profile Adapter` gerekir; URP/HDRP standart Lit kapsamı ilk kabulde yeterli.
- Alpha/saydamlık ile packed smoothness çakışmamalı; base alpha saydamlık içeriyorsa ayrı profil ve uyarı.

**G3 kabul:** Birim testlerde sentetik piksel RGBA değerleri ve formüller birebir doğrulanmış; Fox üzerinde hem URP Lit hem HDRP Lit doğru shading; AO/metal/smoothness kanalları ve normal görseli referansla karşılaştırılmış.

## P0.4 — Unity Editor otomatik materyal, LODGroup ve prefab üretimi

Bir kez kurulan Unity importer:
1. `package_manifest.json` + hash kontrolü; hedef pipeline uyuşmazsa blok.
2. FBX'leri, ortak Base Color/Normal/Mask map'leri ve import ayarlarını Unity AssetDatabase içine alır. **TextureImporter.sRGB**, NormalMap type, mipmap ve platform compression profili ayarlanır; alpha korunur.
3. Pipeline shader'a göre **tek ortak Material** yaratır (`Universal Render Pipeline/Lit` veya `HDRP/Lit`), doğru map slotlarını atar, smoothness/normal/occlusion güçlerini belirtir.
4. `Asset_LOD0 / LOD1 / LOD2` renderer'larını aynı kök altında kurar ve **`LODGroup.SetLODs`** ile bağlar. Varsayılan screen-relative transition değerleri `0.60 / 0.28 / 0.08` yalnız başlangıç parametresi; Editor arayüzünden ve Studio profilinden değişebilir.
5. Tek bir **BoxCollider / SphereCollider / CapsuleCollider** veya `None` seçeneği; oyuncu tarafından taşınan ürünlerde fizik komponentleri oyun tarafı sınırında kontrollü eklenir (gereksiz çoklu MeshCollider yok).
6. `PrefabUtility.SaveAsPrefabAsset` ile `<asset>.prefab` oluşturur; proje `Assets/MallfunctionAI3D/.../` içinde `*.mat` ve kaynak map'lerle birlikte kayıtlıdır.
7. Katalogdaki export sürümü ve Unity'de oluşturulan Prefab'ın yolu/hash/status'u Studio'ya raporlanır. Başarısızlıkta yarım importlar temizlenir; başka varlıklara dokunulmaz.

**G4 kabul:** Tek tıklama/tek importer çalıştırması sonucunda kullanıcı **sıfır manuel materyal veya LODGroup kurulumuyla** prefabı sahneye sürükleyip görebilir. Ön/arka/kuyruk, 2/5/10 metre raf kamera testi; doğru shader/LOD/texture/collider.

## P0.5 — Teslim yolları: proje aktarımı + .unitypackage

- **Tercih edilen yol:** Studio'da hedef Unity Project seç → üret → otomatik Editor importer; kullanıcı prefabı Assets altında görür.
- **Taşınabilir yol:** Kaynak olmayan proje bilgisayarları için `Unity-ready ZIP` + importer bileşeni; gerekiyorsa Unity Editor/AssetDatabase yardımıyla **gerçek `.unitypackage`** export. Rust dosya üreticisi tek başına geçerli Unity asset GUID/metafile/prefab üretmiş sayılmaz.
- Unity Editor kapalı/açık olma senaryoları ayrı test edilir. Proje açıkken ikinci `-batchmode -projectPath` ile çakışma yapmaktan kaçın; Editor içi import watcher/menü komutu tercih edilir.
- Optional `Assets/Editor` kodu yerine UPM importer bir kez kurulabilsin; taşınabilir zip ilk kurulumda açık yönerge taşısın.

**G5 kabul:** URP ve HDRP iki boş test projesine ayrı paketler aktarılır; importer en fazla bir kere kurulmuş olmalı, **asset başına manuel işlem yok**. `.unitypackage` çalışıyorsa paketin içinde çalışır prefab/material/mesh var ve Unity yeniden açıldığında bağlantılar sağlamdır.

## P0.6 — Tek varlık gerçek kabul testi ve performans

- Referans: PLAYBOX Fox Plush; aynı kaynak, LOD0=7.998, LOD1=5.000, LOD2=3.000 tris.
- İki ayrı Unity test projesi **URP / HDRP**; model doğru yön/ölçek/normal ve renk; `LODGroup` tek LOD renderer'ını gösteriyor; collider ve prefab kaydedilip yeniden açılınca çalışıyor.
- Yalnız statik tekli varlık değil, aynı prefabın 100 ve tercihen 1.000 instance'ı için `Frame Debugger` ve `Profiler` raporu; SRP Batcher/GPU instancing ve materyal paylaşımı nedeniyle gerçek draw-call farkı ölçülür. 60–70K objeyi bir anda çizme hedefi varsayılmaz.
- Unity import kapasiteleri sürüme bağlı olduğundan **ProjectVersion / URP-HDRP paket numaraları** raporlanır.

**G6 kabul:** CI tamamen yeşil + gerçek Windows/Unity URP/HDRP kabul notları + tek varlık prefabı üretime uygun **veya eksiklerin açık blok listesi**. Bu kapı olmadan M2.2 “Unity-ready” diye tamamlanmış sayılmasın.

## P1 — Unity exporter'ı genişletme (P0 sonrası)

- Birden fazla materyal/submesh, alpha/transparency, emissive, normal green flip per material, mask resampling.
- Collider proxy ve ürün pivot ayarı; MeshCollider yalnız gerekirse; kütle/Rigidbody politikası game integration.
- Unity shader/texture profilleri (`Standard URP Lit`, `HDRP Lit`, özel Shader Graph adapter).
- Doku import platform formatları: Windows BC7/BC5, gerektiğinde ASTC/ETC; uygulama doğrudan yanlış platform DDS üretmesin, Unity TextureImporter ayarları doğrulasın.
- Aynı texture referansını kullanan prefablar arasında hash tabanlı deduplication; versiyon güncellemelerinde orphan varlık temizliği, geri alma.
- İsteğe bağlı Mallfunction `ItemDefinition` / prefab ID kayıt entegrasyonu; oyun bağımlılığı opsiyonel.

## P2 — Studio platform genişletmeleri

- ComfyUI **AI görüntü üretimi** (referans üretme / varyant), ayrı modül.
- Texture Lab: otomatik map repair, UV padding/bake kalite testi, normal/AO rebake, KTX2/texture atlas, masaüstü işçi kuyruğu.
- TRELLIS.2 ve Hunyuan A/B profil karşılaştırması, asset kalite skoru, yüzey/UV bütünlük ölçümleri.
- Asset arşivi için filtreler, toplu etiketleme, projeler, portable backup/restore; iki istasyon arasında opsiyonel güvenli senkronizasyon.

## P3 — Daha sonra, gerçek ihtiyaç olursa

- Küçük batch üretim (explicit concurrency=1 ve GPU kaynak izleme); yüzlerce asset sıraya almak ana kullanım senaryosu değil.
- Otomatik asset önerileri, çoklu GPU dağıtımı, karmaşık materyal/rig/animasyon tamirleri; deneysel modüller olarak ayrılır.

## Çalışma disiplini

- Her aşamada **küçük değil, bütünleşik ama kabul edilebilir** teslimat; gereksiz mikro-commit/plan tekrarları yerine kullanıcı testine hazır paket.
- Her milestone gerçek kod + regression test + örnek çıktı + kısa doğrulama talimatı içersin.
- Var olan ComfyUI üretimindeki `Workflow aynen` sadakati ve arşiv korunmalı.
- Resmî Unity sürümü/proje pipeline'ı teyit edilmeden shader/property değişikliği merge edilmesin.
- Aynı işlev için URP ve HDRP çıktısı **yalnız seçilen hedef profile** göre üretilsin; bu iki pipeline'ın Lit materyalleri birbirinin yerine geçmez.
- Kod geliştirme dalı: `feat/desktop-studio-foundation`; PR #1 Draft; önce CI yeşil, sonra gerekirse yeni `feat/unity-oneclick-import` dalı ile ayrı PR kararı verilir.

## Resmî referanslar

- URP Lit/Complex Lit packed texture: https://docs.unity3d.com/6000.0/Documentation/Manual/urp/shaders-in-universalrp-channel-packed-texture.html
- HDRP 17 Mask/Detail maps: https://docs.unity3d.com/Packages/com.unity.render-pipelines.high-definition@17.0/manual/Mask-Map-and-Detail-Map.html
- Unity LODGroup: https://docs.unity3d.com/cn/6000.0/ScriptReference/LODGroup.html
- Unity PrefabUtility: https://docs.unity3d.com/kr/2022.3/ScriptReference/PrefabUtility.SaveAsPrefabAsset.html
- Unity asset package export: https://docs.unity3d.com/cn/current/ScriptReference/AssetDatabase.ExportPackage.html
