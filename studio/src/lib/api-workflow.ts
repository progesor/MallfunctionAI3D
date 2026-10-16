/**
 * Executable ComfyUI API workflows are authoritative. Never compile a saved
 * LiteGraph UI layout at runtime when an exported API prompt is available:
 * custom/3D viewport widgets and output-node metadata differ between formats.
 *
 * Supported first-party M1 recipe: official Pixal3D/TRELLIS.2 (IDs pinned to
 * exported user workflow). Future generic recipe schemas register separate
 * adapters rather than trying to modify unknown graph internals.
 */
import type { ApiPrompt, GenerateOptions, ObjectInfo } from './workflow';

export type ApiWorkflow = ApiPrompt;
export type WorkflowDocument = { format: 'api'; prompt: ApiWorkflow } | { format: 'litegraph'; graph: unknown };
export interface ApiGraphInfo { nodeCount: number; outputNodes: string[]; mode: 'pixal3d' | 'trellis2' }

const REQUIRED: Record<string, string> = {
  '122': 'LoadImage', '186': 'DecimateMesh', '288': 'PrimitiveInt',
  '241': 'RemeshMesh', '285': 'MeshToFile3D', '316': 'PrimitiveBoolean',
  '322': 'Save3DAdvanced',
};
const MODEL_STAGES = new Set(['KSampler', 'Trellis2ShapeStage', 'Trellis2UpsampleStage', 'Trellis2TextureStage']);

function entry(prompt: ApiWorkflow, id: string, expected?: string) {
  const node = prompt[id];
  if (!node || (expected && node.class_type !== expected)) {
    throw new Error(`#${id} ${expected ?? 'node'} bulunamadı veya farklı türde. Pixal3D/TRELLIS.2 API workflow seç.`);
  }
  if (!node.inputs || typeof node.inputs !== 'object') throw new Error(`#${id} inputs geçersiz.`);
  return node;
}

function isReference(value: unknown): value is [string, number] {
  return Array.isArray(value) && value.length === 2 && /^\d+$/.test(String(value[0]))
    && Number.isInteger(value[1]) && Number(value[1]) >= 0;
}

/** Fails closed on mask-only or missing Save3D graph, and cycles. */
export function validateApiProduction(prompt: ApiWorkflow): ApiGraphInfo {
  if (!prompt || typeof prompt !== 'object' || Array.isArray(prompt)) {
    throw new Error('Bu dosya API workflow değil. ComfyUI > Export API format kullan.');
  }
  const count = Object.keys(prompt).length;
  if (count < 25 || count > 600) throw new Error(`Üretim grafı boyutu geçersiz (${count} node).`);
  for (const [id, type] of Object.entries(REQUIRED)) entry(prompt, id, type);
  const output = entry(prompt, '322', 'Save3DAdvanced');
  if (!isReference(output.inputs.model_3d) || String(output.inputs.model_3d[0]) !== '285') {
    throw new Error('Save3DAdvanced #322, MeshToFile3D #285 çıkışına bağlı değil; sadece maske/önizleme yapılamaz.');
  }
  if (!output.inputs.viewport_state || typeof output.inputs.viewport_state !== 'object'
    || Array.isArray(output.inputs.viewport_state)) {
    throw new Error('Save3DAdvanced viewport_state eksik. Workflow’u ComfyUI üzerinden API formatında yeniden dışa aktar.');
  }
  const visited = new Set<string>();
  const stack = new Set<string>();
  function visit(id: string): void {
    if (visited.has(id)) return;
    if (stack.has(id)) throw new Error(`#${id} düğümünde döngü bulunuyor.`);
    const node = entry(prompt, id);
    stack.add(id);
    for (const value of Object.values(node.inputs)) {
      if (isReference(value)) visit(String(value[0]));
    }
    stack.delete(id);
    visited.add(id);
  }
  visit('322');
  if (!visited.has('122') || ![...visited].some((id) => MODEL_STAGES.has(prompt[id].class_type)) || visited.size < 25) {
    throw new Error('GLB çıkışına bağlı tam 3D üretim zinciri yok (yalnızca mask/preview).');
  }
  const switcher=entry(prompt,'316','PrimitiveBoolean').inputs.value;
  if (typeof switcher!=='boolean') throw new Error('Pixal3D/TRELLIS.2 seçici boolean değil.');
  const outputNodes = Object.entries(prompt).filter(([,node]) =>
    node.class_type === 'Save3DAdvanced' || node.class_type === 'SaveGLB').map(([id])=>id);
  return { nodeCount: count, outputNodes, mode: switcher ? 'trellis2' : 'pixal3d' };
}

function writeLiteral(prompt: ApiWorkflow, id: string, expected: string, key: string, value: unknown) {
  const node = entry(prompt,id,expected);
  if (!(key in node.inputs)) throw new Error(`#${id} (${expected}): ${key} alanı yok.`);
  if (isReference(node.inputs[key])) {
    throw new Error(`#${id} (${expected}): ${key} bir bağlantı; korumak için değiştirilmiyor.`);
  }
  node.inputs[key] = value;
}

/** No optimizer may silently rewrite an imported production recipe.
 * The default mode changes ONLY the selected LoadImage file name (ComfyUI uploads
 * are given a unique name). Output prefix, samplers, switch, maps, bake, and even
 * auxiliary output nodes remain byte-for-byte semantically equivalent.
 */
export type ProductionMode = 'source' | 'custom';
export interface ProductionTuning {
  mode: ProductionMode;
  /** Separate and explicit: four different KSampler seeds control four stages. */
  overrideAllSeeds?: boolean;
}

export function prepareApiProduction(
  raw: ApiWorkflow,
  options: GenerateOptions,
  imageName: string,
  targetMode: 'pixal3d' | 'trellis2',
  tuning: ProductionTuning = { mode: 'source' },
): ApiWorkflow {
  validateApiProduction(raw);
  if (!imageName || /[\\/]/.test(imageName) || imageName.includes('..')) {
    throw new Error('ComfyUI yükleme dosya adı geçersiz.');
  }
  if (!['source','custom'].includes(tuning.mode)) throw new Error('Bilinmeyen üretim modu');
  const prompt = structuredClone(raw);
  writeLiteral(prompt,'122','LoadImage','image',imageName);
  if (tuning.mode === 'custom') {
    if (![1024,2048].includes(options.textureSize) || !Number.isInteger(options.triangles)
      || options.triangles < 1000 || options.triangles > 80000
      || ![512,768,1024,1536].includes(options.upsample)
      || ![256,384,512,768].includes(options.remesh)
      || ![16,32,64].includes(options.aoSamples)) {
      throw new Error('Özel üretim ayarları doğrulanamadı.');
    }
    writeLiteral(prompt,'186','DecimateMesh','target_face_count',options.triangles);
    writeLiteral(prompt,'288','PrimitiveInt','value',options.textureSize);
    // Both UV resolution and texture baking have links to #288. Never sever them.
    writeLiteral(prompt,'224','BakeNormalMapFromMesh','resolution',options.textureSize);
    writeLiteral(prompt,'233','BakeAmbientOcclusion','resolution',options.textureSize);
    writeLiteral(prompt,'233','BakeAmbientOcclusion','samples',options.aoSamples);
    writeLiteral(prompt,'261','RenderUVAtlas','resolution',options.textureSize);
    writeLiteral(prompt,'241','RemeshMesh','resolution',options.remesh);
    writeLiteral(prompt,'94','Trellis2UpsampleStage','target_resolution',String(options.upsample));
    writeLiteral(prompt,'316','PrimitiveBoolean','value',targetMode === 'trellis2');
    if (tuning.overrideAllSeeds) {
      if (!Number.isSafeInteger(options.seed) || options.seed < 0 || options.seed > 2147483647) {
        throw new Error('Seed geçersiz');
      }
      // Intentionally opt-in ONLY: 56,43,42,42 were distinct in official recipe.
      for (const id of ['3','12','18','23']) writeLiteral(prompt,id,'KSampler','seed',options.seed);
    }
  }
  // Important: keep every output node. Removing extra output nodes can modify
  // ComfyUI's normal execution plan and is not a faithful reproduction.
  validateApiProduction(prompt);
  return prompt;
}

/** Semantic per-input audit, excluding the *requested* uploaded image path. */
export function productionGraphChanges(original: ApiWorkflow, prepared: ApiWorkflow): string[] {
  const changes: string[]=[];
  const ids=new Set([...Object.keys(original),...Object.keys(prepared)]);
  for (const id of [...ids].sort((a,b)=>Number(a)-Number(b))) {
    const before=original[id],after=prepared[id];
    if (!before || !after || before.class_type!==after.class_type) {
      changes.push(`#${id}: node added/removed/changed`);continue;
    }
    const names=new Set([...Object.keys(before.inputs),...Object.keys(after.inputs)]);
    for (const key of names) {
      if (JSON.stringify(before.inputs[key])!==JSON.stringify(after.inputs[key])) {
        changes.push(`#${id}.${key}`);
      }
    }
  }
  return changes;
}

export function assertInstalledNodes(prompt: ApiWorkflow, objectInfo: ObjectInfo): void {
  const missing=[...new Set(Object.values(prompt).map((x)=>x.class_type))].filter((type)=>!objectInfo[type]);
  if (missing.length) throw new Error(`ComfyUI'de API workflow düğümleri eksik: ${missing.join(', ')}`);
}
