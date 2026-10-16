# Unity One-Click Export — URP/HDRP Teknik Şartname (v1 tasarımı)
**Tarih:** 15 Ekim 2026 · **Durum:** Uygulama için onaylanması önerilen teknik hedef; **henüz tamamen uygulanmadı.**  
**Kaynak:** M2.2 `studio/src-tauri/src/unity_export.rs`, `UnityExportPanel.tsx` ve Unity resmî dokümantasyonu.

## 0. Kullanıcı deneyimi — tasarım sözleşmesi

Studio'da **Asset Library → Seçili ürün → Game Export → Unity**:
- `Pipeline`: **Auto / URP / HDRP** (Auto, proje ayarlarıyla kesin olarak doğrulanırsa).
- `Model`: **FBX (Unity Native)** varsayılan; `GLB (glTFast)` opsiyonel.
- `LOD`: arşivde mevcut 3 seviye otomatik seçilir, basit görünür uzaklık profili opsiyonel.
- `Textures`: mevcut PBR export sürümü + ortak materyal / packed mask.
- `Collider`: Box / Sphere / Capsule / None, varsayılan ürün profiline göre tek primitive.
- `Target`: bir kez seçilmiş Unity proje klasörü veya portable `.unitypackage`.
- **Export to Unity** düğmesi: kaynakları değiştirmez; başarı olduğunda `Prefab hazır · Assets/...` gibi doğrulanmış sonuç verir.
- Başarısızlıkta hata ayrıntısı/Unity Console/manifest kaydı; “Export tamamlandı” mesajı yalnız klasör kopyalanınca değil, importer başarı raporuyla gösterilir.

Bu akış **Unity projesine içe alma için kullanıcı izni** gerektirir. Uygulamanın diğer kaynaklarını, proje ayarlarını veya mevcut asset'leri sessizce değiştirmez.

## 1. Neden GLB tek başına yeterli değil?

GLB; ComfyUI/Studio arşivi ve Three.js inceleme için doğru ana format. Fakat Unity'nin yerleşik `ModelImporter` akışı için **FBX** daha doğrudan bir teslim formatıdır. FBX geometriyi, UV'leri ve normalleri taşır; Unity'nin pipeline'a özel `Material`, `LODGroup`, `Collider`, `Prefab` nesnelerini garanti etmez. Bu oyun-native nesneleri **Unity Editor C# import modülü** oluşturur.

**Hedef mimari:**
`Studio kaynak GLB + 3 LOD + PBR` → `Blender CLI FBX exporter` → `Unity URP/HDRP mask baker` → `manifest + files` → `Unity Editor importer` → `Unity Material + LODGroup + Prefab` → opsiyonel `.unitypackage`.

Bir Unity Editor kurulumunda, `.unitypackage` oluşturma için Unity Editor API `AssetDatabase.ExportPackage` kullanılır. **Rust'ta sahte prefab YAML veya Unity GUID uydurulmaz.** Portable ZIP, tam otomatik `.unitypackage` ile eşdeğer diye sunulmaz.

## 2. Shader ve kanal haritalaması

### 2.1 Kaynak glTF/PBR

- Base Color = glTF baseColor texture × `baseColorFactor` (renk uzayı doğru kullanılır).
- Metallic = glTF metalness kanalı × `metallicFactor`.
- Roughness = glTF roughness kanalı × `roughnessFactor`.
- Smoothness = `1 - saturate(Roughness)`.
- AO = glTF AO kanalından okunur; `occlusionTexture.strength` Unity materyalinde eşleniğiyle uygulanır.
- Tangent-space Normal = glTF normal texture (OpenGL +Y semantiği); Unity ekranında kontrol etmeden invert edilmez.
- Kaynak glTF `metallicRoughnessTexture` genellikle `G=Roughness, B=Metallic`; `occlusionTexture` genellikle `R=AO`. Bunlar Unity hedef kanalına **yeniden yazılmalıdır**, görüntü RGB'si kopyalanıp mask diye adlandırılmamalıdır.

### 2.2 URP/Lit — channel-packed texture

Unity 6 resmî URP belgesine göre stok Lit/Complex Lit **tek packed RGBA** texture destekler. Kanal sırası:

| Kanallar | Veri | Hedef bağlantı |
|---|---|---|
| **R** | Metallic | URP Lit Metallic map |
| **G** | Ambient Occlusion | URP Lit Occlusion map |
| **B** | Unused (sabit, örn. 0) | Shader tarafından kullanılmaz |
| **A** | Smoothness = 1 − Roughness | Metallic map alpha |

`Textures/URP_Mask.png` aynı materyalde **iki slot'a** atanır: `_MetallicGlossMap` ve `_OcclusionMap`. Shader/Mip/Linear koşulları test edilir. Ayrı **stok URP `_MaskMap` alanı varsayılmaz**.

Ek bağlantılar: `_BaseMap` = BaseColor, `_BumpMap` = Normal, `_SmoothnessTextureChannel` = Metallic Alpha, `_OcclusionStrength` = kaynak güç. Shader adı: `Universal Render Pipeline/Lit`.

### 2.3 HDRP/Lit — Mask Map

HDRP Lit için aynı fiziksel PBR kanalları, farklı materyal property’sinde kullanılır:

| Kanallar | Veri | Hedef bağlantı |
|---|---|---|
| **R** | Metallic | HDRP Lit `_MaskMap` R |
| **G** | Ambient Occlusion | HDRP Lit `_MaskMap` G |
| **B** | Detail Mask | HDRP Lit `_MaskMap` B |
| **A** | Smoothness = 1 − Roughness | HDRP Lit `_MaskMap` A |

`Textures/HDRP_Mask.png`, `_MaskMap` property'sine atanır. Kaynakta detail mask yoksa **açıkça kayıtlı sabit B politikası** kullanılır (ilk tasarım: `B=0`, detail mask yok; HDRP testinde material davranışı teyit edilecek). Sonradan detail map geldiğinde B gerçek kaynaktan doldurulur. Shader adı: `HDRP/Lit`; BaseColor/Normal parametreleri HDRP adına göre bağlanır (URP property isimleri HDRP'ye kopyalanmaz).

### 2.4 Örnek tek piksel testi

Varsayım: glTF/PNG'de normalleştirilmiş `metallic=0.30`, `roughness=0.25`, `AO=0.80`, kaynak faktörler 1.0. Yuvarlama yöntemi testle sabitlenir.

- URP: `R≈77, G≈204, B=0, A≈191`.
- HDRP: `R≈77, G≈204, B=0 (no detail), A≈191`.
- Girdide metallicFactor=0.5 olursa `R≈38`; roughnessFactor=0.5 ise smoothness `1-0.125=0.875`, A≈223.

**Burası gerçek dönüşüm testidir:** mevcut M2.2 URP'de metal/smooth ve occlusion **ayrı PNG** olarak yazılır; yeni sürüm tek **packed mask** üretecek. URP'de aynı görüntü iki ayrı shader girişine, HDRP'de tek Mask Map girişine bağlanacak.

### 2.5 Texture Importer kuralları

| Dosya | Unity import |
|---|---|
| BaseColor | sRGB açık; alpha tipi/transparent ayrıca kararlaştırılır |
| Normal | Texture Type = Normal map, sRGB veri haritası gibi ele alınmaz; tangent-space/green doğrulanır |
| URP_Mask / HDRP_Mask | **sRGB kapalı** (linear), mipmaps kullanım durumuna göre açık, alpha korunur |
| Diğer yardımcı AO/roughness debug | Temel prefabın zorunlu bağımlılığı olmadan isteğe bağlı |

Platform GPU formatları (örn. BC5 normal, BC7 albedo/mask) **Unity TextureImporter platform override'ında** ayarlanır ve görsel/alpha davranışı test edilir. PNG dışa aktarımında doğrudan GPU BC7 iddiasında bulunulmaz. Texture normal downscale işleminde normal vektörleri normalize edilmelidir; gamma hatası kabul edilmez.

## 3. Önerilen fiziksel paket biçimi

Studio'nun arşivi ve Unity projesi arasında açıkça ayrım olsun.

```text
Studio/asset_versions/<AssetId>/game_exports/<Version>/
  manifest.json
  SourceProof/
    source_hashes.json
  Meshes/
    Asset_LODs.fbx            # LOD0/1/2 child mesh veya doğrulanmış çoklu FBX
  Textures/
    BaseColor.png
    Normal.png
    URP_Mask.png              # URP profili seçildiyse
    HDRP_Mask.png             # HDRP profili seçildiyse
  Previews/
    URP_or_HDRP_preview.png   # opsiyonel
  ImportStatus/
    unity_import_report.json  # importer tarafından oluşturulur

UnityProject/Assets/MallfunctionAI3D/Imported/<AssetId>/<Version>/
  Meshes/Asset_LODs.fbx
  Textures/BaseColor.png
  Textures/Normal.png
  Textures/URP_Mask.png OR HDRP_Mask.png
  Materials/Asset_URP.mat OR Asset_HDRP.mat
  Prefabs/Asset.prefab
  unity_asset_manifest.json
```

Portatif teslim istenirse Unity Editor'ın ürettiği `<AssetId>_URP_vN.unitypackage` / `<AssetId>_HDRP_vN.unitypackage` içinde aynı prefab/material kaynakları olur. Tam otomatik paket mümkün değilse `Unity Project Folder + Importer` modu açık ve doğrulanabilir alternatif sunulur.

## 4. Manifest sözleşmesi

Örnek (tasarım; henüz çalışır export manifest sürümü değildir):

```json
{
  "schemaVersion": 2,
  "assetId": "PLAYBOX_FoxPlush",
  "versionId": "v0002",
  "pipeline": "URP",
  "unity": {
    "detectedVersion": "USER_PROJECT_VERSION",
    "renderPipeline": "URP",
    "shader": "Universal Render Pipeline/Lit",
    "targetPath": "Assets/MallfunctionAI3D/Imported/PLAYBOX_FoxPlush/v0002/"
  },
  "source": {
    "engine": "Pixal3D",
    "glbSha256": "SHA256_PLACEHOLDER",
    "sourceUntouched": true
  },
  "geometry": {
    "path": "Meshes/Asset_LODs.fbx",
    "lodNames": ["LOD0", "LOD1", "LOD2"],
    "triangles": [7998, 5000, 3000],
    "screenHeights": [0.6, 0.28, 0.08],
    "heightMeters": null,
    "collider": "Box"
  },
  "materials": [{
    "name": "Asset_URP",
    "baseColor": "Textures/BaseColor.png",
    "normal": "Textures/Normal.png",
    "packedMask": "Textures/URP_Mask.png",
    "channels": {"R":"metallic","G":"occlusion","B":"unused","A":"smoothness"},
    "linearPackedMask": true,
    "sharedAcrossLODs": true
  }],
  "filesSha256": {},
  "importStatus": "STAGED_NOT_IMPORTED"
}
```

`schemaVersion`, Unity shader, dosya listesi, checksum'lar ve ölçü/pivot birim verisi importer için zorunlu. Importer başarıyla bitince `UnityPrefabReady` durumunu bildirir; yalnız dosya kopyalayan Rust katmanı bu durumu yazamaz.

## 5. Unity Editor Importer sözleşmesi

**Dağıtım:** `com.progesor.ai3d.importer` gibi bağımsız bir UPM Local Package; Unity Editor ilk kurulumdan sonra Studio export manifest klasörlerini otomatik işler.

Geliştirme bileşenleri:
- `AssetDatabase` tarama/Refresh, `ModelImporter` FBX import ayarları (normals/tangents/scale/animation kapalı).
- `TextureImporter` linear/sRGB/Normal map, alpha/mipmap/compression ayarları.
- `Shader.Find` veya uygun ShaderGraph profile resolver; yanlış pipeline'a materyal ataması başarısız kabul edilir.
- `Material` oluşturma / `AssetDatabase.CreateAsset`: tam bir `.mat`, LOD0/1/2 rendererlara **aynı nesne**.
- `LODGroup`, `SetLODs`, `RecalculateBounds`, fade opsiyonları.
- Collider isteğine göre tek primitive; mevcut oyun `ItemDefinition` veya gameplay script'lerine varsayılan zorunlu bağımlılık **olmasın**.
- `PrefabUtility.SaveAsPrefabAsset`: .prefab kaydet ve AssetDatabase üzerinden yeniden yükleyerek referansları doğrula.
- `AssetDatabase.ExportPackage`: dosyaları ve dependency zincirini `.unitypackage` çıkarmak için yalnız Editor içinde kullan.
- Tekrarlı import **idempotent** olmalı: aynı assetId+version yeniden geldiyse aynı GUID/kurgu saklanmalı; yeni version ayrı kayda gider. Kullanıcının elle düzenlediği prefab versiyonu sessizce ezilmemeli.
- Unity proje/Library klasörünün kilitli olması, özel Unity Editor ayarları veya shader bilinmezliği açık hata ve log üretmeli; kullanıcı onayı olmadan var olan sahneler değiştirilmemeli.

**Önemli:** Unity `AssetPostprocessor.OnPostprocessModel` içindeki geçici objelerden başka dosyaya hemen prefab referansı üretmek güvenli değil; importer, import tamamlandıktan sonra ana Editor AssetDatabase aşamasında prefab oluşturmalı.

## 6. Otomatik kabul testleri

### Rust / Blender / TypeScript
1. Packed 1x1/4x4 piksel RGBA testleri: R/G/B/A, metallicFactor/roughnessFactor, AO, smoothness rounding, linear/sRGB, alpha korunumu.
2. GLB source-vs-FBX test: üçgen/vertex sayısı, UV0/normal/tangent, transform/pivot/bounds; kaynak hash değişmemiş.
3. LOD materyal dedupe: tam 1 materyal/tek texture set, üç LOD'da aynı referans.
4. Kaynak klasörüne ve Unity projesi dışına path traversal engeli; GLB/bake/LOD checksum doğrulaması.
5. Çoklu materyal, animasyon, transparency, missing map durumunda sessizce düşürmek yerine anlaşılır reddetme.
6. Export geçmişi ve yeniden import senaryosu (overwrite değil, yeni version).

### Unity Editor — gerçek kabul
1. Unity sürümü ve paket pipeline'ı teyit edilmiş **URP** test projesine Fox export: otomatik `*.fbx`, `*.mat`, `*.prefab` + 3 LOD + collider.
2. Ayrı **HDRP** test projesinde aynı kaynak, doğru `_MaskMap` kanalları ve shader.
3. Materyal rengini/normal yönünü gerçek ComfyUI/Blender görüntüsüyle kıyasla; normal seams ve AO/mask atlas hataları.
4. Gizli/fazla renderer yok, LOD geçişi 2/5/10 metre gerçek kamera görüşünde çalışıyor; import tekrarlandığında dosya/sahne bozulmuyor.
5. Prefab kaydet → Unity kapat/aç → asset/mat/link sağlam. Unity Inspector'da kanallar ve TextureImporter import flag'leri doğru.
6. İsteğe bağlı `unitypackage` içine prefab ile dependency'ler dahil; boş Unity projesine import edildikten sonra sıfır elle materyal/LOD işlemi.
7. Unity proje gerçek performans testi: 100/1000 tekrar, Frame Debugger/Profiler, ayrı LOD/material paylaşımı; shader pipeline farkı raporlu.

## 7. Bu şartnamenin implementasyon sırası

**Önce CI onarımı** → Unity proje/pipeline algılama → Blender FBX → URP packed mask + importer prefab → HDRP mask/profile + importer → portable unitypackage → gerçek Unity kabul. Yeni AI üretim motorları veya batch bu kapıdan önce öncelik değildir.

## Resmî teknik kaynaklar (15 Ekim 2026 kontrolü)

- URP Lit packed texture (R Metallic/G AO/B unused/A Smoothness): https://docs.unity3d.com/6000.0/Documentation/Manual/urp/shaders-in-universalrp-channel-packed-texture.html
- HDRP 17 Mask Map (R Metallic/G AO/B Detail Mask/A Smoothness): https://docs.unity3d.com/Packages/com.unity.render-pipelines.high-definition@17.0/manual/Mask-Map-and-Detail-Map.html
- Unity LODGroup: https://docs.unity3d.com/cn/6000.0/ScriptReference/LODGroup.html
- OnPostprocessModel: https://docs.unity3d.com/cn/6000.0/ScriptReference/AssetPostprocessor.OnPostprocessModel.html
- PrefabUtility: https://docs.unity3d.com/kr/2022.3/ScriptReference/PrefabUtility.SaveAsPrefabAsset.html
- Unity ExportPackage: https://docs.unity3d.com/cn/current/ScriptReference/AssetDatabase.ExportPackage.html
