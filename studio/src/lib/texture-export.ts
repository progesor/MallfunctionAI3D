/** Non-destructive PBR map export. Pure resizing is testable without a DOM. */
import type { EmbeddedTexture, TextureMap, TextureRole } from './glb-inspector';
import { encodeRgbaPng } from './png-encode.ts';

export type TexturePreset = 'source' | '1024' | '512' | '256';
export type TextureExportFile = {
  filename: string;
  role: TextureRole;
  materialIndex: number;
  sourceImageIndex: number;
  sourceChannel: 'rgb' | 'r' | 'g' | 'b';
  width: number;
  height: number;
  contentBase64: string;
};

/** Never upscale source maps; preserve non-square aspect ratios. */
export function exportSize(width: number, height: number, preset: TexturePreset): [number, number] {
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 || width > 8192 || height > 8192) {
    throw new Error('Görüntü boyutu 1–8192 px aralığında olmalı.');
  }
  if (preset === 'source') return [width, height];
  const maxEdge = Number(preset);
  if (![256, 512, 1024].includes(maxEdge)) throw new Error('Bilinmeyen texture export profili.');
  const scale = Math.min(1, maxEdge / Math.max(width, height));
  return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
}

export function mapFilename(map: Pick<TextureMap, 'materialIndex' | 'role'>): string {
  if (!Number.isInteger(map.materialIndex) || map.materialIndex < 0 || map.materialIndex > 63 ||
      !['baseColor', 'normal', 'roughness', 'metallic', 'occlusion', 'emissive'].includes(map.role)) {
    throw new Error('Geçersiz PBR harita kaydı.');
  }
  return `m${map.materialIndex}_${map.role}.png`;
}

const srgbLinear = new Float64Array(256);
for (let i = 0; i < 256; i++) {
  const v = i / 255;
  srgbLinear[i] = v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}
function linearToByte(l: number): number {
  const clamped = Math.min(1, Math.max(0, l));
  const v = clamped <= 0.0031308 ? clamped * 12.92 : 1.055 * (clamped ** (1 / 2.4)) - 0.055;
  return Math.round(v * 255);
}

/** Area-filter resampling, no browser interpolation dependency.
 * Color/emissive resample in linear light (premultiplied alpha), scalar maps in
 * linear channel values, normals in vector space with unit renormalization.
 * RGB output for scalar channels is grayscale PNG, never the packed source.
 */
export function resamplePbr(
  src: Uint8ClampedArray,
  sourceWidth: number,
  sourceHeight: number,
  targetWidth: number,
  targetHeight: number,
  role: TextureRole,
  channel: 'rgb' | 'r' | 'g' | 'b',
): Uint8ClampedArray {
  if (![sourceWidth, sourceHeight, targetWidth, targetHeight].every(n => Number.isSafeInteger(n) && n > 0 && n <= 8192) ||
      src.length !== sourceWidth * sourceHeight * 4 || targetWidth > sourceWidth || targetHeight > sourceHeight) {
    throw new Error('Geçersiz texture yeniden boyutlandırma değerleri.');
  }
  if (sourceWidth === targetWidth && sourceHeight === targetHeight && channel === 'rgb') return new Uint8ClampedArray(src);
  const isColor = (role === 'baseColor' || role === 'emissive') && channel === 'rgb';
  const isNormal = role === 'normal' && channel === 'rgb';
  const component = channel === 'r' ? 0 : channel === 'g' ? 1 : channel === 'b' ? 2 : -1;
  const out = new Uint8ClampedArray(targetWidth * targetHeight * 4);
  const fx = sourceWidth / targetWidth;
  const fy = sourceHeight / targetHeight;
  for (let y = 0; y < targetHeight; y++) {
    const top = y * fy, bottom = (y + 1) * fy;
    for (let x = 0; x < targetWidth; x++) {
      const left = x * fx, right = (x + 1) * fx;
      const sums = [0, 0, 0, 0];
      let total = 0;
      for (let sy = Math.floor(top); sy < Math.ceil(bottom); sy++) {
        const wy = Math.min(bottom, sy + 1) - Math.max(top, sy);
        if (wy <= 0) continue;
        for (let sx = Math.floor(left); sx < Math.ceil(right); sx++) {
          const weight = wy * (Math.min(right, sx + 1) - Math.max(left, sx));
          if (weight <= 0) continue;
          const pos = (sy * sourceWidth + sx) * 4;
          if (component >= 0) {
            sums[0] += src[pos + component] * weight;
          } else if (isColor) {
            const alpha = src[pos + 3] / 255;
            for (let c = 0; c < 3; c++) sums[c] += srgbLinear[src[pos + c]] * alpha * weight;
            sums[3] += alpha * weight;
          } else if (isNormal) {
            for (let c = 0; c < 3; c++) sums[c] += ((src[pos + c] / 255) * 2 - 1) * weight;
          } else {
            for (let c = 0; c < 3; c++) sums[c] += src[pos + c] * weight;
          }
          total += weight;
        }
      }
      const dest = (y * targetWidth + x) * 4;
      if (component >= 0) {
        const v = Math.round(sums[0] / total);
        out[dest] = v; out[dest + 1] = v; out[dest + 2] = v; out[dest + 3] = 255;
      } else if (isColor) {
        const alphaSum = sums[3];
        for (let c = 0; c < 3; c++) out[dest + c] = linearToByte(alphaSum ? sums[c] / alphaSum : 0);
        out[dest + 3] = Math.round((alphaSum / total) * 255);
      } else if (isNormal) {
        const length = Math.hypot(sums[0], sums[1], sums[2]) || 1;
        for (let c = 0; c < 3; c++) out[dest + c] = Math.round((sums[c] / length * 0.5 + 0.5) * 255);
        out[dest + 3] = 255;
      } else {
        for (let c = 0; c < 3; c++) out[dest + c] = Math.round(sums[c] / total);
        out[dest + 3] = 255;
      }
    }
  }
  return out;
}

async function decodeImage(image: EmbeddedTexture): Promise<{ rgba: Uint8ClampedArray; width: number; height: number }> {
  // Decode bytes directly. Blob URLs can be revoked while a background image
  // decode is still pending after React remounts or GPU-heavy previews.
  // ImageBitmap is supported in both modern WebView2 and Chromium tests.
  const source = new Blob([new Uint8Array(image.bytes)], { type: image.mimeType });
  const bitmap = await createImageBitmap(source);
  try {
    const width = bitmap.width, height = bitmap.height;
    if (width < 1 || height < 1 || width > 8192 || height > 8192 || width * height > 32_000_000) {
      throw new Error('Görüntü güvenli decode boyutu sınırını aşıyor.');
    }
    const canvas = document.createElement('canvas');
    canvas.width = width; canvas.height = height;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!ctx) throw new Error('2D Canvas açılamadı.');
    ctx.drawImage(bitmap, 0, 0);
    return { rgba: ctx.getImageData(0, 0, width, height).data, width, height };
  } finally { bitmap.close(); }
}

/** Only the chosen material's semantic roles are exported; unique glTF images can be shared. */
export async function prepareTextureExports(
  maps: TextureMap[], preset: TexturePreset,
  onProgress?: (current: number, total: number) => void,
): Promise<TextureExportFile[]> {
  if (!maps.length || maps.length > 16) throw new Error('Bir materyalde 1–16 PBR haritası olmalı.');
  const decoded = new Map<number, Awaited<ReturnType<typeof decodeImage>>>();
  const files: TextureExportFile[] = [];
  for (const [index, map] of maps.entries()) {
    if (!decoded.has(map.imageIndex)) decoded.set(map.imageIndex, await decodeImage(map.image));
    const image = decoded.get(map.imageIndex)!;
    const [width, height] = exportSize(image.width, image.height, preset);
    const pixels = resamplePbr(image.rgba, image.width, image.height, width, height, map.role, map.channel);
    files.push({ filename: mapFilename(map), role: map.role, materialIndex: map.materialIndex,
      sourceImageIndex: map.imageIndex, sourceChannel: map.channel, width, height,
      contentBase64: await encodeRgbaPng(pixels, width, height) });
    onProgress?.(index + 1, maps.length);
  }
  if (new Set(files.map(x => x.filename)).size !== files.length) throw new Error('Tekrarlanan harita adı bulundu.');
  return files;
}
