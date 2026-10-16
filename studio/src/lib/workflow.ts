/**
 * Converts a pinned ComfyUI LiteGraph UI workflow to ComfyUI /prompt API data.
 * Class metadata is read from the RUNNING ComfyUI /object_info endpoint; no
 * brittle index-to-widget table is hard-coded here.
 */
export type JsonRecord = Record<string, unknown>;
export type LiteLink = [number, number, number, number, number, string?];
export interface LiteNode {
  id: number;
  type: string;
  title?: string;
  mode?: number;
  inputs?: Array<{ name: string; link?: number | null }>;
  widgets_values_named?: JsonRecord;
  widgets_values?: unknown[];
}
export interface LiteGraph {
  nodes: LiteNode[];
  links: LiteLink[];
  [key: string]: unknown;
}
export interface NodeClassDef {
  input?: {
    required?: Record<string, unknown>;
    optional?: Record<string, unknown>;
    hidden?: Record<string, unknown>;
  };
  output?: string[];
  output_node?: boolean;
}
export type ObjectInfo = Record<string, NodeClassDef>;
export type ApiPrompt = Record<string, { class_type: string; inputs: JsonRecord; _meta: { title: string } }>;
export interface PreparedGraph {
  prompt: ApiPrompt;
  reachable: string[];
  nodeClasses: string[];
}
export interface GenerateOptions {
  triangles: number;
  textureSize: number;
  upsample: number;
  remesh: number;
  aoSamples: number;
  seed: number;
}
export const DEFAULT_OPTIONS: GenerateOptions = Object.freeze({
  triangles: 8000,
  textureSize: 1024,
  upsample: 1024,
  remesh: 512,
  aoSamples: 32,
  seed: 42,
});

const VISUAL_ONLY = new Set(['Note', 'MarkdownNote', 'Reroute']);
const NODE_IDS = Object.freeze({
  inputImage: 122, targetFaces: 186, resolution: 288, textureBake: 147,
  unwrap: 196, normalBake: 224, aoBake: 233, uvAtlas: 261,
  remesh: 241, upsample: 94, save3d: 322,
});

function requireNode(graph: LiteGraph, id: number): LiteNode {
  const node = graph.nodes.find((n) => n.id === id);
  if (!node) throw new Error(`Workflow node #${id} bulunamadı.`);
  return node;
}
function setWidget(graph: LiteGraph, id: number, widget: string, value: unknown): void {
  const node = requireNode(graph, id);
  if (!node.widgets_values_named || !Object.prototype.hasOwnProperty.call(node.widgets_values_named, widget)) {
    throw new Error(`#${id} ${node.type}: '${widget}' alanı bulunamadı; kaynak workflow değişmiş olabilir.`);
  }
  node.widgets_values_named[widget] = value;
  // Some ComfyUI versions use widgets_values as well as named values.
  // For all patchable fields, index mappings are pinned to the pilot graph.
  const indices: Record<number, Record<string, number>> = {
    122: { image: 0 }, 186: { target_face_count: 0 }, 288: { value: 0 },
    147: { texture_size: 0 }, 196: { resolution: 1 }, 224: { resolution: 0 },
    233: { resolution: 0, samples: 1 }, 261: { resolution: 0 },
    241: { resolution: 0 }, 94: { target_resolution: 0 },
    3: { seed: 0 }, 12: { seed: 0 }, 18: { seed: 0 }, 23: { seed: 0 },
  };
  const index = indices[id]?.[widget];
  if (index === undefined || !node.widgets_values || index >= node.widgets_values.length) {
    throw new Error(`#${id} ${node.type}: widget düzeni doğrulanamadı.`);
  }
  node.widgets_values[index] = value;
}

/** Clones source; patching never changes the official template or existing ComfyUI graphs. */
export function preparePilot(source: LiteGraph, options: GenerateOptions, uploadedImageName: string): LiteGraph {
  if (!uploadedImageName || /[\\/]/.test(uploadedImageName) || uploadedImageName.includes('..')) {
    throw new Error('ComfyUI tarafından dönen geçerli bir input görüntüsü adı gerekli.');
  }
  if (![1024, 2048].includes(options.textureSize)) throw new Error('Texture: 1024 veya 2048.');
  if (!Number.isInteger(options.triangles) || options.triangles < 1000 || options.triangles > 80000) {
    throw new Error('Triangle hedefi 1.000–80.000 arasında olmalı.');
  }
  if (![512, 768, 1024, 1536].includes(options.upsample)) throw new Error('Desteklenmeyen upsample çözünürlüğü.');
  if (![256, 384, 512, 768].includes(options.remesh)) throw new Error('Desteklenmeyen remesh çözünürlüğü.');
  if (![16, 32, 64].includes(options.aoSamples)) throw new Error('AO samples: 16, 32 veya 64.');
  if (!Number.isSafeInteger(options.seed) || options.seed < 0 || options.seed > 2_147_483_647) throw new Error('Seed geçersiz.');
  const graph = structuredClone(source);
  setWidget(graph, NODE_IDS.inputImage, 'image', uploadedImageName);
  setWidget(graph, NODE_IDS.targetFaces, 'target_face_count', options.triangles);
  setWidget(graph, NODE_IDS.resolution, 'value', options.textureSize);
  setWidget(graph, NODE_IDS.textureBake, 'texture_size', options.textureSize);
  setWidget(graph, NODE_IDS.unwrap, 'resolution', options.textureSize);
  setWidget(graph, NODE_IDS.normalBake, 'resolution', options.textureSize);
  setWidget(graph, NODE_IDS.aoBake, 'resolution', options.textureSize);
  setWidget(graph, NODE_IDS.aoBake, 'samples', options.aoSamples);
  setWidget(graph, NODE_IDS.uvAtlas, 'resolution', options.textureSize);
  setWidget(graph, NODE_IDS.remesh, 'resolution', options.remesh);
  setWidget(graph, NODE_IDS.upsample, 'target_resolution', String(options.upsample));
  for (const id of [3, 12, 18, 23]) setWidget(graph, id, 'seed', options.seed);
  return graph;
}

/**
 * Fail-closed conversion of linked input sockets + named widget inputs.
 * Only nodes feeding the selected GLB Save node are included. Unsupported
 * extensions or schema mismatches surface as errors BEFORE /prompt.
 */
export function toApiPrompt(graph: LiteGraph, objectInfo: ObjectInfo, outputId = NODE_IDS.save3d): PreparedGraph {
  const nodes = new Map(graph.nodes.map((n) => [String(n.id), n]));
  const links = new Map(graph.links.map((l) => [l[0], l]));
  const prompt: ApiPrompt = {};
  const inProgress = new Set<string>();
  const done = new Set<string>();
  const types = new Set<string>();

  function visit(nodeId: string): void {
    if (done.has(nodeId)) return;
    if (inProgress.has(nodeId)) throw new Error(`Workflow'da döngü tespit edildi: ${nodeId}`);
    const node = nodes.get(nodeId);
    if (!node) throw new Error(`Workflow bağlantısı bulunmayan node'a işaret ediyor: ${nodeId}`);
    if (VISUAL_ONLY.has(node.type)) throw new Error(`Görsel node backend'e bağlanamaz: ${node.type}`);
    if (node.mode && node.mode !== 0) throw new Error(`Pasif/muted node #${node.id} desteklenmiyor.`);
    const definition = objectInfo[node.type];
    if (!definition) throw new Error(`ComfyUI'de gerekli node bulunmuyor: ${node.type}`);
    inProgress.add(nodeId);
    const inputs: JsonRecord = {};
    const spec = { ...definition.input?.required, ...definition.input?.optional };
    const inputSockets = new Map((node.inputs ?? []).map((input) => [input.name, input]));
    for (const [name, meta] of Object.entries(spec)) {
      const socket = inputSockets.get(name);
      const link = socket?.link === null || socket?.link === undefined ? null : links.get(socket.link);
      if (socket?.link != null && !link) throw new Error(`#${nodeId} ${name}: bağlantı ID ${socket.link} bulunamadı.`);
      if (link) {
        if (String(link[3]) !== nodeId) throw new Error(`Bağlantı hedefi eşleşmiyor: ${socket?.link}`);
        const src = String(link[1]);
        visit(src);
        inputs[name] = [src, link[2]];
      } else if (node.widgets_values_named && Object.prototype.hasOwnProperty.call(node.widgets_values_named, name)) {
        inputs[name] = node.widgets_values_named[name];
      } else {
        const isRequired = Object.prototype.hasOwnProperty.call(definition.input?.required ?? {}, name);
        const cfg = Array.isArray(meta) ? meta[1] : undefined;
        if (cfg && typeof cfg === 'object' && Object.prototype.hasOwnProperty.call(cfg, 'default')) {
          inputs[name] = (cfg as JsonRecord).default;
        } else if (isRequired) {
          throw new Error(`#${nodeId} (${node.type}): '${name}' girdisi eksik. ComfyUI sürümüyle workflow uyumunu kontrol et.`);
        }
      }
    }
    prompt[nodeId] = { class_type: node.type, inputs, _meta: { title: node.title || node.type } };
    types.add(node.type);
    done.add(nodeId);
    inProgress.delete(nodeId);
  }
  visit(String(outputId));
  if (!prompt[String(outputId)]) throw new Error('Save3DAdvanced bulunamadı.');
  return { prompt, reachable: [...done], nodeClasses: [...types].sort() };
}

export function validatePilot(source: LiteGraph): void {
  const expected = new Map<number, string>([
    [122, 'LoadImage'], [186, 'DecimateMesh'], [288, 'PrimitiveInt'],
    [241, 'RemeshMesh'], [94, 'Trellis2UpsampleStage'], [322, 'Save3DAdvanced'],
  ]);
  for (const [id, type] of expected) {
    if (requireNode(source, id).type !== type) throw new Error(`#${id}: beklenen node türü ${type} değil.`);
  }
  if (!source.links.some((l) => l[1] === 285 && l[3] === 322)) {
    throw new Error('Kaynak grafın GLB çıkış bağlantısı beklenenden farklı.');
  }
}
