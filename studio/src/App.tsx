import { useCallback, useEffect, useState } from 'react';
import {
  Activity, Archive, ArrowDownToLine, ArrowUpRight, Box, Boxes, Check, ChevronDown, ChevronRight,
  CircleAlert, CircleCheck, CloudCog, Command, Cpu, Database, FileBox, FolderOpen, Gauge, Image as ImageIcon,
  Layers3, LoaderCircle, Monitor, PackageCheck, Plus, RotateCcw, Save, Settings2, Upload,
  SlidersHorizontal, Sparkles, SquareStack, WandSparkles, X, Trash2, ShieldCheck, ImagePlus, AlertTriangle,
  PanelLeftClose, PanelLeftOpen, PanelRightClose, PanelRightOpen,
} from 'lucide-react';
import { bridge, formatBytes, isTauri, shortDate } from './lib/bridge';
import type { Asset, Environment, GpuStats, JobStatus, LodResult, Settings, WorkflowDescriptor } from './lib/bridge';
import { DEFAULT_OPTIONS } from './lib/workflow';
import { assertInstalledNodes, prepareApiProduction, productionGraphChanges, validateApiProduction } from './lib/api-workflow';
import type { GenerateOptions } from './lib/workflow';
import type { ProductionMode } from './lib/api-workflow';
import { STUDIO_MODULES } from './lib/modules';
import type { ModuleKind } from './lib/modules';
import { ModelViewport } from './components/ModelViewport';
import { TextureInspector } from './components/TextureInspector';
import { UnityExportPanel } from './components/UnityExportPanel';
import { UVRepairPanel } from './components/UVRepairPanel';
import './styles/app.css';

const initialSettings: Settings = {
  comfyUrl: 'http://127.0.0.1:8188', blenderPath: '', station: 'local', libraryRoot: '',
};
const tabIcons = { generation: WandSparkles, inspection: Box, library: Archive,
  optimization: SlidersHorizontal, texture: Layers3, image: ImageIcon, export: PackageCheck };

function SettingDialog({ current, onSave, onClose }: { current: Settings; onSave: (settings: Settings) => Promise<void>; onClose: () => void }) {
  const [draft, setDraft] = useState<Settings>(current);
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  async function save() {
    setSaving(true); setError('');
    try { await onSave(draft); onClose(); }
    catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  }
  return <div className="modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
    <div className="modal" role="dialog" aria-modal="true" aria-label="Studio settings">
      <div className="modal-header"><div><span className="eyebrow">LOCAL ENVIRONMENT</span><h2>Studio Ayarları</h2></div>
        <button type="button" className="icon-button" onClick={onClose} aria-label="Kapat"><X size={18} /></button></div>
      <p className="modal-desc">Ayarlar bu bilgisayarda tutulur. Mevcut ComfyUI, Blender ve model önbelleği değiştirilmez.</p>
      <label className="field"><span>ComfyUI API adresi</span><input value={draft.comfyUrl} onChange={(e) => setDraft({ ...draft, comfyUrl: e.target.value })} placeholder="http://127.0.0.1:8188" /></label>
      <label className="field"><span>İstasyon kimliği</span><input value={draft.station} onChange={(e) => setDraft({ ...draft, station: e.target.value })} placeholder="4070ti-super veya 5080" /></label>
      <label className="field"><span>Blender executable</span><div className="field-with-button"><input value={draft.blenderPath} onChange={(e) => setDraft({ ...draft, blenderPath: e.target.value })} placeholder="Kurulum yolu otomatik aranır" /><button type="button" onClick={async () => {const p = await bridge.pickBlender(); if (typeof p === 'string') setDraft({ ...draft, blenderPath: p });}}>Seç</button></div></label>
      <label className="field"><span>Asset arşiv klasörü</span><div className="field-with-button"><input value={draft.libraryRoot} onChange={(e) => setDraft({ ...draft, libraryRoot: e.target.value })} placeholder="Varsayılan: uygulama veri klasörü" /><button type="button" onClick={async () => {const p = await bridge.pickArchive(); if (typeof p === 'string') setDraft({ ...draft, libraryRoot: p });}}>Seç</button></div></label>
      {error && <div className="error-message"><CircleAlert size={16} /> {error}</div>}
      <div className="modal-footer"><button className="btn btn-ghost" type="button" onClick={onClose}>Vazgeç</button><button type="button" className="btn btn-primary" disabled={saving} onClick={save}>{saving ? <LoaderCircle className="spin" size={16} /> : <Save size={16} />} Kaydet</button></div>
    </div>
  </div>;
}

function Metric({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return <div className="metric"><span className="metric-label">{label}</span><strong>{value}</strong>{sub && <small>{sub}</small>}</div>;
}

function App() {
  const [active, setActive] = useState<ModuleKind>('generation');
  const [environment, setEnvironment] = useState<Environment | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [viewExpanded, setViewExpanded] = useState(false);
  const [toolsCollapsed, setToolsCollapsed] = useState(false);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(() => typeof window !== 'undefined' && window.innerWidth < 1120);
  const [debugInfo, setDebugInfo] = useState('');
  const [connection, setConnection] = useState<'checking' | 'online' | 'offline'>('checking');
  const [gpu, setGpu] = useState<GpuStats | null>(null);
  const [workflows, setWorkflows] = useState<WorkflowDescriptor[]>([]);
  const [workflowId, setWorkflowId] = useState('pixal3d-fox');
  const [imagePath, setImagePath] = useState('');
  const [referencePreview, setReferencePreview] = useState('');
  const [referenceError, setReferenceError] = useState('');
  const [productionMode, setProductionMode] = useState<ProductionMode>('source');
  const [overrideSeeds, setOverrideSeeds] = useState(false);
  const [sourceSeeds, setSourceSeeds] = useState('');
  const [assetPendingDeletion, setAssetPendingDeletion] = useState<Asset | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [options, setOptions] = useState<GenerateOptions>({ ...DEFAULT_OPTIONS });
  const [busy, setBusy] = useState(false);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [selectedAsset, setSelectedAsset] = useState<Asset | null>(null);
  const [reviewOnlyAsset, setReviewOnlyAsset] = useState(false);
  const [glb, setGlb] = useState<string | null>(null);
  const [previewLabel, setPreviewLabel] = useState('');
  const [viewLoading, setViewLoading] = useState(false);
  const [lod, setLod] = useState<LodResult | null>(null);
  const [lodLoading, setLodLoading] = useState(false);
  const [textureExportRevision, setTextureExportRevision] = useState(0);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [search, setSearch] = useState('');

  const refresh = useCallback(async () => { setAssets(await bridge.assets()); }, []);
  const checkConnection = useCallback(async () => {
    setConnection('checking');
    try { await bridge.health(); setConnection('online'); }
    catch { setConnection('offline'); }
  }, []);
  useEffect(() => {
    if (!isTauri()) { setConnection('offline'); setError('Masaüstü komutları için Tauri uygulamasını çalıştır. npm run dev yalnızca önizleme sağlar.'); return; }
    void Promise.all([bridge.environment(), bridge.workflows(), bridge.assets()])
      .then(([env, items, library]) => {setEnvironment(env);setWorkflows(items);setAssets(library);})
      .catch((e) => setError(String(e)));
    void checkConnection();
  }, [checkConnection]);
  useEffect(() => {
    if (!isTauri()) return;
    const update = () => { void bridge.gpu().then(setGpu).catch(() => setGpu(null)); };
    update(); const timer = window.setInterval(update, 3500);
    return () => window.clearInterval(timer);
  }, []);
  useEffect(() => {
    if (!job || ['done', 'failed', 'no_output'].includes(job.status)) return;
    let stopped = false; let inFlight = false;
    const poll = async () => {
      if (inFlight) return; inFlight = true;
      try {
        const current = await bridge.pollJob(job.promptId);
        if (!stopped) {
          setJob(current);
          if (current.asset) {
            await refresh();
            if (!stopped) {await showAsset(current.asset);setNotice('GLB otomatik arşivlendi ve önizlemeye alındı.');}
          }
        }
      } catch (e) { if (!stopped) setError(`ComfyUI durum izleme: ${String(e)}`); }
      finally { inFlight = false; }
    };
    void poll(); const interval = window.setInterval(() => void poll(), 2000);
    return () => { stopped = true; window.clearInterval(interval); };
    // showAsset is intentionally referenced through current render; polling bound to promptId
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [job?.promptId, job?.status, refresh]);

  // Tauri accesses only the specifically chosen local image; never expose an
  // unrestricted local-file browser or open arbitrary file:// URLs in WebView.
  useEffect(() => {
    let live=true;
    setReferencePreview('');setReferenceError('');
    if (!imagePath || !isTauri()) return () => {live=false;};
    void bridge.referencePreview(imagePath).then(data=>{if(live)setReferencePreview(data);})
      .catch(err=>{if(live)setReferenceError(String(err));});
    return () => {live=false;};
  }, [imagePath]);
  useEffect(() => {
    let live=true;
    if (!isTauri()) return;
    void bridge.workflow(workflowId).then(doc=>{
      if (!live || doc.format!=='api')return;
      const values=['3','12','18','23'].map(id=>String(doc.prompt[id]?.inputs?.seed ?? '—'));
      setSourceSeeds(values.join(' / '));
    }).catch(()=>{if(live)setSourceSeeds('—');});
    return () => {live=false;};
  },[workflowId,workflows]);

  useEffect(() => {
    if (!viewExpanded) return;
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setViewExpanded(false);
    };
    window.addEventListener('keydown', handleEscape);
    return () => window.removeEventListener('keydown', handleEscape);
  }, [viewExpanded]);

  const connected = connection === 'online';
  const stationIsMismatched = !!gpu && !!environment && (
    (gpu.name.includes('5080') && environment.settings.station.includes('4070')) ||
    (gpu.name.includes('4070') && environment.settings.station.includes('5080'))
  );
  const currentWorkflow = workflows.find((w) => w.id === workflowId);
  const filteredAssets = assets.filter((asset) => asset.title.toLowerCase().includes(search.toLowerCase()) || asset.engine.toLowerCase().includes(search.toLowerCase()));

  async function showAsset(asset: Asset, lodIndex?: number) {
    setViewLoading(true);setError('');
    try {
      const [data, report, manifest] = await Promise.all([
        lodIndex === undefined ? bridge.assetBinary(asset.id) : bridge.lodBinary(asset.id, lodIndex),
        bridge.lodReport(asset.id),
        bridge.assetManifest(asset.id).catch(()=>null),
      ]);
      const provenance=(manifest?.provenance ?? null) as Record<string,unknown> | null;
      setReviewOnlyAsset(provenance?.reviewOnly === true);
      setSelectedAsset(asset);setGlb(data);setLod(report);setPreviewLabel(`${asset.title}${lodIndex === undefined ? ' / Source' : ` / LOD${lodIndex}`}`);
      setActive('inspection');
    } catch (e) { setError(`GLB görüntülenemedi: ${String(e)}`); }
    finally {setViewLoading(false);}
  }

  async function runProduction() {
    setError('');setNotice('');
    if (!imagePath) {setError('Önce referans görsel seç.');return;}
    if (!connected) {setError('ComfyUI bağlantısı kapalı. Ayarlardaki adresi kontrol et.');return;}
    setBusy(true);
    try {
      const document = await bridge.workflow(workflowId);
      if (document.format !== 'api') throw new Error('Bu M1 sürümü doğrudan ComfyUI API Export JSON gerektiriyor.');
      const original = document.prompt;
      const graphInfo = validateApiProduction(original);
      const nodeInfo = await bridge.nodeInfo();
      assertInstalledNodes(original, nodeInfo);
      const uploadedName = await bridge.uploadImage(imagePath);
      const mode = currentWorkflow?.family === 'TRELLIS.2' ? 'trellis2' : 'pixal3d';
      const prompt = prepareApiProduction(original, options, uploadedName, mode,
        { mode: productionMode, overrideAllSeeds: productionMode === 'custom' && overrideSeeds });
      const changes=productionGraphChanges(original,prompt);
      // Source mode must never mutate a sampler, switch or geometry parameter.
      if (productionMode === 'source' && (changes.length!==1 || changes[0]!=='#122.image')) {
        throw new Error(`Kaynak workflow korunamadı: ${changes.join(', ')}`);
      }
      const result = await bridge.submit({ workflowId, imagePath, apiPrompt: prompt, uiWorkflow: original,
        config: { ...options, uploadedName, format: 'api', includedNodes: graphInfo.nodeCount,
          productionMode, overrideSeeds: productionMode === 'custom' && overrideSeeds,
          changedInputs: changes, gpu:gpu?.name ?? null, station: environment?.settings.station } });
      setJob(result);setNotice(`ComfyUI kuyruğa aldı: ${result.promptId.slice(0, 8)}…`);
    } catch (e) {setError(`Üretim başlatılamadı: ${String(e)}`);}
    finally {setBusy(false);}
  }

  async function importWorkflow() {
    setError('');setNotice('');
    try {
      const file=await bridge.pickApiWorkflow();
      if (typeof file !== 'string') return;
      const imported=await bridge.importApiWorkflow(file);
      setWorkflows(await bridge.workflows());
      setWorkflowId(imported.id);
      setNotice(`API workflow eklendi: ${imported.title}. Orijinal dosya korunuyor.`);
      setActive('generation');
    } catch (e) {setError(`API workflow içe aktarılamadı: ${String(e)}`);}
  }

  async function showJobDiagnostics() {
    if (!job) return;
    try {
      const report=await bridge.jobDiagnostics(job.promptId);
      setDebugInfo(JSON.stringify(report,null,2));
    } catch(e) {setError(`Tanı kaydı alınamadı: ${String(e)}`);}
  }

  async function importGlb() {
    try {
      const file = await bridge.pickGlb(); if (typeof file !== 'string') return;
      setBusy(true); const asset = await bridge.importGlb(file);
      await refresh();await showAsset(asset);
      setNotice('GLB kaynak arşive kopyalandı. Orijinal dosya değişmedi.');
    } catch (e) {setError(`İçe aktarma hatası: ${String(e)}`);}
    finally {setBusy(false);}
  }

  async function deleteAsset() {
    if (!assetPendingDeletion) return;
    setDeleteBusy(true);setError('');setNotice('');
    try {
      const path=await bridge.trashAsset(assetPendingDeletion.id);
      if (selectedAsset?.id === assetPendingDeletion.id) {
        setSelectedAsset(null);setGlb(null);setLod(null);setPreviewLabel('');
      }
      setAssetPendingDeletion(null);
      await refresh();
      setNotice(`Model arşivden kaldırıldı. Studio çöp arşivinde korunuyor: ${path}`);
    } catch(e) { setError(`Model kaldırılamadı: ${String(e)}`); }
    finally {setDeleteBusy(false);}
  }

  async function generateLods() {
    if (!selectedAsset) return;
    setLodLoading(true);setError('');setNotice('');
    try {
      const result = await bridge.generateLods(selectedAsset.id);
      setLod(result);setNotice('Blender üç LOD modelini arşive kaydetti. Source GLB korunuyor.');
    } catch(e) {setError(`Blender işlemi tamamlanamadı: ${String(e)}`);}
    finally {setLodLoading(false);}
  }

  async function saveSettings(settings:Settings) {
    await bridge.saveSettings(settings);
    setEnvironment(await bridge.environment());
    await checkConnection();
    setNotice('Bu istasyonun ayarları kaydedildi.');
  }

  return <div className={`app-shell ${viewExpanded ? 'focus-mode' : ''}`}>
    <aside className="sidebar">
      <div className="brand"><div className="brand-symbol"><Box size={23}/><i/></div><div><strong>AI3D <em>STUDIO</em></strong><span>MALLFUNCTION / LOCAL</span></div></div>
      <div className="sidebar-section-title">WORKSPACE</div>
      <nav className="main-nav" aria-label="Studio modules">
        {STUDIO_MODULES.map((mod) => {
          const Icon = tabIcons[mod.id];
          return <button type="button" key={mod.id} disabled={!mod.enabled} title={mod.description} onClick={() => setActive(mod.id)}
            className={`nav-item ${active === mod.id ? 'active' : ''}`}><Icon size={18} /><span>{mod.label}</span>{!mod.enabled ? <small>SOON</small> : active === mod.id ? <ChevronRight size={15}/> : null}</button>;
        })}
      </nav>
      <div className="sidebar-bottom">
        <div className="sidebar-section-title">LOCAL ENGINE</div>
        <div className="connection-card"><div className={`status-dot ${connection}`}/><div><strong>ComfyUI Desktop</strong><span>{connected ? 'API connected' : connection === 'checking' ? 'Checking connection' : 'Disconnected'}</span></div><button onClick={() => void checkConnection()} title="Bağlantıyı yenile" type="button"><RotateCcw size={15}/></button></div>
        <div className="station-info"><Monitor size={15}/><span>{environment?.settings.station || 'Local workstation'}</span></div>
        {stationIsMismatched && environment && <button type="button" className="station-fix" onClick={() => void saveSettings({
          ...environment.settings, station:gpu?.name.includes('5080') ? '5080' : '4070ti-super',
        })}>GPU etiketi farklı · Düzelt</button>}
        <button type="button" className="settings-action" onClick={() => setSettingsOpen(true)}><Settings2 size={18}/> Settings <ChevronRight size={15}/></button>
      </div>
    </aside>

    <main className="workspace">
      <header className="topbar">
        <div className="breadcrumb"><span>STUDIO</span><ChevronRight size={13}/><strong>{STUDIO_MODULES.find((m) => m.id === active)?.label}</strong></div>
        <div className="top-actions"><span className={`top-connection ${connected ? 'is-connected' : ''}`}><span className="status-dot"/>{connected ? 'ComfyUI bağlı' : connection === 'checking' ? 'Bağlanıyor' : 'ComfyUI çevrimdışı'}</span>
          <span className="top-station"><Cpu size={14}/>{gpu?.name ?? environment?.settings.station ?? 'Yerel istasyon'}</span>
          <button className="top-icon" type="button" onClick={() => setSettingsOpen(true)} title="Studio ayarları" aria-label="Studio ayarları"><Settings2 size={17}/></button></div>
      </header>
      <div className="workspace-title">
        <div className="workspace-heading"><span className="workspace-eyebrow">ASSET PRODUCTION WORKSPACE</span>
          <div className="workspace-title-line"><h1>{active === 'library' ? 'Model Arşivi' : active === 'optimization' ? 'Blender & LOD' : active === 'inspection' ? '3D Inspector' : '3D Production'}</h1>
            <span className="workspace-crumb">ComfyUI <ChevronRight size={12}/> Blender <ChevronRight size={12}/> Library</span></div>
        </div>
        <div className="workspace-controls">
          <button type="button" className={`toolbar-toggle ${toolsCollapsed ? 'is-active' : ''}`} onClick={() => setToolsCollapsed(value => !value)} aria-label={toolsCollapsed ? 'Üretim panelini göster' : 'Üretim panelini gizle'} aria-pressed={toolsCollapsed} title={toolsCollapsed ? 'Sol paneli göster' : 'Sol paneli gizle'}>{toolsCollapsed ? <PanelLeftOpen size={17}/> : <PanelLeftClose size={17}/>}</button>
          <button type="button" className="btn btn-outline workspace-import" onClick={() => void importGlb()} disabled={busy}><Plus size={16}/> GLB İçe Aktar</button>
          <button type="button" className={`toolbar-toggle ${inspectorCollapsed ? 'is-active' : ''}`} onClick={() => setInspectorCollapsed(value => !value)} aria-label={inspectorCollapsed ? 'Asset Inspector panelini göster' : 'Asset Inspector panelini gizle'} aria-pressed={inspectorCollapsed} title={inspectorCollapsed ? 'Sağ paneli göster' : 'Sağ paneli gizle'}>{inspectorCollapsed ? <PanelRightOpen size={17}/> : <PanelRightClose size={17}/>}</button>
        </div>
      </div>
      {error && <div className="banner banner-error" role="alert"><CircleAlert size={17}/><span>{error}</span><button aria-label="Hata bildirimini kapat" onClick={() => setError('')} type="button"><X size={17}/></button></div>}
      {notice && <div className="banner banner-success" role="status"><CircleCheck size={17}/><span>{notice}</span><button aria-label="Bildirimi kapat" onClick={() => setNotice('')} type="button"><X size={17}/></button></div>}
      <div className={`main-grid ${viewExpanded ? 'inspector-expanded' : ''} ${toolsCollapsed ? 'hide-tools' : ''} ${inspectorCollapsed ? 'hide-inspector' : ''}`}>
        <section className="tools-column">
          {active === 'generation' ? <>
            <div className="panel-heading"><div><span className="panel-id">01</span><h2>Üretim Ayarları</h2></div><Sparkles size={17}/></div>
            <div className="panel-content">
              <label className="field"><span>WORKFLOW ENGINE</span><div className="select-wrap"><select value={workflowId} onChange={(e) => setWorkflowId(e.target.value)}>{workflows.length ? workflows.map(w => <option value={w.id} key={w.id}>{w.title}</option>) : <option value="pixal3d-fox">Pixal3D · Image → PBR Model</option>}</select><ChevronDown size={15}/></div></label>
              <div className="help-text">{currentWorkflow?.description || 'ComfyUI API grafı, yerel modeller, PBR bake.'}</div>
              <button type="button" className="workflow-import" onClick={() => void importWorkflow()}><Upload size={14}/> ComfyUI API JSON İçe Aktar</button>
              <div className="field-label">REFERENCE IMAGE</div>
              <button type="button" className={`image-drop image-preview-picker ${imagePath ? 'has-file' : ''}`} onClick={async () => { const f = await bridge.pickImage(); if (typeof f === 'string') setImagePath(f); }}>
                {referencePreview
                  ? <img className="reference-thumbnail" src={referencePreview} alt="Seçili 3D üretim referansı" />
                  : <div className="image-drop-icon"><ImagePlus size={27}/></div>}
                <span className="image-pick-caption"><strong>{imagePath ? imagePath.split(/[\\/]/).at(-1) : 'Referans görsel seç'}</strong>
                  <small>{imagePath ? 'Değiştirmek için tıkla' : 'PNG · JPG · WEBP'}</small></span>
              </button>
              {referenceError && <p className="reference-error">Önizleme açılamadı: {referenceError}</p>}
              <div className="separator" />
              <div className="field-label">ÜRETİM YÖNTEMİ</div>
              <div className="production-mode-toggle" role="group" aria-label="Üretim modu">
                <button type="button" className={productionMode==='source'?'mode-choice active':'mode-choice'} onClick={()=>setProductionMode('source')} aria-pressed={productionMode==='source'}><ShieldCheck size={15}/> Workflow aynen</button>
                <button type="button" className={productionMode==='custom'?'mode-choice active':'mode-choice'} onClick={()=>setProductionMode('custom')} aria-pressed={productionMode==='custom'}><SlidersHorizontal size={15}/> Özel ayarlar</button>
              </div>
              {productionMode==='source' ?
                <div className="source-mode-info"><CircleCheck size={15}/><div><strong>ComfyUI ile eşdeğer üretim</strong><span>Orijinal parametreler, tüm KSampler seed'leri ve çıktı düğümleri korunur. Yalnızca seçilen görselin ComfyUI yükleme adı değişir.</span>{sourceSeeds && <code>KSampler #3 / #12 / #18 / #23: {sourceSeeds}</code>}</div></div>
                : <div className="custom-mode-warning"><AlertTriangle size={15}/> Bu ayarlar kaynak workflow'u değiştirir. Sonuç/üretim süresi ComfyUI'deki orijinalden farklı olabilir.</div>}
              {productionMode==='custom' && <>
              <div className="separator" />
              <div className="field-label">GAME ASSET PRESET</div>
              <div className="preset-grid"><button type="button" className={options.triangles === 8000 && options.textureSize === 1024 ? 'preset active' : 'preset'} onClick={() => setOptions({...DEFAULT_OPTIONS})}><SquareStack size={15}/> Game</button>
                <button type="button" className={options.triangles === 20000 ? 'preset active' : 'preset'} onClick={() => setOptions({...DEFAULT_OPTIONS,triangles:20000,textureSize:2048})}><Layers3 size={15}/> Detail*</button></div>
              <div className="detail-warning">* Detail preset daha fazla VRAM kullanabilir; RTX 4070/5080'de doğrulanmadı.</div>
              <div className="fields-two">
                <label className="field"><span>TRIANGLE TARGET</span><input type="number" min={1000} max={80000} step={500} value={options.triangles} onChange={(e) => setOptions({...options,triangles:Number(e.target.value)})}/></label>
                <label className="field"><span>TEXTURE SIZE</span><select value={options.textureSize} onChange={(e) => setOptions({...options,textureSize:Number(e.target.value)})}><option value={1024}>1024 px</option><option value={2048}>2048 px</option></select></label>
                <label className="field"><span>REMESH GRID</span><select value={options.remesh} onChange={(e) => setOptions({...options,remesh:Number(e.target.value)})}>{[256,384,512,768].map(v=><option key={v} value={v}>{v}</option>)}</select></label>
                <label className="field"><span>UPSAMPLE</span><select value={options.upsample} onChange={(e) => setOptions({...options,upsample:Number(e.target.value)})}>{[512,768,1024,1536].map(v=><option key={v} value={v}>{v}</option>)}</select></label>
                <label className="field"><span>AO SAMPLES</span><select value={options.aoSamples} onChange={(e) => setOptions({...options,aoSamples:Number(e.target.value)})}>{[16,32,64].map(v=><option key={v} value={v}>{v}</option>)}</select></label>
                {overrideSeeds && <label className="field"><span>ALL 4 KSampler SEEDS</span><input type="number" min={0} max={2147483647} value={options.seed} onChange={(e) => setOptions({...options,seed:Number(e.target.value)})}/></label>}
              </div>
              <label className="seed-toggle"><input type="checkbox" checked={overrideSeeds} onChange={e=>setOverrideSeeds(e.target.checked)}/><span>Tüm KSampler seed'lerini ayrıca değiştir <small>(dört aşama birlikte değiştirilir)</small></span></label>
              </>}
            </div>
            <div className="production-action-bar">
              <button type="button" className="btn btn-primary btn-generate" onClick={() => void runProduction()} disabled={busy || !!(job && ['queued','running','pending_result'].includes(job.status)) || !connected}>
                {busy ? <LoaderCircle className="spin" size={18}/> : <WandSparkles size={18}/>} {busy ? 'Gönderiliyor…' : '3D Model Üret'} <ChevronRight size={17}/>
              </button>
              <p className="mini-disclaimer">{productionMode==='source' ? 'Workflow aynen korunur' : 'Özel ayarlar etkin'} · Tekli üretim</p>
            </div>
          </> : <>
            <div className="panel-heading"><div><span className="panel-id">LIB</span><h2>Model Arşivi</h2></div><Database size={17}/></div>
            <div className="panel-content"><label className="field"><span>ARA</span><input type="search" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Model, motor…"/></label>
              <div className="asset-list">{filteredAssets.map(asset => <div key={asset.id} className="asset-list-entry"><button type="button" className={`asset-row ${selectedAsset?.id === asset.id ? 'chosen' : ''}`} onClick={() => void showAsset(asset)}>
                <div className="asset-avatar"><FileBox size={20}/></div><div><strong>{asset.title}</strong><span>{asset.engine} · {formatBytes(asset.fileSize)}</span></div><ChevronRight size={14}/></button>
                <button type="button" className="asset-delete-icon" onClick={()=>setAssetPendingDeletion(asset)} title={`${asset.title} modelini arşivden kaldır`} aria-label={`${asset.title} modelini arşivden kaldır`}><Trash2 size={15}/></button></div>)}
                {!filteredAssets.length && <p className="empty-library">Arşiv boş. GLB içe aktar veya ComfyUI ile üret.</p>}</div>
              <button className="btn btn-outline btn-full" onClick={() => void importGlb()} type="button"><FolderOpen size={16}/> GLB İçe Aktar</button></div>
          </>}
        </section>
        <section className="center-column">
          <ModelViewport base64={glb} title={previewLabel} busy={viewLoading} expanded={viewExpanded} onToggleExpanded={() => setViewExpanded(value => !value)} />
          <div className="activity-strip" role="status" aria-live="polite">
            <div className="activity-status"><span className={`activity-led ${job && ['queued','running','pending_result'].includes(job.status) ? 'is-busy' : job && ['failed','no_output'].includes(job.status) ? 'is-error' : ''}`}/>
              <span className="activity-label">{job && ['queued','running','pending_result'].includes(job.status) ? 'Üretim sürüyor' : job && ['failed','no_output'].includes(job.status) ? 'Üretim hatası' : job?.status === 'done' ? 'Üretim tamamlandı' : 'Studio hazır'}</span>
              <span className="activity-description">{job ? job.message : 'Arşivden bir model seç veya ComfyUI ile üret.'}</span>
            </div>
            <div className="activity-actions">
              {job && ['failed','no_output'].includes(job.status) && <button className="activity-debug" type="button" onClick={() => void showJobDiagnostics()}>Tanı kaydı</button>}
              <span className="activity-asset-count"><Archive size={13}/>{assets.length} model</span>
            </div>
          </div>
        </section>
        <section className="inspector-column">
          <div className="panel-heading"><div><span className="panel-id">02</span><h2>Model Bilgileri</h2></div><Gauge size={17}/></div>
          <div className="inspector-scroll">
          {selectedAsset ? <div className="inspector-block">
            <span className="section-label">SELECTED MODEL</span><h3>{selectedAsset.title}</h3>
            <div className="asset-meta"><span>{selectedAsset.engine}</span><span>{shortDate(selectedAsset.createdAt)}</span></div>
            {reviewOnlyAsset&&<p role="alert" className="unity-export-error">
              <strong>QA FAIL · UV REVIEW</strong> — Bu varlık yalnız görsel/UV inceleme
              taslağıdır. Kalite kontrolünü geçmedi; Unity export engellendi.
              Orijinal model arşivde korunuyor.
            </p>
            <div className="stats-grid"><Metric label="TRIANGLES" value={selectedAsset.triangleCount.toLocaleString('en-US')}/><Metric label="VERTICES" value={selectedAsset.vertexCount.toLocaleString('en-US')}/><Metric label="MATERIALS" value={String(selectedAsset.materialCount)}/><Metric label="GLB SIZE" value={formatBytes(selectedAsset.fileSize)}/></div>
            <TextureInspector glbBase64={glb} assetTitle={selectedAsset.title} assetId={selectedAsset.id} canExport={!reviewOnlyAsset && !/ \/ LOD\d+$/.test(previewLabel)} onExportComplete={() => setTextureExportRevision(x => x + 1)} />
            <div className="separator"/>
            <UVRepairPanel asset={selectedAsset} revision={textureExportRevision}
              canRepair={!reviewOnlyAsset && !/ \/ LOD\d+$/.test(previewLabel)}
              onCreated={async () => {await refresh();}}
              onOpen={showAsset}/>
            <div className="meta-row"><span>SHA256</span><code title={selectedAsset.sha256}>{selectedAsset.sha256.slice(0, 16)}…</code></div>
            <div className="meta-row"><span>Station</span><strong>{selectedAsset.station}</strong></div>
            <button type="button" className="btn btn-danger-subtle btn-full" onClick={()=>setAssetPendingDeletion(selectedAsset)}><Trash2 size={15}/> Arşivden kaldır</button>
            <div className="separator"/>
            <div className="card-heading"><div><Layers3 size={16}/><strong>Blender LOD Pipeline</strong></div></div>
            <p className="block-description">Tek modelden LOD0/1/2. Orijinal dosya asla değişmez.</p>
            {lod && <div className="lod-selector"><button onClick={() => void showAsset(selectedAsset)} type="button">SRC</button>{[0,1,2].map(index=><button key={index} type="button" onClick={() => void showAsset(selectedAsset,index)}>LOD{index}</button>)}</div>}
            {lod && <div className="lod-stats">{lod.report.lods.map((v,index)=><div key={index}><span>LOD{index}</span><strong>{v.triangles.toLocaleString('en-US')} tris</strong></div>)}</div>}
            <button type="button" className="btn btn-outline btn-full" onClick={() => void generateLods()} disabled={lodLoading || reviewOnlyAsset}>
              {lodLoading?<LoaderCircle className="spin" size={16}/>:<SquareStack size={16}/>} {lodLoading ? 'Blender işliyor…' : lod ? 'Yeni LOD versiyonu oluştur' : 'Blender ile LOD oluştur'}
            </button>
            <p className="mini-disclaimer">LOD geometrisi oluşturulur; yeniden normal/AO bake yapılmaz.</p>
            <div className="separator"/>
            {reviewOnlyAsset ?
              <p className="mini-disclaimer">QA FAIL inceleme varlıklarında Unity Native
                ve URP/HDRP export kapalıdır. Bu sürümün kullanımı önizleme ile sınırlı.</p> :
              <UnityExportPanel asset={selectedAsset} lod={lod}
                refreshVersion={textureExportRevision} />}
          </div> : <div className="inspector-block"><div className="waiting-state"><Boxes size={30}/><strong>Bir model seç</strong><span>Geometri, PBR ve LOD ayrıntıları burada görünecek.</span></div></div>}
          </div>
          <div className="system-panel"><div className="card-heading"><div><Cpu size={17}/><strong>GPU Telemetry</strong></div><span className="live-status">● LIVE</span></div>
            <p className="gpu-name">{gpu?.name || 'NVIDIA GPU'}</p><div className="gpu-line"><span>GPU</span><strong>{gpu?.utilPercent?.toFixed(0) ?? '—'}%</strong></div>
            <div className="mini-progress"><div style={{width: `${Math.min(100,gpu?.utilPercent ?? 0)}%`}} /></div>
            <div className="gpu-line"><span>VRAM</span><strong>{gpu?.vramUsedMiB != null ? (gpu.vramUsedMiB/1024).toFixed(2) : '—'} / {gpu?.vramTotalMiB ? (gpu.vramTotalMiB/1024).toFixed(1) : '—'} GiB</strong></div>
            <div className="mini-progress"><div style={{width:`${gpu?.vramUsedMiB && gpu.vramTotalMiB ? Math.min(100,100*gpu.vramUsedMiB/gpu.vramTotalMiB):0}%`}} /></div>
            <div className="gpu-line subtle"><span>Temperature</span><strong>{gpu?.temperatureC ?? '—'} °C</strong></div></div>
        </section>
      </div>
      <footer className="bottom-bar"><div><span className="footer-light"/> LOCAL PRODUCTION · NO CLOUD INFERENCE</div><div>{environment?.settings.comfyUrl ?? 'http://127.0.0.1:8188'} <span>•</span> {assets.length} ASSETS</div></footer>
    </main>
    {assetPendingDeletion && <div className="modal-backdrop" onMouseDown={(e)=>{if(e.target===e.currentTarget && !deleteBusy)setAssetPendingDeletion(null);}}>
      <div className="modal delete-dialog" role="alertdialog" aria-modal="true" aria-label="Modeli arşivden kaldır">
        <div className="modal-header"><div><span className="eyebrow">ASSET LIBRARY</span><h2>Modeli kaldır?</h2></div><button type="button" className="icon-button" disabled={deleteBusy} onClick={()=>setAssetPendingDeletion(null)} aria-label="Kapat"><X size={18}/></button></div>
        <p className="modal-desc"><strong>{assetPendingDeletion.title}</strong> arşiv listesinden kaldırılacak. Studio'nun kendi model ve LOD dosyaları çöp arşivine taşınacak; dışarıdan içe aktardığın orijinal dosyalar silinmeyecek.</p>
        <div className="modal-footer"><button type="button" className="btn btn-ghost" disabled={deleteBusy} onClick={()=>setAssetPendingDeletion(null)}>Vazgeç</button><button type="button" className="btn btn-danger" disabled={deleteBusy} onClick={()=>void deleteAsset()}>{deleteBusy?<LoaderCircle className="spin" size={16}/>:<Trash2 size={16}/>} Arşivden kaldır</button></div>
      </div>
    </div>}
    {settingsOpen && <SettingDialog current={environment?.settings ?? initialSettings} onSave={saveSettings} onClose={() => setSettingsOpen(false)} />}
    {debugInfo && <div className="modal-backdrop" onMouseDown={(e) => {if(e.target===e.currentTarget) setDebugInfo('');}}><div className="modal debug-modal" role="dialog" aria-modal="true" aria-label="ComfyUI job diagnostics"><div className="modal-header"><h2>ComfyUI Tanı Kaydı</h2><button type="button" className="icon-button" onClick={()=>setDebugInfo('')}><X size={18}/></button></div><p className="modal-desc">Hata kaydını kopyalayıp paylaşabilirsin. Prompt ID, hata veren node ve çıktı düğümleri yer alır.</p><textarea className="debug-text" value={debugInfo} readOnly spellCheck={false}/><div className="modal-footer"><button className="btn btn-outline" type="button" onClick={() => void navigator.clipboard?.writeText(debugInfo).then(()=>setNotice('Tanı kaydı kopyalandı.')).catch(()=>setError('Metni elle kopyalayabilirsin.'))}>Kopyala</button><button className="btn btn-ghost" type="button" onClick={()=>setDebugInfo('')}>Kapat</button></div></div></div>}
  </div>;
}

export default App;
