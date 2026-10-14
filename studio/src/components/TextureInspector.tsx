import { useEffect, useMemo, useState } from 'react';
import { ChevronDown, Image as ImageIcon, Layers3, Maximize2, X } from 'lucide-react';
import {
  decodeGlbBase64, grayscaleChannel, inspectGlbTextures, MAP_LABELS,
} from '../lib/glb-inspector';
import type { TextureMap, TextureRole, GlbTextureInspection } from '../lib/glb-inspector';

interface Props { glbBase64: string | null; assetTitle: string }
interface MapPreview { url: string; sourceUrl: string; width: number; height: number }

async function extractPreview(image: TextureMap, originalUrl: string): Promise<MapPreview> {
  const loaded = new Image();
  loaded.src = originalUrl;
  await loaded.decode();
  const width = loaded.naturalWidth;
  const height = loaded.naturalHeight;
  if (!width || !height) throw new Error(`Görüntü açılmadı: ${image.image.name}`);
  if (image.channel === 'rgb') return { url: originalUrl, sourceUrl: originalUrl, width, height };
  const maxPixels = 2048;
  const factor = Math.min(1, maxPixels / Math.max(width, height));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(width * factor));
  canvas.height = Math.max(1, Math.round(height * factor));
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) throw new Error('Texture kanalı için 2D Canvas oluşturulamadı.');
  ctx.drawImage(loaded, 0, 0, canvas.width, canvas.height);
  const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height);
  pixels.data.set(grayscaleChannel(pixels.data, image.channel));
  ctx.putImageData(pixels, 0, 0);
  const previewBlob = await new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('Texture önizlemesi oluşturulamadı.')), 'image/png');
  });
  return { url: URL.createObjectURL(previewBlob), sourceUrl: originalUrl, width, height };
}

const mapHints: Record<TextureRole, string> = {
  baseColor: 'sRGB · Renk',
  normal: 'Linear · Tangent space',
  roughness: 'G · Pürüzlülük',
  metallic: 'B · Metaliklik',
  occlusion: 'R · Ortam örtülmesi',
  emissive: 'sRGB · Işınım',
};

export function TextureInspector({ glbBase64, assetTitle }: Props) {
  const inspection = useMemo<{ value?: GlbTextureInspection; error?: string }>(() => {
    if (!glbBase64) return {};
    try { return { value: inspectGlbTextures(decodeGlbBase64(glbBase64)) }; }
    catch (error) { return { error: String(error) }; }
  }, [glbBase64]);
  const [materialIndex, setMaterialIndex] = useState(0);
  const [previews, setPreviews] = useState<Record<string, MapPreview>>({});
  const [loading, setLoading] = useState(false);
  const [previewError, setPreviewError] = useState('');
  const [zoomedMap, setZoomedMap] = useState<string | null>(null);
  const [viewPacked, setViewPacked] = useState(false);
  const items = useMemo(() => inspection.value?.maps.filter(item => item.materialIndex === materialIndex) ?? [],
    [inspection.value, materialIndex]);
  useEffect(() => { setMaterialIndex(0); setZoomedMap(null); setViewPacked(false); }, [glbBase64]);

  useEffect(() => {
    let stopped = false;
    const objectUrls: string[] = [];
    setPreviews({});
    setPreviewError('');
    setLoading(items.length > 0);
    async function prepare() {
      try {
        const results: Record<string, MapPreview> = {};
        for (const item of items) {
          if (stopped) break;
          const bytes = new Uint8Array(item.image.bytes);
          const original = URL.createObjectURL(new Blob([bytes], { type: item.image.mimeType }));
          objectUrls.push(original);
          const preview = await extractPreview(item, original);
          // Cleanup may already have run while Image.decode() was pending.
          // Revoke that newly created channel URL immediately in that case.
          if (stopped) {
            if (preview.url !== original) URL.revokeObjectURL(preview.url);
            break;
          }
          if (preview.url !== original) objectUrls.push(preview.url);
          results[item.key] = preview;
        }
        if (!stopped) setPreviews(results);
      } catch (error) {
        if (!stopped) setPreviewError(`PBR önizlemesi: ${String(error)}`);
      } finally { if (!stopped) setLoading(false); }
    }
    void prepare();
    return () => { stopped = true; for (const url of objectUrls) URL.revokeObjectURL(url); };
  }, [items]);
  useEffect(() => {
    if (!zoomedMap) return;
    function escape(event: KeyboardEvent) { if (event.key === 'Escape') setZoomedMap(null); }
    window.addEventListener('keydown', escape);
    return () => window.removeEventListener('keydown', escape);
  }, [zoomedMap]);

  if (!glbBase64) return null;
  const info = inspection.value;
  const selectedMap = items.find(item => item.key === zoomedMap);
  const selectedPreview = selectedMap ? previews[selectedMap.key] : null;

  return <section className="texture-inspector" aria-label="PBR Texture Inspector" data-testid="pbr-texture-inspector">
    <div className="texture-section-heading"><div><Layers3 size={17}/><strong>PBR Texture Inspector</strong></div>
      {info && <span className="texture-summary">{info.imageCount} görsel · {info.maps.length} kanal</span>}</div>
    {inspection.error ? <p className="texture-inspector-error">GLB texture okunamadı: {inspection.error}</p>
      : !info || !info.maps.length ? <p className="texture-inspector-empty">Bu modelde görüntülenebilir gömülü PBR haritası bulunamadı.</p>
        : <>
          {info.materialNames.length > 1 && <label className="texture-material-field"><span>MATERYAL</span><span className="select-wrap"><select value={materialIndex} onChange={event => { setMaterialIndex(Number(event.target.value)); setZoomedMap(null); }}>
            {info.materialNames.map((name, index) => <option value={index} key={index}>{name}</option>)}
          </select><ChevronDown size={14}/></span></label>}
          <div className="texture-map-grid">{items.map(item => {
            const preview = previews[item.key];
            return <button type="button" key={item.key} className="texture-map-card" disabled={!preview}
              onClick={() => { setZoomedMap(item.key); setViewPacked(false); }} title={`${MAP_LABELS[item.role]} haritasını büyüt`}>
              <span className="texture-map-thumb">{preview ? <img src={preview.url} alt={`${MAP_LABELS[item.role]} kanalı`} loading="lazy"/> : <ImageIcon size={23}/>}</span>
              <span className="texture-map-title">{MAP_LABELS[item.role]}</span>
              <span className="texture-map-meta">{preview ? `${preview.width} × ${preview.height}` : 'Yükleniyor'}</span>
            </button>;
          })}</div>
          {loading && <span className="texture-inspector-loading">PBR haritaları açılıyor…</span>}
          {previewError && <p className="texture-inspector-error">{previewError}</p>}
          {!!info.warnings.length && <p className="texture-inspector-note">{info.warnings.join(' ')}</p>}
          <p className="texture-inspector-note">Roughness (G), Metallic (B), AO (R) kanalları gömülü packed map'ten ayrılarak gösterilir. Kaynak GLB değiştirilmez.</p>
        </>}
    {selectedMap && selectedPreview && <div className="texture-lightbox" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) setZoomedMap(null); }}>
      <div className="texture-lightbox-panel" role="dialog" aria-modal="true" aria-label={`${MAP_LABELS[selectedMap.role]} haritası`}>
        <div className="texture-lightbox-head"><div><span className="eyebrow">PBR MAP · {selectedMap.materialName}</span><h3>{assetTitle} / {MAP_LABELS[selectedMap.role]}</h3></div><button type="button" className="icon-button" aria-label="Texture önizlemesini kapat" onClick={() => setZoomedMap(null)}><X size={18}/></button></div>
        <div className="texture-lightbox-image"><img src={viewPacked ? selectedPreview.sourceUrl : selectedPreview.url} alt={`${MAP_LABELS[selectedMap.role]} harita büyük görünüm`} /></div>
        <div className="texture-lightbox-foot"><span><Maximize2 size={15}/>{selectedPreview.width} × {selectedPreview.height} · {mapHints[selectedMap.role]}</span>
          {selectedMap.channel !== 'rgb' && <label className="texture-packed-toggle"><input type="checkbox" checked={viewPacked} onChange={event => setViewPacked(event.target.checked)}/>Packed orijinali göster</label>}
        </div>
      </div>
    </div>}
  </section>;
}
