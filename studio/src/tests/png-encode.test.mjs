import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inflateSync } from 'node:zlib';
import { encodeRgbaPng } from '../lib/png-encode.ts';

function unpack(base64) {
  const png = Buffer.from(base64, 'base64');
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  const chunks = [];
  let at = 8;
  while (at < png.length) {
    const size = png.readUInt32BE(at);
    const type = png.toString('ascii', at + 4, at + 8);
    const data = png.subarray(at + 8, at + 8 + size);
    // Validate every PNG chunk CRC with an independent bitwise implementation.
    let crc = 0xffffffff;
    for (const byte of png.subarray(at + 4, at + 8 + size)) {
      crc ^= byte;
      for (let j = 0; j < 8; j++) crc = crc >>> 1 ^ (crc & 1 ? 0xedb88320 : 0);
    }
    assert.equal((crc ^ 0xffffffff) >>> 0, png.readUInt32BE(at + 8 + size));
    chunks.push({ type, data });
    at += 12 + size;
    if (type === 'IEND') break;
  }
  assert.equal(at, png.length);
  assert.deepEqual(chunks.map(c => c.type), ['IHDR', 'IDAT', 'IEND']);
  const header = chunks[0].data;
  assert.equal(header[8], 8);
  assert.equal(header[9], 6);
  const width = header.readUInt32BE(0), height = header.readUInt32BE(4);
  const rows = inflateSync(chunks[1].data);
  const raw = [];
  for (let y = 0; y < height; y++) {
    assert.equal(rows[y * (width * 4 + 1)], 0);
    raw.push(...rows.subarray(y * (width * 4 + 1) + 1, (y + 1) * (width * 4 + 1)));
  }
  return { width, height, rgba: raw };
}

test('lossless GPU-independent PNG retains exact RGBA and valid PNG CRC/IDAT', async () => {
  const original = Uint8ClampedArray.from([30, 200, 0, 255, 255, 30, 128, 48]);
  const encoded = await encodeRgbaPng(original, 2, 1);
  assert.deepEqual(unpack(encoded), { width: 2, height: 1, rgba: [...original] });
});

test('PNG preserves multiple scanlines and opaque/transparent channel values', async () => {
  const original = new Uint8ClampedArray(4 * 5 * 3);
  for (let i = 0; i < original.length; i++) original[i] = (i * 73) % 256;
  assert.deepEqual(unpack(await encodeRgbaPng(original, 5, 3)),
    { width: 5, height: 3, rgba: [...original] });
});

test('malformed dimensions and incomplete RGBA cannot encode a PNG', async () => {
  await assert.rejects(encodeRgbaPng(new Uint8ClampedArray(4), 2, 2));
  await assert.rejects(encodeRgbaPng(new Uint8ClampedArray(4), 0, 1));
  await assert.rejects(encodeRgbaPng(new Uint8ClampedArray(4), 9000, 1));
});
