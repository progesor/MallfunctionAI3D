import { useEffect, useState } from 'react';
import { Check, ChevronDown, Clipboard, Layers3, LoaderCircle, Wrench } from 'lucide-react';
import { bridge } from '../lib/bridge';
import type {
  Asset, TextureExportSummary, UvRepairResult, UvRepairPreset,
  RecoverableUvRepair, UvEngineMode, UvEngineStatus, UvAtlasObjective
} from '../lib/bridge';

interface Props {
  asset: Asset;
  revision: number;
  canRepair: boolean;
  onCreated: () => Promise<void>;
  onOpen: (asset: Asset) => Promise<void>;
}

export const UV_RUNTIME_REVISION = 'M2.8-XATLAS-TRADEOFF' as const;

export function UVRepairPanel({asset,revision,canRepair,onCreated,onOpen}:Props) {
  const [versions,setVersions] = useState<TextureExportSummary[]>([]);
  const [selected,setSelected] = useState('');
  const [loading,setLoading] = useState(true);
  const [busy,setBusy] = useState(false);
  const [preset,setPreset] = useState<UvRepairPreset>('preserve');
  const [resolution,setResolution] = useState(1024);
  const [seamAngle,setSeamAngle] = useState(62);
  const [weld,setWeld] = useState(true);
  const [safeCleanup,setSafeCleanup] = useState(true);
  const [uvEngine,setUvEngine] = useState<UvEngineMode>('both');
  const [atlasObjective,setAtlasObjective] = useState<UvAtlasObjective>('auto');
  const [engineStatus,setEngineStatus] = useState<UvEngineStatus|null>(null);
  const [runtimeError,setRuntimeError] = useState('');
  const [installingXatlas,setInstallingXatlas] = useState(false);
  const [advanced,setAdvanced] = useState(false);
  const [result,setResult] = useState<UvRepairResult|null>(null);
  const [error,setError] = useState('');
  const [recoverable,setRecoverable] = useState<RecoverableUvRepair[]>([]);
  const [chosenRecovery,setChosenRecovery] = useState('');
  const [approveOneImportFaceLoss,setApproveOneImportFaceLoss] = useState(false);
  const [acceptFailedQuality,setAcceptFailedQuality] = useState(false);

  useEffect(()=>{
    let alive=true;
    void bridge.uvEngineStatus().then(status=>{
      if(alive){setEngineStatus(status);setRuntimeError('');}
    }).catch(err=>{
      if(alive){
        setRuntimeError(String(err));
        setError('UV motorları denetlenemedi: '+String(err));
      }
    });
    return ()=>{alive=false;};
  },[]);

  useEffect(()=>{
    let alive=true;
    setLoading(true);setResult(null);setSelected('');setVersions([]);setError('');
    void bridge.textureExports(asset.id).then(rows=>{
      if (!alive)return;
      setVersions(rows);
      setSelected(rows[0]?.outputDir ?? '');
    }).catch(err=>{if(alive)setError('Texture Export listesi: '+String(err));})
      .finally(()=>{if(alive)setLoading(false);});
    return ()=>{alive=false;};
  },[asset.id,revision]);

  async function refreshRecoverable() {
    try {
      const attempts=await bridge.uvRepairAttempts(asset.id);
      setRecoverable(attempts);
    } catch {
      setRecoverable([]);
    }
  }
  useEffect(()=>{
    setRecoverable([]);setChosenRecovery('');
    setApproveOneImportFaceLoss(false);setAcceptFailedQuality(false);
    void refreshRecoverable();
    // Asset and export history changes should refresh recoverable runs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  },[asset.id,revision]);

  async function installXatlas() {
    if(installingXatlas || busy)return;
    setInstallingXatlas(true);
    setError('');
    try {
      const status=await bridge.installUvXatlas();
      setEngineStatus(status);
      if(!status.xatlasReady) {
        setError('xatlas kurulumu tamamlanamadı: '+status.message);
      }
    } catch(err) {
      setError('xatlas kurulumu: '+String(err));
    } finally {
      setInstallingXatlas(false);
    }
  }

  async function run() {
    if (!selected || busy || !canRepair)return;
    setBusy(true);setError('');setResult(null);
    try {
      const repaired=await bridge.repairUV(
        asset.id,selected,preset,resolution,seamAngle,weld,safeCleanup,uvEngine,atlasObjective);
      const fidelityTradeoff=repaired.quality?.atlasSelectionTier==='fidelity-tradeoff'
        && repaired.quality?.atlasPolicyPassed===true
        && repaired.quality?.passed===true;
      if (!repaired.sourceUntouched || repaired.newIslands<1 ||
          (repaired.newIslands>=repaired.oldIslands && !fidelityTradeoff) ||
          repaired.asset.id===asset.id) {
        throw new Error('Onarım kaydı source/UV kabul koşullarını geçmedi.');
      }
      setResult(repaired);
      await onCreated();
    } catch(err) {setError('UV/PBR onarımı: '+String(err));}
    finally{setBusy(false);void refreshRecoverable();}
  }

  // Recovery is independent of NEW bake settings. The latest failed run
  // may be 2048px while the default new-bake dropdown still reads 1024px.
  // Always list all completed runs and pass their *recorded* profile/size.
  const matches = recoverable;
  const recovery = matches.find(a=>a.versionDir===chosenRecovery) ?? matches[0];
  const importFaceDelta = recovery ? asset.triangleCount - recovery.inputTriangles : 0;
  const isFailedRecovery = recovery?.qualityStatus === 'needs-review';

  async function resume() {
    if (!selected || !recovery || busy || !canRepair) return;
    setBusy(true);setError('');setResult(null);
    try {
      const repaired=await bridge.resumeUVRepair(
        asset.id,recovery.versionDir,selected,
        recovery.profile,recovery.resolution,approveOneImportFaceLoss);
      if (!repaired.sourceUntouched || repaired.newIslands<1 ||
          repaired.newIslands>repaired.oldIslands || repaired.asset.id===asset.id) {
        throw new Error('Kurtarılmış UV varlığı kabul kontrolünü geçmedi.');
      }
      setResult(repaired);
      await onCreated();
    } catch(err) {
      setError('Önceki UV bake kurtarma: '+String(err));
    } finally {
      setBusy(false);
      void refreshRecoverable();
    }
  }

  async function saveReview() {
    if (!selected || !recovery || !isFailedRecovery || !acceptFailedQuality ||
        busy || !canRepair) return;
    setBusy(true);setError('');setResult(null);
    try {
      const draft=await bridge.saveUVReview(
        asset.id,recovery.versionDir,selected,
        recovery.profile,recovery.resolution,
        approveOneImportFaceLoss,true);
      if (!draft.sourceUntouched || !draft.reviewOnly ||
          draft.asset.id===asset.id) {
        throw new Error('QA başarısız inceleme taslağı kabul kaydı doğrulanamadı.');
      }
      setResult(draft);
      await onCreated();
    } catch(err) {
      setError('UV inceleme taslağı kurtarma: '+String(err));
    } finally {
      setBusy(false);void refreshRecoverable();
    }
  }

  async function copy(value:string) {
    try {await navigator.clipboard.writeText(value);}
    catch {setError('Klasör yolu panoya kopyalanamadı.');}
  }

  const runtimeOk=engineStatus?.backendRevision===UV_RUNTIME_REVISION &&
    engineStatus?.proxyWorkerReady===true;
  const enabled=canRepair && asset.materialCount===1 && !!selected &&
    !loading && !busy && !installingXatlas && runtimeOk &&
    (uvEngine!=='xatlas' || engineStatus?.xatlasReady===true);
  return <section className="uv-repair-panel" aria-label="Auto UV ve PBR Repair" data-testid="uv-repair-panel">
    <div className="card-heading"><div><Wrench size={16}/><strong>Auto UV + Texture Repair</strong></div></div>
    <p className="uv-runtime-version" data-testid="uv-runtime-version" role="status">
      UI {UV_RUNTIME_REVISION} · Backend {engineStatus?.backendRevision ?? 'yükleniyor'}
      {engineStatus?.workerSha256 ? ' · Worker '+engineStatus.workerSha256.slice(0,10) : ''}
    </p>
    {runtimeError && <p role="alert" className="unity-export-error"
      data-testid="uv-runtime-mismatch">
      UV backend komutu başarısız: {runtimeError}. Eski Tauri sürümünü
      kapatıp güncel feature dalından yeniden başlat.
    </p>}
    {engineStatus && !runtimeOk &&
      <p role="alert" className="unity-export-error" data-testid="uv-runtime-mismatch">
        UV motoru sürümleri uyuşmuyor. Eski Tauri backend veya
        yanlış Git dalı çalışıyor. Kaynak modeli korumak için yeni UV işlemi
        engellendi. Studio'yu kapatıp güncel feature dalından yeniden başlat.
      </p>}
    <p className="block-description">Tüm statik modeller için: UV dikişlerinde ayrılan
      vertex bağlantıları kontrollü biçimde yeniden kurulur; kaynak UV/normal korunur.
      Blender ve kuruluysa isteğe bağlı xatlas UV adayları aynı kalite kapısından geçer.
      Uygun plan bulunamazsa <strong>5 PBR map bake edilmeden</strong>
      UV_Plan.json raporu hazırlanır. Orijinal mesh ve texture arşivi değişmez.</p>
    <div className="uv-engine-status" data-testid="uv-engines">
      <label className="unity-export-select"><span>UV MOTORU</span><div><select
        aria-label="UV Repair motor seçimi" value={uvEngine}
        disabled={busy||installingXatlas}
        onChange={e=>setUvEngine(e.target.value as UvEngineMode)}>
        <option value="both">Otomatik · Blender + xatlas (varsa)</option>
        <option value="blender">Yalnız Blender Angle Unwrap</option>
        <option value="xatlas">Yalnız xatlas (kurulum gerekli)</option>
      </select><ChevronDown size={13}/></div></label>
      <p className="block-description" role="status">
        Blender: {engineStatus?.blenderReady===true?'hazır':'denetleniyor / yok'} ·
        xatlas: {engineStatus?.xatlasReady===true?'hazır':'kurulu değil'}.
        {engineStatus?.message&&' '+engineStatus.message}
      </p>
      {engineStatus?.blenderReady===true && engineStatus?.xatlasReady!==true&&
        <button type="button" className="btn btn-outline btn-full"
          data-testid="uv-install-xatlas"
          disabled={installingXatlas || busy}
          onClick={()=>void installXatlas()}>
          {installingXatlas?<LoaderCircle className="spin" size={15}/>:<Layers3 size={15}/>}
          {installingXatlas?'xatlas kuruluyor…':'xatlas 0.0.11 kur (Blender Python / PyPI)'}
        </button>}
      <small>Kurulum yalnız seçtiğin Studio kütüphanesinin plugins/xatlas
        klasörüne yapılır; Blender sistem dosyaları değiştirilmez.
        İndirmenin başlaması için bu düğmeye basman gerekir.</small>
    </div>
    <label className="unity-export-select"><span>UV ATLAS HEDEFİ</span><div><select
      aria-label="UV atlas hedefi" value={atlasObjective} disabled={busy}
      onChange={e=>setAtlasObjective(e.target.value as UvAtlasObjective)}>
        <option value="auto">Otomatik · önce az ada, gerekirse yüksek doku sadakati</option>
        <option value="compact">Az parçalı UV · ada sayısı mutlaka azalsın</option>
        <option value="fidelity">Doku öncelikli · daha çok UV adasına izin ver</option>
      </select><ChevronDown size={13}/></div></label>
    <p className="mini-disclaimer">
      Doku öncelikli atlas, yalnızca UV ada sayısı en fazla 1,8 kat artarken
      P95 gerilmesi en az %10 ve en kötü gerilme en az %50 iyileşirse
      bake işlemine geçer. Beş PBR map kalite testi ayrıca zorunludur.
      Daha çok ada, elle UV düzenlemeyi zorlaştırabilir.
    </p>
    <label className="unity-export-select"><span>PBR KAYNAĞI</span><div><select
      aria-label="UV Repair PBR sürümü" value={selected} disabled={busy||loading}
      onChange={e=>{setSelected(e.target.value);setResult(null);}}>
      {!versions.length&&<option value="">Önce Texture Export oluştur</option>}
      {versions.map((v,i)=><option key={v.outputDir} value={v.outputDir}>{i===0?'Son · ':''}{v.preset} · {v.mapCount} map</option>)}
    </select><ChevronDown size={13}/></div></label>
    <label className="unity-export-select"><span>DÜZELTME PROFİLİ</span><div><select
      aria-label="UV Repair profil" value={preset} disabled={busy}
      onChange={e=>{setPreset(e.target.value as UvRepairPreset);setResult(null);setApproveOneImportFaceLoss(false);}}>
      <option value="preserve">Doğal · Kaynak PBR değerlerini koru</option>
      <option value="plush">Peluş / kumaş · Metal=0, Roughness ≥0.82</option>
    </select><ChevronDown size={13}/></div></label>
    <label className="unity-export-select"><span>YENİ TEXTURE BOYUTU</span><div><select
      aria-label="UV Repair çözünürlük" value={resolution} disabled={busy}
      onChange={e=>{setResolution(Number(e.target.value));setResult(null);setApproveOneImportFaceLoss(false);}}>
      <option value={512}>512 px</option><option value={1024}>1024 px</option><option value={2048}>2048 px</option>
    </select><ChevronDown size={13}/></div></label>
    <button type="button" className="uv-repair-advanced" aria-expanded={advanced}
      onClick={()=>setAdvanced(v=>!v)} disabled={busy}>UV dikiş ayarı {advanced?'−':'+'}</button>
    {advanced&&<label className="unity-export-select"><span>KESKİN KENAR EŞİĞİ</span><div><select
      aria-label="UV Repair kenar açısı" value={seamAngle} disabled={busy}
      onChange={e=>{setSeamAngle(Number(e.target.value));setResult(null);}}>
      <option value={45}>45° · Daha fazla ada</option>
      <option value={62}>62° · Dengeli (varsayılan)</option>
      <option value={78}>78° · Daha büyük adalar</option>
    </select><ChevronDown size={13}/></div></label>}
    {advanced&&<label className="uv-repair-toggle"><input type="checkbox" checked={weld} disabled={busy}
      onChange={e=>{setWeld(e.target.checked);setResult(null);}}/>
      <span>UV sınırındaki aynı konumlu noktaları yalnız çalışma kopyasında birleştir (güvenli kontrol)</span></label>}
    {advanced&&<label className="uv-repair-toggle"><input type="checkbox" checked={safeCleanup}
      disabled={busy} onChange={e=>setSafeCleanup(e.target.checked)}/>
      <span>Güvenli mesh temizliği: yalnızca hiçbir yüzeye bağlı olmayan nokta/kenarları kaldır.
        Delikler, görünür çakışan yüzler ve normal yönleri otomatik değiştirilmez.</span>
    </label>}
    <button type="button" className="btn btn-outline btn-full" data-testid="uv-repair-button"
      disabled={!enabled} onClick={()=>void run()}>
      {busy?<LoaderCircle className="spin" size={15}/>:<Layers3 size={15}/>}
      {busy?'Blender UV ve 5 PBR map hazırlıyor…':'Otomatik UV + PBR Onar'}
    </button>
    {matches.length>0&&<div className="uv-repair-recovery" data-testid="uv-repair-recovery">
      <strong><Layers3 size={14}/> Önceki Blender bake dosyaları bulundu</strong>
      <p className="block-description">Kalite testinde reddedilen çalışmalar da burada gösterilir.
        Beş PNG ve geometry.glb yeniden üretilmez. QA FAIL sürümleri
        yalnızca inceleme taslağı olarak kaydedilebilir.</p>
      <label className="unity-export-select"><span>KURTARILACAK SÜRÜM</span><div>
        <select aria-label="Önceki UV onarım sürümü" value={recovery?.versionDir??''}
          disabled={busy} onChange={e=>{
            setChosenRecovery(e.target.value);
            setApproveOneImportFaceLoss(false);
            setAcceptFailedQuality(false);
          }}>
          {matches.map(a=><option key={a.versionDir} value={a.versionDir}>
            {a.qualityStatus==='passed' ? 'QA OK' : 'QA FAIL'} ·
            {a.versionDir.split(/[\\/]/).pop()?.slice(0,8)} · {a.resolution}px · {a.profile} ·
            UV {a.oldIslands} → {a.newIslands} · {a.inputTriangles} tris
          </option>)}
        </select><ChevronDown size={13}/></div></label>
      {importFaceDelta===1&&<label className="uv-repair-toggle">
        <input type="checkbox" aria-label="Blender içe aktarma tek üçgen farkını bilinçli kabul et"
          checked={approveOneImportFaceLoss} disabled={busy}
          onChange={e=>setApproveOneImportFaceLoss(e.target.checked)}/>
        <span><strong>Dikkat: kaynak {asset.triangleCount}, Blender {recovery?.inputTriangles} üçgen.</strong>
          Kaynakta yinelenen yüz kanıtlanamazsa bile, modeli gözle inceledikten sonra
          bu **yalnız bir üçgenlik içe aktarma farkını** açıkça kabul edebilirsin.
          Orijinal model değişmez; bu istisna yeni varlığın manifestine kaydedilir.</span>
      </label>}
      {isFailedRecovery ? <div className="uv-repair-review" data-testid="uv-repair-review">
        <p role="note" className="unity-export-error">
          <strong>QA BAŞARISIZ — otomatik onarım olarak kabul edilmedi.</strong>
          {recovery.qualityIssues.length>0
            ? ' '+recovery.qualityIssues.join(' · ')
            : ' Yüzey texture/UV testlerinden biri başarısız.'}
          {' '}Bu sürüm yalnızca görünüm incelemesi içindir; Unity export engellenir.
        </p>
        <label className="uv-repair-toggle">
          <input type="checkbox" aria-label="QA başarısız sürümü inceleme taslağı olarak kaydetmeyi onayla"
            disabled={busy} checked={acceptFailedQuality}
            onChange={e=>setAcceptFailedQuality(e.target.checked)}/>
          <span>Kalite hatalarını gördüm. Sürümü <strong>QA FAIL / UV REVIEW</strong>
            etiketiyle yalnız inceleme için kaydet (üretim onayı değil).</span>
        </label>
        <button type="button" className="btn btn-outline btn-full" data-testid="uv-save-review-button"
          disabled={!enabled || !acceptFailedQuality} onClick={()=>void saveReview()}>
          {busy?<LoaderCircle className="spin" size={15}/>:<Layers3 size={15}/>}
          QA başarısız sürümü taslak olarak kurtar (Blender çalışmaz)
        </button>
      </div> : <button type="button" data-testid="uv-resume-button" className="btn btn-outline btn-full"
        disabled={!enabled} onClick={()=>void resume()}>
        {busy?<LoaderCircle className="spin" size={15}/>:<Layers3 size={15}/>}
        Önceki bake'i tamamla (Blender çalışmaz)
      </button>}
    </div>}
    {!versions.length&&!loading&&<p className="mini-disclaimer">Önce yukarıdan 5 PBR haritasını dışa aktar; ardından onarım tek işlemde çalışır.</p>}
    {asset.materialCount!==1&&<p className="mini-disclaimer">v1 yalnız tek materyalli statik ürünlerde çalışır.</p>}
    {!canRepair&&<p className="mini-disclaimer">LOD önizlemesinden SRC görünümüne dön.</p>}
    {uvEngine==='xatlas' && engineStatus?.xatlasReady!==true&&
      <p role="alert" className="mini-disclaimer">xatlas-only modunu
        çalıştırmak için önce xatlas'ı kur ve durumun Hazır olduğunu doğrula.</p>}
    {busy&&<p role="status" className="texture-inspector-loading">İşlem 1–3 dakika veya daha uzun sürebilir. ComfyUI üretimi başlatılmaz.</p>}
    {error&&<div className="uv-repair-error">
      <p role="alert" className="unity-export-error">{error.split('\n')[0]}</p>
      {error.includes('\n')&&<details><summary>Blender hata ayrıntıları</summary>
        <pre>{error}</pre>
      </details>}
      <button type="button" className="btn btn-ghost btn-full"
        onClick={()=>void copy(error)}><Clipboard size={14}/> Hata metnini kopyala</button>
    </div>}
    {result&&<div className="uv-repair-result" role="status" data-testid="uv-repair-success">
      <strong>{result.reviewOnly
        ? 'QA FAIL · UV REVIEW — inceleme taslağı arşive eklendi'
        : result.quality?.atlasSelectionTier==='fidelity-tradeoff'
          ? 'Doku öncelikli UV hazır — daha çok UV adası var'
          : <><Check size={14}/> Yeni temiz sürüm arşive eklendi</>}</strong>
      {result.quality?.atlasSelectionTier==='fidelity-tradeoff' &&
        <p role="note" className="mini-disclaimer">
          UV adaları {result.oldIslands} → {result.newIslands} yükseldi.
          Doku ve geometrik gerilme kontrolleri geçti; ancak atlas elle
          düzenleme için kaynak modelden daha parçalıdır. Oyuna aktarmadan
          önce normal/doku kalitesini görsel olarak kontrol et.
        </p>}
      {result.reviewOnly&&<p role="alert" className="unity-export-error">
        Bu atlas PBR/UV kalite kapısını GEÇMEDİ. Blender bake dosyaları kurtarıldı;
        orijinal GLB korundu. Bu taslağın Unity export'u engellendi.
        Oyun için kaynak modeli veya kalite kabulünden geçen yeni bir sürümü kullan.
      </p>
      <span>UV adası: {result.oldIslands} → {result.newIslands} · {result.strategy==='smart-project-fallback'?'Yedek UV yöntemi':'Açı + topoloji ile unwrap'}</span>
      {result.asset.triangleCount!==asset.triangleCount&&
        <p className="mini-disclaimer" role="note">
          Geometri notu: kaynak {asset.triangleCount} → yeni {result.asset.triangleCount} üçgen.
          Bu fark Blender içe aktarımında oluştu; orijinal dosyan korunuyor.
          Unity aktarımından önce temiz modelin yüzeyini görsel olarak kontrol et.
          Kabul yöntemi Repair_Manifest.json dosyasına kaydedildi.
        </p>}
      {result.quality&&<div className="uv-repair-quality" data-testid="uv-repair-quality">
        <strong>{result.quality.passed
          ? '3D yüzey texture sadakati: otomatik kontrolden geçti'
          : 'PBR / UV kalite denetimi başarısız — yalnızca inceleme'}</strong>
        {result.quality.uvStretchGate&&<p className="block-description">
          UV gerilme oranı: P95 ×{result.quality.uvStretchGate.p95Ratio.toFixed(2)},
          maksimum ×{result.quality.uvStretchGate.maximumRatio.toFixed(2)}
          {result.quality.uvStretchGate.passed ? ' — kontrol edildi' : ' — DİKKAT'}
        </p>}
        {Object.values(result.quality.maps).map(map=><div key={map.role} className="meta-row">
          <span>{map.role}</span>
          <strong>Ort. {(map.meanAbsoluteError*100).toFixed(2)}% ·
            P95 {(map.p95AbsoluteError*100).toFixed(1)}% ·
            P99 {map.p99AbsoluteError === undefined ? '—' : (map.p99AbsoluteError*100).toFixed(1)+'%'}</strong>
        </div>)}
        <small>M2.5.1: aynı 3D yüzeyde PBR ortalama/P95/P99 ve UV gerilmesi
          denetlenir. Normal/tangent, renk sınırları ve kuyruk birleşimi
          yine görsel onay ister; numerik test tek başına oyun kabulü değildir.</small>
      </div>}
      {result.meshHealth&&<div className="uv-repair-health">
        <strong>Mesh sağlık kontrolü</strong>
        <span>Açık sınır kenarları: {result.meshHealth.repairedWorking.boundaryEdges} ·
          Non-manifold kenarlar: {result.meshHealth.repairedWorking.nonManifoldEdges}</span>
        <span>Bağlantısız silinen nokta: {result.meshHealth.safeCleanup.removedLooseVertices} ·
          Sıfır alanlı çalışma yüzü: {result.meshHealth.repairedWorking.zeroAreaFaces}</span>
        <span>Üst üste görünen aynı-yönlü yüz adayları: {result.meshHealth.repairedWorking.exactOverlappingFaceCandidates} ·
          Küçük kapalı delik sınırı adayları: {result.meshHealth.repairedWorking.tinyHoleLoopCandidates}</span>
        <small>Bunlar tanı verileridir; otomatik delik doldurma veya görünür
          üst üste binmiş yüzeyleri silme yapılmaz.</small>
      </div>}
      <button type="button" className="btn btn-outline btn-full"
        onClick={()=>void onOpen(result.asset)}>Temiz modeli 3D Inspector’da aç</button>
      <button type="button" className="btn btn-ghost btn-full"
        onClick={()=>void copy(result.workingBlend)}><Clipboard size={14}/> Blender düzenleme dosyasını kopyala</button>
      <button type="button" className="btn btn-ghost btn-full"
        onClick={()=>void copy(result.uvLayout)}><Clipboard size={14}/> UV atlas SVG yolunu kopyala</button>
      {result.uvStressLayout&&<button type="button" className="btn btn-ghost btn-full"
        onClick={()=>void copy(result.uvStressLayout!)}><Clipboard size={14}/> UV gerilme haritası SVG yolunu kopyala</button>}
      <small>Elle UV düzenlemek için çalışma .blend dosyası da saklanır. Düzenlenmiş UV’yi yeniden bake etme, sonraki kabul adımıdır.</small>
    </div>}
  </section>;
}
