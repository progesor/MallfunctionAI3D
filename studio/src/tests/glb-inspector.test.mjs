import assert from 'node:assert/strict';
import fs from 'node:fs';
import { test } from 'node:test';
import {
  inspectGlbTextures, readGlbChunks, grayscaleChannel, decodeGlbBase64,
} from '../lib/glb-inspector.ts';

const png = Uint8Array.from([137,80,78,71,13,10,26,10,0,0,0,0]);
function fixture({ materials, imageData = png, malformedView = false } = {}) {
  const entries = materials ?? [{ name:'Fabric', pbrMetallicRoughness:{baseColorTexture:{index:0},metallicRoughnessTexture:{index:1}}, normalTexture:{index:2}, occlusionTexture:{index:1}}];
  const imageLen = imageData.length;
  const bin = new Uint8Array(imageLen*3);
  bin.set(imageData,0);bin.set(imageData,imageLen);bin.set(imageData,imageLen*2);
  const metadata = {
    asset:{version:'2.0'},
    buffers:[{byteLength:bin.length}],
    bufferViews:Array.from({length:3},(_,i)=>({buffer:0,byteOffset:malformedView&&i===2 ? 100000:i*imageLen,byteLength:imageLen})),
    images:Array.from({length:3},(_,i)=>({bufferView:i,mimeType:'image/png'})),
    textures:[{source:0},{source:1},{source:2}],
    materials:entries,
  };
  const content=Buffer.from(JSON.stringify(metadata),'utf-8');
  const size=content.length+(4-content.length%4)%4;
  const padded=Buffer.alloc(size,0x20);content.copy(padded);
  const out=Buffer.alloc(12+8+size+8+bin.length);
  out.write('glTF',0);out.writeUInt32LE(2,4);out.writeUInt32LE(out.length,8);
  out.writeUInt32LE(size,12);out.write('JSON',16);padded.copy(out,20);
  out.writeUInt32LE(bin.length,20+size);out.write('BIN\0',24+size);Buffer.from(bin).copy(out,28+size);
  return new Uint8Array(out);
}

test('PBR packed ORM exports distinct AO/Roughness/Metallic channels', () => {
  const result = inspectGlbTextures(fixture());
  assert.equal(result.imageCount,3);
  assert.deepEqual(result.maps.map(x => x.role), ['baseColor','normal','roughness','metallic','occlusion']);
  assert.deepEqual(result.maps.map(x=>x.channel),['rgb','rgb','g','b','r']);
  assert.equal(result.maps[2].imageIndex,result.maps[3].imageIndex);
  assert.equal(result.maps[3].imageIndex,result.maps[4].imageIndex);
  assert.equal(result.maps[0].image.bytes.length,png.length);
  assert.deepEqual(result.warnings,[]);
});

test('multi-material GLB maps are grouped by material', () => {
  const materials=[
    {name:'Paint',pbrMetallicRoughness:{baseColorTexture:{index:0}}},
    {name:'Metal',pbrMetallicRoughness:{metallicRoughnessTexture:{index:1}}, normalTexture:{index:2}},
  ];
  const result=inspectGlbTextures(fixture({materials}));
  assert.deepEqual(result.materialNames,['Paint','Metal']);
  assert.deepEqual(result.maps.map(x=>x.materialIndex),[0,1,1,1]);
});

test('grayscale isolates the specified packed texture channel', () => {
  const original=Uint8ClampedArray.from([9,144,246,33,17,88,11,99]);
  assert.deepEqual([...grayscaleChannel(original,'r')],[9,9,9,255,17,17,17,255]);
  assert.deepEqual([...grayscaleChannel(original,'g')],[144,144,144,255,88,88,88,255]);
  assert.deepEqual([...grayscaleChannel(original,'b')],[246,246,246,255,11,11,11,255]);
  assert.deepEqual([...original],[9,144,246,33,17,88,11,99]);
});

test('GLB parsing rejects corrupt chunk bounds and image offsets', () => {
  const wrong=fixture();wrong[8]=0;
  assert.throws(()=>readGlbChunks(wrong),/başlığı|boyutu/);
  assert.throws(()=>inspectGlbTextures(fixture({malformedView:true})),/byte aralığı/);
});

test('base64 source decoding roundtrips without mutating input', () => {
  const binary=fixture();const base64=Buffer.from(binary).toString('base64');
  const result=inspectGlbTextures(decodeGlbBase64(base64));
  assert.equal(result.maps.length,5);
  assert.equal(result.imageCount,3);
});

// Optional integration against the user's real first Pixal3D GLB when supplied.
// CI does not require or check in large/private assets.
const glbPath=process.env.MALLFUNCTION_TEST_GLB;
if (glbPath && fs.existsSync(glbPath)) {
  test('first Pixal3D GLB contains valid PBR role links', () => {
    const bytes=new Uint8Array(fs.readFileSync(glbPath));
    const result=inspectGlbTextures(bytes);
    assert.equal(result.imageCount,3);
    assert.deepEqual(result.maps.map(x=>x.role), ['baseColor','normal','roughness','metallic','occlusion']);
    assert.equal(result.maps.length,5);
    assert.deepEqual(result.warnings,[]);
  });
}
