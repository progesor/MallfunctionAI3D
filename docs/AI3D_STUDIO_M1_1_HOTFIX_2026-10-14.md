# Mallfunction AI3D Studio M1.1 — RTX 5080 API üretim hotfix

**Tarih:** 2026-10-14  
**Durum:** Kod güncellemesi uygulanıyor. CI ve gerçek RTX 5080 üretim testi ayrı kabul kapılarıdır.

## Gelen raporlar

- Aktif istasyon NVIDIA RTX 5080 16 GB; ComfyUI Desktop 0.40.0 API: `http://127.0.0.1:8191`, output: `F:\ComfyUI\output`.
- Model ağırlıkları 7/7, resmî grafın 43/43 düğümü mevcut.
- GPU logu: genel GPU VRAM peak **12.696 MiB**, GPU yük peak **%99**. JSON dosyasındaki `station:4070ti-super` etiketi yanlış; `gpu_name` RTX 5080 ve gerçek cihaz öncelikli.
- Kullanıcı GLB viewer + Blender LOD başarılı, Studio'dan 3D üretimde mask/preview aşamasına geldiğini fakat GLB oluşturulmadığını bildirdi.
- Sağlanan ComfyUI API-export dosyası **63 node** ve tam 3D çıkış zinciri içeriyor: `Save3DAdvanced #322`, `MeshToFile3D #285`, Pixal3D shape+texture aşamaları.
- İlgili başarısız işin ComfyUI `/history/<prompt-id>` kaydı henüz paylaşılmadı; kesin hata sebebi teyit edilemez.

## Uygulanan düzeltmeler

1. Önceki M1'de UI/LiteGraph JSON, `/object_info` ile API biçimine dönüştürülüyordu. Orijinal UI grafının `Save3DAdvanced.viewport_state` alanı `""` olabilirken kullanıcı API export'unda bu alan kamera bilgilerini içeren bir nesne. Bu bir uyumluluk riskiydi.
2. M1.1 artık doğrudan doğrulanmış **ComfyUI API JSON** kaynak dosyasını kullanır. `studio/src/lib/api-workflow.ts` kaynak grafı değiştirmeden seçenekleri uygular.
3. `ComfyUI API JSON İçe Aktar` seçilen API üretim grafını hash kimliğiyle yerel arşive alır. `Save3DAdvanced #322` -> `MeshToFile3D #285` zinciri yoksa mask-only grafı reddeder.
4. PBR bake / UV bağlantıları korunur; `Save3DAdvanced.viewport_state` yapılandırılmış nesne olarak korunur; çıktı için gerekli olmayan yan önizlemeler gönderilmez.
5. `comfy_job_diagnostics` üzerinden `history` hataları ve çıktı düğümleri görüntülenir. Eski işleri salt okunur incelemek için `06_Diagnose-ComfyUI-Job.ps1` eklendi.
6. Başlık/sabit paneller küçültülüp 3D viewer genişletildi. Tam genişlik görünüm düğmesi ve kompakt araç alanı eklendi.
7. GPU cihaz adı ile istasyon etiketi karıştığında düzeltme seçeneği sunulur.

## Bekleyen kabul

- User API dosyası üzerinde TypeScript graph regression testi çalıştırılmalı.
- Windows GitHub Actions: `npm run test:adapter`, `npm run build`, `cargo check`, `cargo test --lib` izlenmeli.
- RTX 5080'de **tek üretim** ile `Save3DAdvanced #322` GLB sonucu, GPU kullanım ve arşive otomatik ekleme doğrulanmalı.
- Gerçek hatayı kesinleştirmek için yeni başarısızlıkta Tanı Kaydı veya önceki iş için `06_Diagnose-ComfyUI-Job.ps1` JSON'u kullanılmalı.

Çalışan ComfyUI / Blender kurulumu, model ağırlıkları ve eski GLB dosyaları değiştirilmedi. Batch kapalı.
