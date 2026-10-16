# MallfunctionAI3D Studio — M3 Devir Belgesi (15 Ekim 2026)

**Repo:** https://github.com/progesor/MallfunctionAI3D  
**Aktif branch:** feat/desktop-studio-foundation  
**Draft PR:** https://github.com/progesor/MallfunctionAI3D/pull/1  
**Çalışma ortamı:** Windows / Tauri 2 + Rust / React + Vite / Blender 5.1.2 / ComfyUI / Unity URP + HDRP.

**ÖNEMLİ:** Bu devir M3.0 kaynak kodunun son durumudur. Kullanıcı Windows PC üzerinde M3.0 için başarılı Cargo, Playwright, gerçek GLB/texture ve Unity kabul testleri henüz alınmadı. Kaynak kod kontrollerini gerçek build PASS gibi gösterme. GitHub CI çalıştırma.

## 1. Son commitler ve kullanıcı düzeltmesi

- 5877f2ac: Kullanıcının UVRepairPanel.tsx içindeki iki karakterlik JSX/parantez düzeltmesi; sonraki commitlerde korunmuştur.
- 81ebccec: M2.8 xatlas chart sweep ve Auto / Compact / Fidelity atlas hedefleri.
- b73d93fe: M3.0 3D Texture & Mesh Health; seam analizi, dar bölge PBR repair, REVIEW ve manuel onay.
- 4d1f0ea4: Seçilmemiş PBR maplerin ve özellikle Normal PNG'nin byte-identical korunması, M3 test runner.
- f1954b03: Kullanıcının Windows logunda görülen 24 geçersiz Rust leading-dot float (ör. .10) hatasının tümü düzeltildi; studio/scripts/check-rust-literals.mjs ve npm pre-dev denetimi eklendi.
- Son devir commit'i: tools/Test-TextureRepair.ps1 -Full içindeki sıralama Cargo check/test önce, Playwright en sonra olacak şekilde düzeltildi; bu belge eklendi. Yeni sohbette son branch HEAD kontrol edilmeli.

PR #1 hâlâ Draft ve merge edilmedi. .github/workflows/studio-ci.yml yalnız manual workflow_dispatch içerir. GitHub Actions tetiklenmeyecek, push/PR ile auto-CI açılmayacak.

## 2. Neden Auto UV yerine 3D Texture Repair?

Fox'un otomatik UV düzeltmeleri kuyruğu bantlaştırdı, PBR atlasını ve renkleri bozdu. Smart UV fallback 262 adayı 1.733 adaya çıkardı. M2.7 normal-safe topology proxy sorununu çözdü: 262 disconnected geometrik bileşen → 4. xatlas 0.0.11 çalıştı ve P95 gerilmesini 1.24099 → 1.09790; maksimum gerilmeyi 30.14263 → 3.16874 düşürdü. Ancak 262 adaya karşı 459 UV adası üretti; görsel olarak elle düzenlemesi zorlaştı.

M2.8 Auto UV deneysel araç olarak korunacak. Ana yeni yaklaşım: **orijinal UV0, mesh, vertex, normal ve kaynak GLB'ye dokunmadan, aynı 3D yüzeyin iki UV adası arasındaki küçük PBR farklarını iyileştirmek.** Fox'a hardcoded isim/topoloji kuralı yazma. Farklı bir mağaza ürünüyle de test et.

## 3. M3.0 kaynak dosyaları

| Yol | İşlev |
|---|---|
| studio/src-tauri/src/texture_seam_geometry.rs | Indexed glTF POSITION+TEXCOORD_0 okur; aynı 3D edge üzerinde farklı UV koordinatı kullanan kenarları eşler; open, sharp, non-manifold ve bozuk edge sağlık raporu. |
| studio/src-tauri/src/texture_seam_pixels.rs | BaseColor/Metallic/Roughness/Occlusion için 1–3 px düşük/orta kontrastlı dikiş yumuşatma; yüksek farkları atlama; change-mask PNG. Normal map işlenmez. |
| studio/src-tauri/src/texture_surface.rs | analyze_texture_seams, repair_texture_seams, approve_texture_seam_review IPC; kaynak/PBR SHA kontrolleri, yeni versiyon, kayıt ve manuel onay. |
| studio/src-tauri/src/uv_repair.rs | gltf_factors / make_orm / build_repaired_glb yardımcıları M3 tarafından yeniden kullanılır; M2.8 experimental UV de burada kalır. |
| studio/src-tauri/src/lib.rs | Üç yeni IPC komutunu kaydeder. |
| studio/src/lib/bridge.ts | SurfaceInspection, SurfaceRepairResult ve repair köprüsü. |
| studio/src/components/TextureRepairPanel.tsx | Sağ Inspector'da analizi, rol seçimi, Gentle/Balanced, şerit boyutu, REVIEW/visual approval. |
| studio/src/App.tsx | Texture Repair paneli ve Texture REVIEW vs UV QA FAIL ayrı etiketleri. |
| studio/e2e/texture-surface.spec.ts | Mock Tauri UI: analiz → REVIEW → onay. |
| tests/test_texture_seam_contract.py | Source/mesh/normal bütünlüğü, Unity guard ve Rust fractional-literal statik sözleşmeleri. |
| tools/Test-TextureRepair.ps1 | PC'de yerel M3 testleri. |
| docs/AI3D_TEXTURE_SURFACE_M3_0.md | M3.0 teknik tasarımı. |

## 4. Algoritma / kaynak bütünlüğü

- **Salt okunur analiz:** Orijinal GLB SHA256 ve seçilen beş PBR Texture Export map SHA doğrulanır. Aynı 3D kenarda farklı UV temsilini kullanan iki yüzün dikişi aday olabilir. Non-manifold (3+ yüz), açık edge, keskin açılı (normal dot < 0.90) ve geçersiz UV çiftleri otomatik düzeltmeye girmez.
- **Dar dikiş düzeltmesi:** Gentle/Balanced, 1–3 px; yalnız seçilen dört PBR rolünde. İki taraftaki görüntü orijinal kaynak PNG'den okunur; çok yüksek renk/scalar farkları atlanır. Normalize piksel farkı ortalaması 0.085, en kötü fark 0.22, değişen piksel oranı %8 üzerindeyse işlem reddedilir.
- **Normal korunur:** Normal tangent-space map hiçbir zaman otomatik blur edilmez; seçilmiş Texture Export'taki Normal PNG byte-identical kopyalanır. Seçilmemiş diğer rol PNG'leri de encode edilmeden birebir kopyalanır.
- **Mesh/UV korunur:** Orijinal/result geometry_only_glb verileri tam karşılaştırılır. Farklı triangles/UV0/vertex/normal varsa yeni katalog varlığı kabul edilmez. Kaynak GLB, eski UV repair arşivleri, Blender dosyaları ve Unity prefablara dokunulmaz.
- **Sadece REVIEW:** Yeni varlık Texture Seam REVIEW; manifest reviewOnly=true ve productionExportBlocked=true. Unity Native ve legacy export REVIEW asset'ini engeller. Kullanıcı önce orijinal ve yeni 3D modeli karşılaştırıp ayrı checkbox/onay komutuyla yeni, ayrı Texture Repair Approved varlığı üretir.
- **Sınırlar:** Geniş yüzey/kuyruk doku gürültüsünü, büyük boya hatalarını veya normal bozulmalarını otomatik düzeltmez. Mesh deliklerini doldurmaz, üst üste binen yüzleri silmez. İlk sürüm tek opak materyalli statik triangle GLB'lerle sınırlı; multi-material, rigged/animated ve vertex-color adaptation daha sonra.

## 5. Son derleme hatası ve gerçek test seviyesi

Kullanıcının paylaştığı Windows terminal logunda texture_seam_geometry.rs ve texture_seam_pixels.rs içindeki testlerde toplam **24** geçersiz Rust sayısı (.10, .15, .65 gibi) Cargo derlemesini durdurmuştu. Branch HEAD f1954b03 ile düzeltildi. Ayrıca npm dev / npm check:syntax içinde Rust fractional-literal guard mevcut.

Devir öncesi GitHub'daki mevcut **16 Rust modülünde** aynı tarama mantığı ile 0 geçersiz leading-dot float bulundu. GitHub source/bridge/IPCs, REVIEW koruması, kullanıcı JSX düzeltmesinin mevcut olması ve manual-only CI doğrulandı.

**Bunlar gerçek compile/test değildir.** Asistan çalışma konteynerinde Rust/Cargo yok; kullanıcı Windows PC'deki güncel Cargo check, cargo test, npm build, Playwright ve gerçek GLB görsel kalite kabulü hâlâ bekleniyor. Hata çıkarsa terminalin ilk hata bloğundan ilerle. Kullanıcıya yine manuel parantez/ondalık düzeltmesi bırakma.

## 6. Yeni sohbetin İLK işi

Windows PowerShell:

~~~powershell
cd C:\Works\MallfunctionAI3D
git status --short
git fetch origin feat/desktop-studio-foundation
git switch feat/desktop-studio-foundation
git pull --ff-only origin feat/desktop-studio-foundation
.\tools\Test-TextureRepair.ps1 -Full
~~~

Güncel test sırası: Python M3 source regression → npm run check:syntax (Rust literal + TSX parser) → npm run build → **cargo check** → cargo test --lib texture_seam → cargo test --lib texture_surface → yerel Playwright e2e/texture-surface.spec.ts.

Playwright kurulu değilse bunu açık bildir, Windows'ta npm ci ile kur; yeni paketleri sessizce indirme, GitHub CI açma. git status yerel değişiklik gösteriyorsa silmeden önce incele. Compile/test başarısızsa yeni özellik geliştirmeden önce her bir hata için kod düzeltmesi ve commit yap.

Başarılı testlerden sonra:

~~~powershell
.\05_Start-Studio.ps1
~~~

Original Fox SOURCE → sağ Inspector → **3D Texture & Mesh Health** → 1024 PBR Export → **Gentle / 2 px** → **3D dikiş analizi** (salt okunur). Sonuçtaki seams, BaseColor/Roughness changedPixels raporlanır. Sonra yalnız BaseColor + Roughness seçip yeni Texture Seam REVIEW oluştur. Orijinal ve yeni 3D görünümü aynı ışık altında değerlendir, ChangeMask PNG ve Texture_Repair_Report.json raporunu incele. Kötü görünüyorsa onay verme. Başarılıysa ayrıca ikinci onaylı katalog modeli oluştur; Unity'ye yalnız approved model gönder.

**Kullanıcıdan beklenen:** Test-TextureRepair.ps1 -Full terminal çıktısı; başarısızsa Cargo/Playwright ilk hata bloğu; başarılıysa analizde seams/changedPixels sayıları ve orijinal–REVIEW ekran görüntüleri. En az bir farklı elektronik/mobilya ürünüyle de kontrol yap.

## 7. Yol haritası

1. **M3.0 acceptance:** Windows Cargo/Playwright düzeltmeleri + gerçek Fox + farklı mağaza ürünü testleri. Bu tamamlanmadan büyük ek modül ekleme.
2. **M3.1 3D fırça/maskeli texture repair:** 3D bölgeleri seç, dikiş ısı haritasını göster, lokal müdahale ve undo/versioned snapshots.
3. **M3.2 Reversible mesh repair:** küçük hole/overlap için önce read-only aday raporu, sonra önce/sonra 3D preview ve açık onay. Görünür yüzleri kör silme.
4. **M3.3 World-space normal/PBR:** tangent-space normali UV adaları arasında blur yapma; ortak 3D frame ile karşılaştır, bölgesel Roughness/AO sorunlarını değerlendir.
5. **Unity URP/HDRP:** yalnız approved asset için yeni Texture Export → LOD → native Unity import acceptance. REVIEW asset Unity'ye gitmez.

## 8. Çalışma kuralları

- Son commitleri ve kullanıcının yerel değişikliklerini koru; remote branch'i force update etme.
- Kod + test + commit yap; test geçmediyse PASS iddiasında bulunma.
- GitHub CI çalıştırma, workflow trigger ekleme.
- Orijinal GLB, map arşivleri ve Unity prefabları immutable.
- Model-agnostic ve gerçek kullanım odaklı ilerle, Fox'a aşırı uyarlama yapma.

## Yeni sohbet için hazır mesaj

> MallfunctionAI3D Studio projesinde feat/desktop-studio-foundation dalındaki docs/AI3D_M3_HANDOVER_2026-10-15.md üzerinden devam et. Draft PR #1 merge edilmedi; GitHub CI çalıştırma. M3.0, orijinal UV0/mesh/Normal'i koruyan 3D dikişli PBR analiz ve Texture Seam REVIEW → açık görsel onay akışıdır. Kullanıcı JSX parantez düzeltmesi ve 24 Rust ondalık sayı düzeltmesi branch'te mevcut. Önce son branch HEAD'i, yerel değişiklikleri ve tools/Test-TextureRepair.ps1 -Full sonuçlarını denetle. Cargo/TSX/Playwright hatalarını düzelt, Fox + ikinci model görsel kabulünü bitir, daha sonra M3.1 maskeli/fırçalı yüzey onarımına geç.
