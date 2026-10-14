# PLAYBOX Fox Plush — İlk gerçek Pixal3D pilot değerlendirmesi

**Tarih:** 14 Ekim 2026
**Durum:** Pixal3D üretimi + PBR GLB + GPU ölçümü PASS. Blender LOD dışa aktarma ve Unity görsel/perf kabulü bekliyor.
**İstasyon:** RTX 4070 Ti SUPER (16 GB), ComfyUI Desktop 0.40.0. RTX 5080 kullanılmadı.

## Kullanıcı kararı

Kullanıcı GPU'nun %100'e ulaşmasını problem olarak değerlendirmiyor. Varsayılan üretim biçimi kontrollü, ağırlıklı **tekli üretim**; batch otomasyonu şimdilik kapalı. Pelüşün bir ayağında deformasyon, yüzeyde texture/bake izleri var; ilk prototipin genel görünümü kabul edilebilir. Zor asset'ler için daha sonra farklı kalite presetleri değerlendirebiliriz.

## Kaynak

- Orijinal referans: Meshy_AI_image_0860363f-5fd5-4b9b-a2f3-8ae99c9fd3b4_0.png, 1024x1024 RGBA.
- Referans SHA256: e77015dcad28f82a3ec7680ea0282a4cb33c1beb6a6071e5e50a6a33be4064e6
- GLB: PLAYBOX_FoxPlush_pixal3d_pilot_v01_00001.glb — 3,339,600 byte (3.19 MiB).
- GLB SHA256: 9447201e9522788b70b3c8580ffba2ac515042a23defe6a104c207a24e176388
- Referans, GLB, sistem logları özel tutulur; sadece özet/kimlik verileri commitlenir.

## GPU raporu

GPU_Pilot_20261014_161913.json dosyasından:

| Ölçüm | Değer |
|---|---:|
| ComfyUI kuyruk gözlem penceresi | yaklaşık 68.01 saniye (örneklemeli) |
| Monitor toplam | 144.09 saniye |
| GPU VRAM peak | 15,014 MiB (GPU genelindeki süreçler) |
| GPU kullanım peak | %100 |
| GPU kullanım ortalaması | %30.08 (bekleme dahil) |
| Host RAM kullanım peak | 55,948 MiB |
| GPU sıcaklık peak | 65°C |

Süre ve VRAM süreç bazlı ayrıştırılmış sonuçlar değildir. Kullanıcı tercihi uyarınca başarılı ilk preset bu aşamada sırf GPU yükünden dolayı küçültülmüyor.

## GLB teknik QC

| Alan | Değer |
|---|---:|
| Mesh / materyal | 1 / 1 |
| Vertex / triangle | 6,252 / 7,998 |
| UV ve normal | mevcut |
| PBR kullanımı | base color, metallic-roughness, normal, AO |
| Gömülü PNG | 3 (AO ve MR aynı texture'ı paylaşabiliyor) |
| Texture çözünürlüğü | 1024x1024 |
| Blender LOD / Unity | henüz denenmedi |

### Topoloji düzeltmesi

İlk saf indeks bağlantı analizi 168 bağlantılı parça ve sınır kenarları göstermişti. Ancak bu değer çok sayıdaki **UV seam ayrımı** nedeniyle yanıltıcı. Kaynak değiştirilmeksizin bir kopyada sadece aynı konumdaki vertex'ler birleştirildiğinde **3,987 pozisyon vertex'i, 2 bağlantılı kabuk ve kapalı (watertight) yüzeyler** elde edildi. Dolayısıyla 168 ayrı geometrik kopukluk olduğu iddiası doğru değil. Bu sonuç dokulu kaynağa otomatik weld uygulanmasını gerektirmiyor.

### Blender görsel değerlendirme

- Genel siluet, büyük kuyruk, kulaklar: bu prototip için yeterli.
- Bir ayak/gövde birleşiminde gerçek geometri deformasyonu görünür.
- Yüzey dikişleri ve lekeler UV/texture bake artefaktları olabilir.
- Göz/nose ayrıntıları ağırlıklı texture ile ifade ediliyor; daha fazla geometriye şimdilik ihtiyaç yok.
- Asset Unity raf görüşü, 2/5/10 m kamera ve LOD materyal görünüm testini geçmedi.

## Blender başlatma düzeltmesi

Eski "blender -b" komutu PATH'te blender.exe olmadığı için bulunamadı; bu Blender kurulu değil anlamına gelmez. Kullanıcı Blender arayüzünden görüntü paylaştı.

Yeni başlatıcı: 04_Prepare-LODs.ps1. Windows standart Blender kurulum klasörlerinden Blender.exe'yi tespit eder, bulunamazsa -BlenderPath parametresiyle tam exe yolu alır. Varsayılan kaynak: ComfyUI Desktop output klasöründeki en yeni PLAYBOX Pixal3D GLB'si; varsayılan çıktı: reports/FoxBlenderTest_<timestamp>. Kaynak asla değiştirilmez ve daha önceki dolu çıktı klasörünün üzerine yazılmaz.

```powershell
cd C:\Works\MallfunctionAI3D
git pull
.\04_Prepare-LODs.ps1 -DryRun
.\04_Prepare-LODs.ps1
```

Otomatik bulunamazsa gerçek yolu kullanıcı belirtir (aşağıdaki yol **örnektir**):

```powershell
.\04_Prepare-LODs.ps1 -BlenderPath "D:\Apps\Blender\blender.exe"
```

Beklenen çıktı: üç LOD GLB, Blender_LOD_Report.json, Blender_Run.log. Bu LOD scripti yeni normal/AO bake yapmaz. Gözle doğrulama ve Unity kabulünden önce "production-ready" kararı verilmez.

**Sonraki karar kapısı:** LOD raporu + mümkünse üç LOD görseli, ardından Unity raf kamerası, materyal/ölçek ve yakın/uzak mesafe testleri.
