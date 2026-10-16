# M3.0 Fox görsel değerlendirmesi — 16 Ekim 2026

**Branch:** feat/desktop-studio-foundation · **PR #1:** Draft. CI tetiklenmeyecek.  
**Test kaynağı:** Kullanıcının iki Studio PBR viewport ekran görüntüsü: `PLAYBOX Fox Plush / Source` ve `PLAYBOX Fox Plush · Texture Seam REVIEW / Source`.

## Karar: Fox için görsel kalite kabulü RED

Görünür gövde, yüz ve özellikle kuyruktaki geniş bantlaşma, parçalı ton farkları ve düşük kaliteli renk geçişleri REVIEW modelinde devam ediyor. Görsellerde görünür ve anlamlı bir onarım yok. Kaynakta ve REVIEW'da 7.980 triangles ve 6.639 vertices var; bu yalnız mesh korunumu açısından iyi, texture kalitesi kanıtı değil. GLB boyutu 3.34 MiB → 4.14 MiB, kalite delili değil. **Kullanıcının bu Fox REVIEW'u üretim için onaylamaması önerilir.** Kullanıcıdan rapor JSON gelmeden sıfır piksel değiştiği iddia edilmez.

## Teknik teşhis

`texture_seam_pixels.rs` sadece aynı 3D kenarın iki UV adası yakınındaki 1–3 px dikiş bandını ve küçük farkları değiştirir. Özgün `BaseColor` / `Roughness` / `AO` map'lerinde UV seam dışındaki geniş düşük frekanslı aydınlık-koyu parçalanmaları işlem kapsamı dışındadır. Sert farkları atlamak ve tangent-space Normal'i korumak bilinçli koruma. Yöntem dar UV dikişi hatasında işe yarayabilir, ancak Fox'taki **geniş yüzey bozukluğu** için uygun araç değildir. Yeni export üretmek veya radius'u sınırsız büyütmek sorunu güvenle çözmez.

UI'da `Texture Seam REVIEW` onay ekranı artık müdahale edilen gerçek piksel kapsamını manifest inspection'dan gösteriyor ve geniş doku bandını onarmadığını açıkça söylüyor; sonuç ekranı da görsel kaliteyi iddia etmiyor. Değişiklikler `f425d4d` ve `ae2e139`. Bu değişiklikler **texture algoritmasını düzeltmez**, yalnızca dürüst ve ölçülebilir kabul UX'idir. Yerel `npm build` ve `Playwright` tekrar koşusu gerekiyor.

## M3.1 için önerilen gerçek düzeltme yönü

**Hedef:** Orijinal GLB, UV0, normals, mesh, PBR geçmişi ve Unity prefablara dokunmadan; kullanıcı 3D viewport üzerinde *bozuk yüzey bölgesini* işaretleyecek ve sadece o bölgede PBR artefaktını onaracak.

1. **3D maske altyapısı (ilk dilim):** Yüzey raycast ile nokta/üçgen barycentrik + model-uzayı koordinatı; 3D fırça boyutu/gücü, ekle/sil, undo, fırça izi ve kilitli pozda maske ısı haritası. UV sınırları boyamayı sınırlamamalı; aynı dünya yüzeyi üzerinden mask UV0'a aktarılmalı. Bu dilimde önce read-only maske, otomatik doku değişikliği yok.
2. **İki kontrollü araç:** (a) renk/roughness *edge-preserving surface smoothing/leveling* (geniş bantları azaltabilir), (b) sağlıklı bölgeden örnek alıp hedef bölgeye *clone/heal*; texture detaylarını silen kör blur yapılmaz. Kamera/UV kaynakları ve örnekleme provenance saklanır.
3. **Kabul ve güvenlik:** PBR role başına maske, kullanıcı kaynak-hedef örneği; önce/sonra görünüm, değişiklik overlay'i, piksel istatistiği, baskın renk/malzeme sınırı guard; tekil versioned REVIEW. **Normal map varsayılan değişmez**; orijinal UV0 ve GLB geometrisi checksum eşleşmeli; Unity/LOD REVIEW için veto.
4. **Fox test:** Kuyruk + gövde bantları geniş alanlarda belirgin azalırken beyaz kuyruk ucu, göz/burun ve yüz-gövde sınırları korunmalı. Yakın/uzak, aynı kamera/ışıkta before-after kontrol. Ayrıca başka kategori elektronik/mobilya modelinde maske sızıntısı ve materyal sınırı test edilir.
5. **UX/performance:** Mevcut Three.js `frameloop="demand"` kalsın; her fırça hareketinde GLB yeniden yüklenmesin. 3D maske cache'li, kaydedilmiş stroke'lar geri alınabilir; screenshotlarla PASS iddia edilmez.

## Sonuç

**M3.0 statik / Cargo kabulü başarılı olabilir; Fox görsel kabulü başarısız.** Aynı otomatik 1–3px seam pass'ini tekrar tekrar denemek yerine, M3.1 kullanıcı kontrollü yüzey onarımına geçilmeli. Maskeli doku algoritması başarıya ulaşana kadar mevcut Fox REVIEW asset'i yalnız inceleme amaçlı tutulmalı.
