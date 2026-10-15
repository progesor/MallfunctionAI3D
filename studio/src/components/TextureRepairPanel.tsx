import { useEffect, useState } from 'react';
import { Check, Clipboard, Eye, LoaderCircle, ShieldCheck, Wrench } from 'lucide-react';
import { bridge } from '../lib/bridge';
import type {
  Asset, TextureExportSummary, SurfaceInspection, SurfaceRepairResult,
  SurfaceRepairMode, SeamEditableRole,
} from '../lib/bridge';

interface Props {
  asset: Asset;
  revision: number;
  canRepair: boolean;
  onCreated: () => Promise<void>;
  onOpen: (asset:Asset) => Promise<void>;
}
const ROLES: {role:SeamEditableRole;label:string}[]=[
  {role:'baseColor',label:'Base Color'},
  {role:'metallic',label:'Metallic'},
  {role:'roughness',label:'Roughness'},
  {role:'occlusion',label:'Occlusion (AO)'},
];
export function TextureRepairPanel({asset,revision,canRepair,onCreated,onOpen}:Props) {
  const [expanded,setExpanded]=useState(true);
  const [versions,setVersions]=useState<TextureExportSummary[]>([]);
  const [selected,setSelected]=useState('');
  const [mode,setMode]=useState<SurfaceRepairMode>('gentle');
  const [radius,setRadius]=useState(2);
  const [roles,setRoles]=useState<SeamEditableRole[]>(ROLES.map(x=>x.role));
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const [analysis,setAnalysis]=useState<SurfaceInspection|null>(null);
  const [result,setResult]=useState<SurfaceRepairResult|null>(null);
  const [review,setReview]=useState(false);
  const [approval,setApproval]=useState(false);
  const [approved,setApproved]=useState<Asset|null>(null);
  const [parent,setParent]=useState<Asset|null>(null);

  useEffect(()=>{
    let alive=true;
    setVersions([]);setSelected('');setError('');setAnalysis(null);
    setResult(null);setReview(false);setApproval(false);setApproved(null);setParent(null);
    void Promise.all([
      bridge.textureExports(asset.id).catch(()=>[]),
      bridge.assetManifest(asset.id).catch(()=>null),
    ]).then(async([rows,manifest])=>{
      if(!alive)return;
      setVersions(rows);setSelected(rows[0]?.outputDir??'');
      const provenance=(manifest?.provenance??null) as Record<string,unknown>|null;
      if(provenance?.origin==='texture-seam-repair'&&
         provenance?.reviewOnly===true) {
        setReview(true);
        const list=await bridge.assets().catch(()=>[]);
        if(alive)setParent(list.find(a=>a.id===provenance.parentAssetId)??null);
      }
    }).catch(err=>{if(alive)setError(String(err));});
    return()=>{alive=false;};
  },[asset.id,revision]);

  const ready=canRepair && asset.materialCount===1 && !!selected && !busy;
  const applicable=ready && !!analysis && analysis.editablePixels>0 && roles.length>0 &&
    roles.some(role=>analysis.maps.some(m=>m.role===role&&m.changedPixels>0));

  async function analyze(){
    if(!ready)return;
    setBusy(true);setAnalysis(null);setResult(null);setError('');
    try {
      const info=await bridge.surfaceAnalyze(asset.id,selected,mode,radius);
      if(!info.sourceUntouched||!info.uvUnchanged||!info.sourceNormalUnchanged)
        throw new Error('Mesh, UV0 veya kaynak normal koruma denetimi başarısız.');
      setAnalysis(info);
    }catch(e){setError('Dikiş analizi: '+String(e));}
    finally{setBusy(false);}
  }
  async function repair(){
    if(!applicable)return;
    setBusy(true);setResult(null);setError('');
    try{
      const out=await bridge.surfaceRepair(asset.id,selected,mode,radius,roles);
      if(!out.sourceUntouched||!out.uvUnchanged||!out.sourceNormalUnchanged||
         !out.reviewOnly||out.asset.id===asset.id)
        throw new Error('Texture Repair sonuç güvenlik doğrulaması başarısız.');
      setResult(out);
      await onCreated();
    }catch(e){setError('Dikiş onarımı: '+String(e));}
    finally{setBusy(false);}
  }
  async function confirm(){
    if(!review||!approval||busy)return;
    setBusy(true);setError('');
    try {
      const next=await bridge.surfaceApprove(asset.id,true);
      if(next.id===asset.id)throw new Error('Yeni varlık kimliği oluşturulmadı.');
      setApproved(next);await onCreated();
    }catch(e){setError('Görsel onay: '+String(e));}
    finally{setBusy(false);}
  }
  async function copyReport(){
    if(!result)return;
    try{await navigator.clipboard.writeText(result.reportPath);}
    catch{setError('Rapor yolu kopyalanamadı.');}
  }
  function toggle(role:SeamEditableRole,checked:boolean){
    setRoles(prev=>checked?[...prev.filter(r=>r!==role),role]:prev.filter(r=>r!==role));
  }
  return <section className="texture-surface-panel" data-testid="texture-surface-panel">
    <button type="button" className="uv-repair-advanced" aria-expanded={expanded}
      onClick={()=>setExpanded(x=>!x)}>
      <Wrench size={15}/> <strong>3D Texture &amp; Mesh Health</strong>
      {expanded?' −':' +'}
    </button>
    {expanded&&<>
      <p className="block-description">
        Orijinal UV0, geometri ve Normal map korunur.
        Yalnız aynı 3D yüzey kenarının iki UV adası yakınındaki küçük PBR farkları
        yumuşatılır. Büyük farklar ve keskin birleşimler otomatik atlanır.
        Mesh sağlık analizi salt okunurdur; delik doldurma/yüz silme yapılmaz.
      </p>
      {review?<div className="texture-surface-review" data-testid="texture-seam-review">
        <p role="alert" className="unity-export-error">
          <strong>TEXTURE REVIEW — Unity export engellendi.</strong>
          Yeni model ve orijinal doku/normal görünümünü karşılaştırmalısın.
        </p>
        {parent&&<button type="button" className="btn btn-ghost btn-full"
          onClick={()=>void onOpen(parent)}><Eye size={14}/> Orijinal modeli aç</button>}
        <label className="uv-repair-toggle">
          <input type="checkbox" aria-label="Görsel karşılaştırmayı tamamladım"
            checked={approval} disabled={busy||!!approved}
            onChange={e=>setApproval(e.target.checked)}/>
          <span>Kaynak ile onarılan modelin renk/roughness/metal/AO görünümünü
            karşılaştırdım ve üretim onayı veriyorum.</span>
        </label>
        {approved?
          <button className="btn btn-outline btn-full" type="button"
            onClick={()=>void onOpen(approved)}><Check size={14}/> Onaylı modeli aç</button>:
          <button type="button" className="btn btn-outline btn-full"
            data-testid="texture-approve-button" disabled={!approval||busy}
            onClick={()=>void confirm()}>
            {busy?<LoaderCircle className="spin" size={14}/>:<ShieldCheck size={14}/>}
            Görsel onayla · yeni üretim modeli oluştur
          </button>}
      </div>:<>
        <label className="unity-export-select"><span>PBR KAYNAĞI</span>
          <div><select aria-label="Texture Repair PBR kaynağı" value={selected}
            disabled={busy} onChange={e=>{
              setSelected(e.target.value);setAnalysis(null);setResult(null);
            }}>
            {!versions.length&&<option value="">Önce Texture Export oluştur</option>}
            {versions.map(v=><option key={v.outputDir} value={v.outputDir}>
              {v.preset} · {v.mapCount} harita</option>)}
          </select></div>
        </label>
        <div className="texture-surface-settings">
          <label className="unity-export-select"><span>GÜÇ</span><div>
            <select aria-label="Dikiş onarım gücü" value={mode} disabled={busy}
              onChange={e=>{setMode(e.target.value as SurfaceRepairMode);setAnalysis(null);}}>
              <option value="gentle">Nazik</option>
              <option value="balanced">Dengeli</option>
            </select></div></label>
          <label className="unity-export-select"><span>ŞERİT</span><div>
            <select aria-label="Dikiş şerit genişliği" value={radius} disabled={busy}
              onChange={e=>{setRadius(Number(e.target.value));setAnalysis(null);}}>
              <option value={1}>1 px</option>
              <option value={2}>2 px</option>
              <option value={3}>3 px</option>
            </select></div></label>
        </div>
        <button type="button" className="btn btn-outline btn-full"
          data-testid="texture-seam-analyze" disabled={!ready}
          onClick={()=>void analyze()}>
          {busy?<LoaderCircle className="spin" size={15}/>:<Eye size={15}/>}
          3D dikiş analizi (yalnız oku)
        </button>
        {analysis&&<div className="texture-surface-diagnostics" data-testid="texture-seam-metrics">
          <strong>UV dikişleri ve mesh sağlığı</strong>
          <p>{analysis.geometry.triangles.toLocaleString('tr-TR')} üçgen ·
            {analysis.eligibleSeamPairs.toLocaleString('tr-TR')} yumuşak eşleşen UV kenarı</p>
          <p>Açık kenar: {analysis.geometry.openBoundaryEdges} ·
            Keskin dikiş atlandı: {analysis.geometry.intentionalSharpEdgesSkipped} ·
            Non-manifold atlandı: {analysis.geometry.nonManifoldEdgesSkipped}</p>
          <p>Öngörülen değişiklik: <strong>{analysis.editablePixels.toLocaleString('tr-TR')} piksel</strong></p>
          <div className="texture-surface-maps">
            {ROLES.map(item=>{
              const metric=analysis.maps.find(m=>m.role===item.role);
              return <label key={item.role} className="texture-surface-role">
                <input type="checkbox" disabled={busy} checked={roles.includes(item.role)}
                  onChange={e=>toggle(item.role,e.target.checked)}/>
                <span>{item.label}</span>
                <small>{metric?String(metric.changedPixels)+' px · '+
                  (metric.changedFraction*100).toFixed(2)+'%':'—'}</small>
              </label>;
            })}
          </div>
          <p className="mini-disclaimer">
            Normal map hiç düzenlenmez. Büyük farklar atlanır; UV dışındaki
            genel doku bozukluklarını bu araç gidermez.
          </p>
          <button type="button" className="btn btn-outline btn-full"
            data-testid="texture-seam-apply" disabled={!applicable}
            onClick={()=>void repair()}>
            {busy?<LoaderCircle className="spin" size={15}/>:<Wrench size={15}/>}
            Yeni Texture REVIEW modeli oluştur
          </button>
        </div>}
        {result&&<div className="texture-surface-result" data-testid="texture-seam-result">
          <strong><Check size={14}/> İnceleme taslağı hazır</strong>
          <p>UV/vertex/normal verileri korunmuş ayrı bir GLB oluşturuldu.
            Unity export, görsel onay verilene kadar kapalı.</p>
          <button type="button" className="btn btn-outline btn-full"
            onClick={()=>void onOpen(result.asset)}>
            <Eye size={14}/> Düzeltilen modeli 3D görüntüle
          </button>
          <button type="button" className="btn btn-ghost btn-full"
            onClick={()=>void copyReport()}>
            <Clipboard size={14}/> Rapor yolunu kopyala
          </button>
        </div>}
        {!canRepair&&<p className="mini-disclaimer">Önce orijinal SRC görünümünü seç.</p>}
      </>}
      {error&&<p className="unity-export-error" role="alert">{error}</p>}
    </>}
  </section>;
}
