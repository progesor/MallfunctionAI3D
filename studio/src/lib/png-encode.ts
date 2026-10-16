/** Deterministic, GPU-independent RGBA PNG encoder for WebView2/Chromium.
 * Canvas.toBlob() can stall while a software WebGL context is busy on Windows.
 * Uses browser's standards-based deflate CompressionStream, never GPU/canvas.
 */
const SIGNATURE = Uint8Array.from([137,80,78,71,13,10,26,10]);

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < table.length; i++) {
    let crc = i;
    for (let bit = 0; bit < 8; bit++) {
      crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    table[i] = crc >>> 0;
  }
  return table;
})();

function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of data) crc = (crc >>> 8) ^ CRC_TABLE[(crc ^ byte) & 255];
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length, false);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)), false);
  return out;
}

function toBase64(bytes: Uint8Array): string {
  // Avoid String.fromCharCode(...largeBuffer) stack overflows on 4K textures.
  const chars: string[] = [];
  for (let start = 0; start < bytes.length; start += 16_384) {
    chars.push(String.fromCharCode(...bytes.subarray(start, start + 16_384)));
  }
  return btoa(chars.join(''));
}

/** Encodes 8-bit RGBA in lossless PNG. No gamma/color-space conversion,
 * so source sRGB albedo and linear/normal scalar bytes remain as prepared.
 */
export async function encodeRgbaPng(rgba: Uint8ClampedArray, width: number, height: number): Promise<string> {
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) ||
      width <= 0 || height <= 0 || width > 8192 || height > 8192 ||
      width * height > 32_000_000 || rgba.length !== width * height * 4) {
    throw new Error('PNG RGBA dimensions/buffer length invalid');
  }
  if (typeof CompressionStream === 'undefined') {
    throw new Error('PNG encoding requires CompressionStream (modern WebView2/Chromium)');
  }
  const stride = width * 4;
  const raw = new Uint8Array(height * (stride + 1));
  for (let y = 0; y < height; y++) {
    // Filter method 0 / type 0: every row includes one zero filter byte.
    raw.set(rgba.subarray(y * stride, (y + 1) * stride), y * (stride + 1) + 1);
  }
  const compressed = new Uint8Array(
    await new Response(new Blob([raw]).stream().pipeThrough(new CompressionStream('deflate'))).arrayBuffer()
  );
  const header = new Uint8Array(13);
  const view = new DataView(header.buffer);
  view.setUint32(0, width, false);
  view.setUint32(4, height, false);
  header[8] = 8;  // RGBA each 8 bits
  header[9] = 6;  // truecolor + alpha
  const ihdr = chunk('IHDR', header), idat = chunk('IDAT', compressed), iend = chunk('IEND', new Uint8Array());
  const out = new Uint8Array(SIGNATURE.length + ihdr.length + idat.length + iend.length);
  let offset = 0;
  for (const item of [SIGNATURE, ihdr, idat, iend]) {
    out.set(item, offset);
    offset += item.length;
  }
  return toBase64(out);
}
