# M2.1 — PBR Texture Export / Optimize

**Durum:** Kaynağa yazmayan PNG dışa aktarma uygulanmıştır. RTX 5080 üstündeki gerçek uygulama testi henüz bekliyor. ComfyUI workflow, AI üretim parametreleri ve Blender LOD dosyalarına müdahale edilmez.

## Kullanım

1. Asset Library'den bir model seç, sağ panelde **PBR Texture Inspector** aç.
2. Varsa materyal seç. LOD varyantındaysan `SRC` (orijinal) görünümüne dön.
3. Texture Export bölümünden **Kaynak**, **1024**, **512**, **256** piksel uzun kenar sınırını belirle.
4. **N PBR haritasını dışa aktar** düğmesine bas. Kaynak dosya korunur, ayrı bir çıktı klasörü oluşturulur.
5. Çıktı klasörünü kopyala. Son dışa aktarımlar (maksimum 3) sağ panelde görünür; tüm sürümler arşivde saklanır.

## Kaynak ve çıktı

Çıktı yolu `library/assets/<assetUUID>/texture_exports/pbr_<timestamp>_<UUID>/` biçimindedir. Aynı asset'in bütün işlem geçmişi, orijinal `original.glb` korunarak kaydedilir. Her klasörde:

- `m0_baseColor.png`, `m0_normal.png`, `m0_roughness.png`, `m0_metallic.png`, `m0_occlusion.png` (var olan semantik rollere göre)
- `texture_export_manifest.json`: asset ID, kaynak SHA256, preset, materyal/görüntü/kanal, çıktı dosya checksum ve boyutları.

**Tek bir glTF ORM görüntüsü** üç farklı semantik harita olarak açılır: AO=R, Roughness=G, Metallic=B. Bu nedenle 3 gömülü PNG'den 5 rol dışa çıkabilir; yanlışlıkla 5 ayrı model materyali üretildiği anlamına gelmez.

## Kalite ve performans notları

- Sadece gömülü PNG/JPEG/WEBP kaynaklar. Ağdan otomatik texture indirme yok.
- Oran korunur; hiçbir görüntü büyütülmez.
- Base Color / Emissive renk haritaları lineer ışıkta alfa ağırlıklı küçültülür.
- Roughness / Metallic / AO lineer gri kanal değerleri alan filtrelemesiyle küçültülür.
- Tangent-space normal haritaları vektör olarak ortalanır ve tekrar birim uzunluğa getirilir. Kanal yönü çevrilmez.
- Sonuç PNG'dir. PNG çözünürlüğü azalabilir ama Unity GPU belleği, gerçek platform sıkıştırma ayarları yapılmadan otomatik iyileşmiş sayılmaz.
- Bu **texture export** aşamasıdır; mesh UV rebake, PBR doku tamiri veya Unity otomatik materyal ataması **değildir**.

## Güvenlik / kalite kapıları

- Kaynak arşiv UUID ve `original.glb` sahipliği Rust tarafından doğrulanır.
- Tüm PNG dosyaları kaydetmeden önce dosya adları, temel PNG başlıkları, genişlik/yükseklik ve toplam büyüklük açısından denetlenir.
- Yazma hatasında yalnız yeni yarım kalmış export klasörü kaldırılır, önceki sürümlere dokunulmaz.
- Önizlemenin gösterdiği LOD varyantından yanlış SHA ile çıktı almamak için yalnız SRC görünümünde export yapılır.
- Node.js saf hesaplama testleri; Playwright kullanıcı tıklaması; Windows CI Rust testleri ayrı yapılır.

## İlk kabul deneyi

RTX 5080 istasyonunda yeniden AI modeli üretmeden mevcut PLAYBOX tilki modelini seçip 512 px profilini dışa aktar. Beklenen 5 PNG ve manifest. Roughness/Metallic/AO görünümü gri tonlu; normal haritası mavimsi; kaynak 1024 haritaları ve GLB değişmemiş olmalı. Kaynak model dosyasını Unity'ye aktarmadan önce normal görünüm kıyaslanır.
