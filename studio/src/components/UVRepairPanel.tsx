import { useEffect, useState } from 'react';
import { Check, ChevronDown, Clipboard, Layers3, LoaderCircle, Wrench } from 'lucide-react';
import { bridge } from '../lib/bridge';
import type { Asset, TextureExportSummary, UvRepairResult, UvRepairPreset, RecoverableUvRepair } from '../lib/bridge';

interface Props {
  asset: Asset;
  revision: number;
  canRepair: boolean;
  onCreated: () => Promise<void>;
  onOpen: (asset: Asset) => Promise<void>;
}

export function UVRepairPanel({asset,revision,canRepair,onCreated,onOpen}:Props) {
  const [versions,setVersions] = useState<TextureExportSummary[]>([]);
  const [selected,setSelected] = useState('');
  const [loading,setLoading] = useState(true);
  const [busy,setBusy] = useState(false);
  const [preset,setPreset] = useState<UvRepairPreset>('preserve');
  const [resolution,setResolution] = useState(1024);
  const [seamAngle,setSeamAngle] = useState(62);
  const [weld,setWeld] = useState(true);
  const [advanced,setAdvanced] = useState(false);
  const [result,setResult] = useState<UvRepairResult|null>(null);
  const [error,setError] = useState('');
  const [recoverable,setRecoverable] = useState<RecoverableUvRepair[]>([]);
  const [chosenRecovery,setChosenRecovery] = useState('');

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
    void refreshRecoverable();
    // Asset and export history changes should refresh recoverable runs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  },[asset.id,revision]);

  async function run() {
    if (!selected || busy || !canRepair)return;
    setBusy(true);setError('');setResult(null);
    try {
      const repaired=await bridge.repairUV(asset.id,selected,preset,resolution,seamAngle,weld);
      if (!repaired.sourceUntouched || repaired.newIslands<1 ||
          repaired.newIslands>repaired.oldIslands ||
          repaired.asset.id===asset.id) {
        throw new Error('Onarım kaydı source/UV kabul koşullarını geçmedi.');
      }
      setResult(repaired);
      await onCreated();
    } catch(err) {setError('UV/PBR onarımı: '+String(err));}
    finally{setBusy(false);void refreshRecoverable();}
  }

  const matches = recoverable.filter(a=>a.profile===preset && a.resolution===resolution);
  const recovery = matches.find(a=>a.versionDir===chosenRecovery) ?? matches[0];

  async function resume() {
    if (!selected || !recovery || busy || !canRepair) return;
    setBusy(true);setError('');setResult(null);
    try {
      const repaired=await bridge.resumeUVRepair(
        asset.id,recovery.versionDir,selected,preset,resolution);
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

  async function copy(value:string) {
    try {await navigator.clipboard.writeText(value);}
    catch {setError('Klasör yolu panoya kopyalanamadı.');}
  }

  const enabled=canRepair && asset.materialCount===1 && !!selected && !loading && !busy;
  return <section className="uv-repair-panel" aria-label="Auto UV ve PBR Repair" data-testid="uv-repair-panel">
    <div className="card-heading"><div><Wrench size={16}/><strong>Auto UV + Texture Repair</strong></div></div>
    <p className="block-description">Mesh korunarak daha bütünlüklü UV adaları ve 5 ayrı PBR map yeniden hazırlanır. Kaynak hiç değişmez; yeni model arşive eklenir.</p>
    <label className="unity-export-select"><span>PBR KAYNAĞI</span><div><select
      aria-label="UV Repair PBR sürümü" value={selected} disabled={busy||loading}
      onChange={e=>{setSelected(e.target.value);setResult(null);}}>
      {!versions.length&&<option value="">Önce Texture Export oluştur</option>}
      {versions.map((v,i)=><option key={v.outputDir} value={v.outputDir}>{i===0?'Son · ':''}{v.preset} · {v.mapCount} map</option>)}
    </select><ChevronDown size={13}/></div></label>
    <label className="unity-export-select"><span>DÜZELTME PROFİLİ</span><div><select
      aria-label="UV Repair profil" value={preset} disabled={busy}
      onChange={e=>{setPreset(e.target.value as UvRepairPreset);setResult(null);}}>
      <option value="preserve">Doğal · Kaynak PBR değerlerini koru</option>
      <option value="plush">Peluş / kumaş · Metal=0, Roughness ≥0.82</option>
    </select><ChevronDown size={13}/></div></label>
    <label className="unity-export-select"><span>YENİ TEXTURE BOYUTU</span><div><select
      aria-label="UV Repair çözünürlük" value={resolution} disabled={busy}
      onChange={e=>{setResolution(Number(e.target.value));setResult(null);}}>
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
    <button type="button" className="btn btn-outline btn-full" data-testid="uv-repair-button"
      disabled={!enabled} onClick={()=>void run()}>
      {busy?<LoaderCircle className="spin" size={15}/>:<Layers3 size={15}/>}
      {busy?'Blender UV ve 5 PBR map hazırlıyor…':'Otomatik UV + PBR Onar'}
    </button>
    {matches.length>0&&<div className="uv-repair-recovery" data-testid="uv-repair-recovery">
      <strong><Check size={14}/> Önceki Blender bake dosyaları bulundu</strong>
      <p className="block-description">Mevcut 5 PBR PNG ve UV'yi tekrar üretmeden doğrulayarak yeni asset oluştur. Gerçek bir geometri kaybı varsa işlem yine durur.</p>
      <label className="unity-export-select"><span>KURTARILACAK SÜRÜM</span><div>
        <select aria-label="Önceki UV onarım sürümü" value={recovery?.versionDir??''}
          disabled={busy} onChange={e=>setChosenRecovery(e.target.value)}>
          {matches.map(a=><option key={a.versionDir} value={a.versionDir}>
            {a.versionDir.split(/[\\/]/).pop()?.slice(0,8)} · UV {a.oldIslands} → {a.newIslands} · {a.inputTriangles} tris
          </option>)}
        </select><ChevronDown size={13}/></div></label>
      <button type="button" data-testid="uv-resume-button" className="btn btn-outline btn-full"
        disabled={!enabled} onClick={()=>void resume()}>
        {busy?<LoaderCircle className="spin" size={15}/>:<Layers3 size={15}/>}
        Önceki bake'i tamamla (Blender çalışmaz)
      </button>
    </div>}
    {!versions.length&&!loading&&<p className="mini-disclaimer">Önce yukarıdan 5 PBR haritasını dışa aktar; ardından onarım tek işlemde çalışır.</p>}
    {asset.materialCount!==1&&<p className="mini-disclaimer">v1 yalnız tek materyalli statik ürünlerde çalışır.</p>}
    {!canRepair&&<p className="mini-disclaimer">LOD önizlemesinden SRC görünümüne dön.</p>}
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
      <strong><Check size={14}/> Yeni temiz sürüm arşive eklendi</strong>
      <span>UV adası: {result.oldIslands} → {result.newIslands} · {result.strategy==='smart-project-fallback'?'Yedek UV yöntemi':'Açı + topoloji ile unwrap'}</span>
      <button type="button" className="btn btn-outline btn-full"
        onClick={()=>void onOpen(result.asset)}>Temiz modeli 3D Inspector’da aç</button>
      <button type="button" className="btn btn-ghost btn-full"
        onClick={()=>void copy(result.workingBlend)}><Clipboard size={14}/> Blender düzenleme dosyasını kopyala</button>
      <button type="button" className="btn btn-ghost btn-full"
        onClick={()=>void copy(result.uvLayout)}><Clipboard size={14}/> UV atlas SVG yolunu kopyala</button>
      <small>Elle UV düzenlemek için çalışma .blend dosyası da saklanır. Düzenlenmiş UV’yi yeniden bake etme, sonraki kabul adımıdır.</small>
    </div>}
  </section>;
}
