/** Read embedded PBR maps directly from a GLB; never modifies source data.
 *
 * Only bufferView and data: image payloads are accepted. Referenced external
 * files/URLs are deliberately not followed by the Studio inspector.
 */
export type TextureRole = 'baseColor' | 'normal' | 'roughness' | 'metallic' | 'occlusion' | 'emissive';
export type TextureChannel = 'rgb' | 'r' | 'g' | 'b';
export interface EmbeddedTexture {
  imageIndex: number;
  mimeType: 'image/png' | 'image/jpeg' | 'image/webp';
  bytes: Uint8Array;
  name: string;
}
export interface TextureMap {
  key: string;
  materialIndex: number;
  materialName: string;
  role: TextureRole;
  channel: TextureChannel;
  textureIndex: number;
  imageIndex: number;
  image: EmbeddedTexture;
}
export interface GlbTextureInspection {
  materialNames: string[];
  maps: TextureMap[];
  imageCount: number;
  warnings: string[];
}

const roles: Array<[TextureRole, TextureChannel, (m: Record<string, unknown>) => unknown]> = [
  ['baseColor', 'rgb', m => (m.pbrMetallicRoughness as Record<string, unknown> | undefined)?.baseColorTexture],
  ['normal', 'rgb', m => m.normalTexture],
  ['roughness', 'g', m => (m.pbrMetallicRoughness as Record<string, unknown> | undefined)?.metallicRoughnessTexture],
  ['metallic', 'b', m => (m.pbrMetallicRoughness as Record<string, unknown> | undefined)?.metallicRoughnessTexture],
  ['occlusion', 'r', m => m.occlusionTexture],
  ['emissive', 'rgb', m => m.emissiveTexture],
];
const MIME = new Set(['image/png', 'image/jpeg', 'image/webp']);

function uint32(bytes: Uint8Array, offset: number): number {
  if (offset + 4 > bytes.length) throw new Error('GLB başlık verisi eksik.');
  return new DataView(bytes.buffer, bytes.byteOffset + offset, 4).getUint32(0, true);
}

/** Parse GLB JSON and original binary chunk, with strict bounds checks. */
export function readGlbChunks(bytes: Uint8Array): { json: Record<string, unknown>; bin: Uint8Array } {
  if (bytes.length < 20 || bytes.length > 100_000_000) throw new Error('GLB boyutu geçersiz (en fazla 100 MB).');
  if (uint32(bytes, 0) !== 0x46546c67 || uint32(bytes, 4) !== 2 || uint32(bytes, 8) !== bytes.length) {
    throw new Error('GLB 2.0 başlığı veya toplam dosya boyutu geçersiz.');
  }
  let cursor = 12;
  let json: Record<string, unknown> | null = null;
  let bin: Uint8Array = new Uint8Array(0);
  while (cursor + 8 <= bytes.length) {
    const size = uint32(bytes, cursor);
    const type = uint32(bytes, cursor + 4);
    cursor += 8;
    if (size > bytes.length - cursor || size % 4 !== 0) throw new Error('GLB chunk sınırları geçersiz.');
    if (type === 0x4e4f534a && !json) {
      const content = new TextDecoder().decode(bytes.subarray(cursor, cursor + size)).trim();
      const parsed: unknown = JSON.parse(content);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('GLB JSON nesnesi geçersiz.');
      json = parsed as Record<string, unknown>;
    } else if (type === 0x004e4942 && bin.length === 0) {
      bin = bytes.subarray(cursor, cursor + size);
    }
    cursor += size;
  }
  if (cursor !== bytes.length || !json) throw new Error('GLB JSON verisi eksik.');
  return { json, bin };
}

function object(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}
function array(value: unknown): unknown[] { return Array.isArray(value) ? value : []; }
function integer(value: unknown): number | null { return Number.isInteger(value) && (value as number) >= 0 ? value as number : null; }

function imageFromJson(item: unknown, index: number, views: unknown[], bin: Uint8Array): EmbeddedTexture | null {
  const image = object(item);
  if (!image) return null;
  const name = typeof image.name === 'string' && image.name ? image.name : `Image ${index + 1}`;
  const viewIndex = integer(image.bufferView);
  if (viewIndex !== null) {
    const view = object(views[viewIndex]);
    if (!view || (view.buffer !== undefined && view.buffer !== 0)) throw new Error(`Image ${index}: geçersiz bufferView.`);
    const start = integer(view.byteOffset ?? 0);
    const size = integer(view.byteLength);
    if (start === null || size === null || size === 0 || size > bin.length || start > bin.length - size) {
      throw new Error(`Image ${index}: bufferView byte aralığı geçersiz.`);
    }
    const mime = image.mimeType;
    if (typeof mime !== 'string' || !MIME.has(mime)) throw new Error(`Image ${index}: desteklenmeyen MIME tipi.`);
    return { imageIndex: index, name, mimeType: mime as EmbeddedTexture['mimeType'], bytes: bin.subarray(start, start + size) };
  }
  // Data URI is an allowed inline image source. Never fetch an external URI.
  if (typeof image.uri === 'string' && image.uri.startsWith('data:')) {
    const match = /^data:(image\/(?:png|jpeg|webp));base64,([a-zA-Z0-9+/=]+)$/.exec(image.uri);
    if (!match) throw new Error(`Image ${index}: geçersiz gömülü data URI.`);
    const base64 = match[2];
    if (base64.length > 80_000_000) throw new Error(`Image ${index}: görüntü boyutu çok büyük.`);
    const decoded = atob(base64);
    const bytes = Uint8Array.from(decoded, c => c.charCodeAt(0));
    return { imageIndex: index, name, mimeType: match[1] as EmbeddedTexture['mimeType'], bytes };
  }
  return null;
}

/** Lists map *roles*, not unique image files. AO and roughness can share a packed image. */
export function inspectGlbTextures(bytes: Uint8Array): GlbTextureInspection {
  const { json, bin } = readGlbChunks(bytes);
  const materials = array(json.materials);
  const textures = array(json.textures);
  const images = array(json.images);
  const bufferViews = array(json.bufferViews);
  const warnings: string[] = [];
  const decodedImages = images.map((img, idx) => {
    const result = imageFromJson(img, idx, bufferViews, bin);
    if (!result) warnings.push(`Image ${idx}: GLB dışındaki görüntü bağlantısı açılmadı.`);
    return result;
  });
  const maps: TextureMap[] = [];
  const materialNames = materials.map((item, idx) => {
    const material = object(item);
    return typeof material?.name === 'string' && material.name ? material.name : `Material ${idx + 1}`;
  });
  for (let mi = 0; mi < materials.length; mi++) {
    const material = object(materials[mi]);
    if (!material) continue;
    for (const [role, channel, pick] of roles) {
      const property = object(pick(material));
      const textureIndex = integer(property?.index);
      if (textureIndex === null) continue;
      const texture = object(textures[textureIndex]);
      const imageIndex = integer(texture?.source);
      if (imageIndex === null || imageIndex >= decodedImages.length || !decodedImages[imageIndex]) {
        warnings.push(`Material ${mi} / ${role}: görüntü bulunamadı veya harici.`);
        continue;
      }
      maps.push({ key: `${mi}:${role}`, materialIndex: mi, materialName: materialNames[mi],
        role, channel, textureIndex, imageIndex, image: decodedImages[imageIndex]! });
    }
  }
  return { maps, materialNames, imageCount: images.length, warnings };
}

export function decodeGlbBase64(base64: string): Uint8Array {
  if (!base64 || base64.length > 135_000_000) throw new Error('GLB verisi çok büyük veya boş.');
  const binary = atob(base64);
  return Uint8Array.from(binary, c => c.charCodeAt(0));
}

export const MAP_LABELS: Record<TextureRole, string> = {
  baseColor: 'Base Color', normal: 'Normal', roughness: 'Roughness',
  metallic: 'Metallic', occlusion: 'AO', emissive: 'Emissive',
};

/** Pure channel extraction; preview must be grayscale, never RGB packed data. */
export function grayscaleChannel(rgba: Uint8ClampedArray, channel: 'r' | 'g' | 'b'): Uint8ClampedArray {
  const offset = channel === 'r' ? 0 : channel === 'g' ? 1 : 2;
  const out = new Uint8ClampedArray(rgba.length);
  for (let i = 0; i < rgba.length; i += 4) {
    const value = rgba[i + offset];
    out[i] = value; out[i + 1] = value; out[i + 2] = value; out[i + 3] = 255;
  }
  return out;
}
