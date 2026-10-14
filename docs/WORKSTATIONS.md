# MallfunctionAI3D — iki geliştirme istasyonu

**Kayıt tarihi:** 2026-10-14. Her iki bilgisayarda geliştirme yapılabilir. Donanım değişimi ilk Pixal3D testinin ön koşulu **değildir**.

| Alan | Station A — `4070ti-super` (aktif) | Station B — `5080` (diğer bilgisayar) |
|---|---|---|
| GPU | GeForce RTX 4070 Ti SUPER | GeForce RTX 5080 |
| VRAM | NVIDIA tarafından raporlanan 16.376 MiB | Kullanıcının geçmiş bildirimine göre 16 GB; yerel yeniden denetlenmedi |
| OS | Windows 11 Pro | Bu pilotta henüz denetlenmedi |
| RAM | 64 GB | Bu pilotta henüz denetlenmedi |
| ComfyUI | Desktop 0.40.0, `http://127.0.0.1:8188` | Kurulum/sürüm mevcut çalışmada doğrulanmadı |
| Pixal3D/TRELLIS.2 modelleri | 7/7 ad API'de mevcut | Mevcut çalışmada doğrulanmadı |
| Resmî grafın çalıştırılabilir node'ları | 43/43 mevcut | Mevcut çalışmada doğrulanmadı |
| Optimizasyonlu Fox Plush üretimi | **Pixal3D GLB/PBR + GPU log PASS (2026-10-14)**; Blender LOD/Unity bekliyor | **Henüz yapılmadı** |

## Geçmiş çalışma deneyimi

Kullanıcı ComfyUI üzerinden **daha önce 3D üretimini başarıyla gerçekleştirdiğini** teyit etti. Bu, şu anki optimize edilmiş Mallfunction Pixal3D PBR/UV/GLB pilotunun sonuçlarının şimdiden doğrulandığı anlamına gelmez. Teste kurulum değişiklikleri yapmadan geçilir.

## İstasyon bağımsızlığı ve rapor standardı

- Aynı repodaki **tekil model kimliği, referans PNG hash'i, kaynak graf commit SHA'sı, seed ve preset** kaydedilerek ilerle.
- İşlem türü (Pixal3D/TRELLIS.2), GPU modeli, VRAM toplamı, monitor başlangıç/bitiş zamanları, GLB QC ve Blender LOD bilgileri karşılaştırılır.
- Monitor başlatma örneği — Station A:
  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File .\03_Monitor-Pilot.ps1 -ComfyUrl "http://127.0.0.1:8188" -Station "4070ti-super" -MaxSeconds 1800
  ```
- İleride Station B'de aynı komutu `-Station "5080"` ile kullan. **Yeni bilgisayarda önce ayrı audit** çalıştır; model dosyalarının ortak ya da senkronize olduğunu varsayma.
- Monitor `reports/GPU_Pilot_*.csv` ve `.json` dosyalarına `station` etiketini ekler. Bu raporlar `.gitignore` nedeniyle özel kalır; gerekli özetleri (hassas dosya yolları olmadan) dokümana aktar.
- VRAM `nvidia-smi` tarafından **GPU genelinde** ölçülür, sadece ComfyUI'ye ait değildir. Aynı GPU'daki diğer uygulamalar kıyaslamayı etkiler. Bellek tabanı / peak ve kullanılan iş uygulamaları not edilmeli.
- Unity 60–70K nesneli sahnelerde varlık geometrisini otomatik sayısal limitlerle 'başarılı' sayma; görünürlük, tekrar sayısı, materyal paylaşımı ve GPU profiler sonuçları ayrıca ölçülmeli.

## Anlık karar

**İlk Pixal3D pilotu Station A'da tamamlandı.** Referans ve gerçek GLB kimlikleri, GPU ölçümleri ve topoloji düzeltmesi docs/AI3D_PLAYBOX_FOX_FIRST_RUN_2026-10-14.md altında kayıtlı. Kullanıcı kararına göre GPU'nun %100'e ulaşması tekli üretimde kabul edilebilir. Station B'ye geçmek zorunlu değil. Şimdi Blender LOD ve Unity testi yapılacak; batch otomasyonu kullanıcının çoğunlukla tekli üretim tercihi nedeniyle açılmayacak.
