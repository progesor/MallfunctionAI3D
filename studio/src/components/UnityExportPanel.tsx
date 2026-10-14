import { useCallback, useEffect, useState } from 'react';
import { Box, Check, ChevronDown, Clipboard, PackageCheck, LoaderCircle, RefreshCw, ShieldCheck } from 'lucide-react';
import { bridge, formatBytes, shortDate } from '../lib/bridge';
import type { Asset, LodResult, TextureExportSummary, UnityExportResult, UnityExportSummary } from '../lib/bridge';

interface Props { asset: Asset; lod: LodResult | null; refreshVersion: number }

export function UnityExportPanel({ asset, lod, refreshVersion }: Props) {
  const [textureExports, setTextureExports] = useState<TextureExportSummary[]>([]);
  const [pastPackages, setPastPackages] = useState<UnityExportSummary[]>([]);
  const [selected, setSelected] = useState('');
  const [loading, setLoading] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [result, setResult] = useState<UnityExportResult | null>(null);
  const [error, setError] = useState('');
  const [copied, setCopied] = useState(false);

  const reload = useCallback(async () => {
    if (!asset.id) return;
    setLoading(true);
    try {
      const [exports, packages] = await Promise.all([
        bridge.textureExports(asset.id), bridge.unityExports(asset.id),
      ]);
      setTextureExports(exports);
      setPastPackages(packages);
      setSelected(old => exports.some(x => x.outputDir === old) ? old : (exports[0]?.outputDir ?? ''));
    } catch (err) { setError(`Game Export arşivi: ${String(err)}`); }
    finally { setLoading(false); }
  }, [asset.id]);

  useEffect(() => {
    setResult(null);setError('');setSelected('');setCopied(false);
    void reload();
  }, [reload,refreshVersion]);

  async function create() {
    if (!selected || !lod || exporting) return;
    setError('');setResult(null);setExporting(true);setCopied(false);
    try {
      const saved = await bridge.exportUnityUrp(asset.id,selected);
      if (!saved.sourceUntouched || !saved.lodTriangles.every(x => x > 0)) {
        throw new Error('Unity export manifesti gerekli kabul alanlarını içermiyor.');
      }
      setResult(saved);
      await reload();
    } catch (err) { setError(`Unity paketleme başarısız: ${String(err)}`); }
    finally { setExporting(false); }
  }

  async function copyPath(path: string) {
    try { await navigator.clipboard.writeText(path);setCopied(true); }
    catch { setError('Klasör yolu panoya kopyalanamadı.'); }
  }

  return <div className="unity-export-panel" aria-label="Unity URP Game Export" data-testid="unity-export-panel">
    <div className="unity-export-title"><div><Box size={16}/><strong>Unity URP Game Export</strong></div><button type="button" title="Texture export sürümlerini yenile" aria-label="Texture export listesini yenile" disabled={loading || exporting} onClick={() => void reload()}><RefreshCw size={14} className={loading ? 'spin' : ''}/></button></div>
    <p className="unity-export-description">3 LOD geometri GLB + tek ortak PBR materyal seti. Mevcut modeller, Blender çıktıları ve map arşivi değişmez.</p>
    <div className="unity-export-status"><span className={lod ? 'ready' : ''}>{lod ? <Check size={13}/> : <span className="unity-export-status-dot"/>} Blender LOD {lod ? 'hazır' : 'gerekli'}</span><span className={textureExports.length ? 'ready' : ''}>{textureExports.length ? <Check size={13}/> : <span className="unity-export-status-dot"/>} PBR Export {textureExports.length ? 'hazır' : 'gerekli'}</span></div>
    {lod && <div className="unity-export-lod-row">{lod.report.lods.slice(0,3).map((item,index)=><span key={index}><small>LOD{index}</small><strong>{Math.round(item.triangles).toLocaleString('en-US')}</strong></span>)}</div>}
    <label className="unity-export-select"><span>TEXTURE EXPORT SÜRÜMÜ</span><div><select value={selected} onChange={event=>{setSelected(event.target.value);setResult(null);}} disabled={exporting || loading} aria-label="Unity export için PBR sürümü seç">
      {textureExports.length === 0 && <option value="">Önce Texture Export oluştur</option>}
      {textureExports.map((entry,index)=><option key={entry.outputDir} value={entry.outputDir}>{index===0 ? 'Son · ' : ''}{entry.preset}px · {entry.mapCount} map · {shortDate(entry.generatedAt)}</option>)}
    </select><ChevronDown size={13}/></div></label>
    <button type="button" className="btn btn-outline btn-full unity-export-action" disabled={!lod || !selected || exporting || loading} onClick={() => void create()} data-testid="unity-export-button">
      {exporting ? <LoaderCircle size={15} className="spin"/> : <PackageCheck size={15}/>} {exporting ? 'Paket oluşturuluyor…' : 'Unity URP Paketi Oluştur'}
    </button>
    {!lod && <p className="unity-export-hint">Önce Blender LOD Pipeline bölümünden 3 LOD oluştur.</p>}
    {lod && !textureExports.length && <p className="unity-export-hint">Önce yukarıdaki PBR Texture Inspector'dan 512 veya 1024 px texture export yap.</p>}
    {error && <p className="unity-export-error" role="alert">{error}</p>}
    {result && <div className="unity-export-success" role="status" data-testid="unity-export-success"><div><ShieldCheck size={17}/><strong>Game staging paketi hazır</strong></div>
      <div className="unity-export-savings">LOD geometrisi: {formatBytes(result.sourceLodBytes)} → {formatBytes(result.geometryLodBytes)} <small>· Ortak doku {formatBytes(result.sharedTextureBytes)}</small></div>
      <button type="button" onClick={()=>void copyPath(result.outputDir)} title={result.outputDir}><Clipboard size={13}/>{copied ? 'Kopyalandı' : 'Klasör yolunu kopyala'}</button>
      <small>Unity'ye import ve LODGroup/prefab doğrulaması ayrıca yapılmalı.</small>
    </div>}
    {!!pastPackages.length && <div className="unity-export-history"><strong>Game Export geçmişi</strong>{pastPackages.slice(0,3).map(item=><button type="button" title={item.outputDir} key={item.outputDir} onClick={()=>void copyPath(item.outputDir)}><span>{shortDate(item.generatedAt)} · {item.triangles.join('/')} tris</span><Clipboard size={12}/></button>)}</div>}
  </div>;
}
