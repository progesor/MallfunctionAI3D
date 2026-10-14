import { Suspense, useEffect, useMemo, useState } from 'react';
import { Canvas } from '@react-three/fiber';
import { Grid, OrbitControls } from '@react-three/drei';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import {
  Box3, Color, Group, Material, Mesh, MeshBasicMaterial, MeshNormalMaterial, MeshStandardMaterial,
  Vector3,
} from 'three';
import { Box, Eye, Scan, Grid2X2, Axis3D, Maximize2, Minimize2 } from 'lucide-react';

export type ViewMode = 'pbr' | 'clay' | 'wireframe' | 'normals';
interface Props { base64: string | null; title?: string; busy: boolean; expanded?: boolean; onToggleExpanded?: () => void }

function decode(base64: string): ArrayBuffer {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

function LoadedObject({ binary, mode }: { binary: string; mode: ViewMode }) {
  const [model, setModel] = useState<Group | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let alive = true;
    const loader = new GLTFLoader();
    void loader.parseAsync(decode(binary), '').then((gltf) => {
      if (alive) { setModel(gltf.scene); setError(''); }
    }).catch((e: unknown) => { if (alive) setError(String(e)); });
    return () => { alive = false; setModel(null); };
  }, [binary]);
  const transform = useMemo(() => {
    if (!model) return { position: [0, 0, 0] as const, scale: 1 };
    const box = new Box3().setFromObject(model);
    const size = box.getSize(new Vector3());
    const center = box.getCenter(new Vector3());
    const scale = 2.25 / Math.max(size.x, size.y, size.z, 0.01);
    return { position: [-center.x * scale, 1.15 - center.y * scale, -center.z * scale] as const, scale };
  }, [model]);
  useEffect(() => {
    if (!model) return;
    const original = new Map<Mesh, Material | Material[]>();
    const temporary: Material[] = [];
    model.traverse((object) => {
      if (!(object instanceof Mesh)) return;
      original.set(object, object.material);
      if (mode === 'pbr') return;
      let changed: Material;
      if (mode === 'clay') changed = new MeshStandardMaterial({ color: new Color('#b7c4ce'), roughness: 0.87 });
      else if (mode === 'normals') changed = new MeshNormalMaterial({ flatShading: false });
      else changed = new MeshBasicMaterial({ color: new Color('#86d7b7'), wireframe: true });
      temporary.push(changed);
      object.material = changed;
    });
    return () => {
      for (const [object, material] of original) object.material = material;
      for (const material of temporary) material.dispose();
    };
  }, [model, mode]);
  if (error) return null;
  return model ? <group position={transform.position} scale={transform.scale}><primitive object={model} /></group> : null;
}

const viewOptions: Array<{ id: ViewMode; label: string; icon: typeof Eye }> = [
  { id: 'pbr', label: 'PBR', icon: Eye },
  { id: 'clay', label: 'Clay', icon: Box },
  { id: 'wireframe', label: 'Wire', icon: Grid2X2 },
  { id: 'normals', label: 'Normals', icon: Axis3D },
];

export function ModelViewport({ base64, title, busy, expanded=false, onToggleExpanded }: Props) {
  const [mode, setMode] = useState<ViewMode>('pbr');
  return (
    <div className="viewport-frame">
      <div className="viewport-toolbar">
        <div className="viewport-caption"><Scan size={16} /><span>{title || '3D Inspector'}</span></div>
        <div className="view-modes" aria-label="3D render mode">
          {viewOptions.map(({ id, label, icon: Icon }) => (
            <button key={id} type="button" className={mode === id ? 'view-button selected' : 'view-button'}
              onClick={() => setMode(id)} title={label} aria-pressed={mode === id}>
              <Icon size={14} /><span>{label}</span>
            </button>
          ))}
          {onToggleExpanded && <button type="button" className="view-button expand-viewport" aria-label={expanded ? 'Görünümü küçült' : 'Görünümü büyüt'} title={expanded ? 'Görünümü küçült' : 'Görünümü tam genişlikte aç'} onClick={onToggleExpanded}>{expanded ? <Minimize2 size={16}/> : <Maximize2 size={16}/>}</button>}
        </div>
      </div>
      <div className="viewport-canvas">
        <Canvas shadows={false} camera={{ position: [4.4, 3.15, 5.1], fov: 34, near: 0.02, far: 120 }} dpr={[1, 1.75]}>
          <color attach="background" args={['#141e2a']} />
          <ambientLight intensity={1.25} />
          <hemisphereLight args={['#f1f6ff', '#233445', 1.3]} />
          <directionalLight position={[5, 9, 5]} intensity={2.1} />
          <directionalLight position={[-5, 3, -4]} intensity={0.75} color="#a8c4ed" />
          <Grid position={[0, -0.08, 0]} args={[8, 8]} infiniteGrid sectionColor="#304152"
            cellColor="#233342" fadeDistance={20} fadeStrength={1.8} sectionSize={1} cellSize={0.25} />
          <axesHelper args={[0.75]} />
          <Suspense fallback={null}>{base64 && <LoadedObject binary={base64} mode={mode} />}</Suspense>
          <OrbitControls makeDefault minDistance={1} maxDistance={24} enableDamping dampingFactor={0.075} target={[0, 1.1, 0]} />
        </Canvas>
        {!base64 && <div className="viewport-empty">
          <div className="empty-visual"><Box size={35} strokeWidth={1.35} /></div>
          <strong>{busy ? 'GLB yükleniyor…' : '3D sahnesi hazır'}</strong>
          <span>Arşivden bir GLB seç veya Pixal3D ile ilk modelini üret.</span>
        </div>}
      </div>
      <div className="viewport-footer"><span>ORBIT · SOL TIK</span><span>PAN · SAĞ TIK</span><span>ZOOM · SCROLL</span><span className="live-indicator">● LOCAL VIEWPORT</span></div>
    </div>
  );
}
