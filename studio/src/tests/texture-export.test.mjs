import assert from 'node:assert/strict';
import { test } from 'node:test';
import { exportSize, mapFilename, resamplePbr } from '../lib/texture-export.ts';

test('resolution presets never upscale and respect aspect ratio', () => {
  assert.deepEqual(exportSize(1024,512,'512'), [512,256]);
  assert.deepEqual(exportSize(320,160,'512'), [320,160]);
  assert.deepEqual(exportSize(1536,1024,'1024'), [1024,683]);
  assert.deepEqual(exportSize(1200,1600,'256'), [192,256]);
  assert.throws(()=> exportSize(9000,1024,'1024'));
});

test('roughness channel G is isolated and box-averaged; not RGB packed', () => {
  const src=Uint8ClampedArray.from([10,20,30,255, 200,220,250,255]);
  const out=resamplePbr(src,2,1,1,1,'roughness','g');
  assert.deepEqual([...out],[120,120,120,255]);
  const metallic=resamplePbr(src,2,1,1,1,'metallic','b');
  assert.deepEqual([...metallic],[140,140,140,255]);
  const ao=resamplePbr(src,2,1,1,1,'occlusion','r');
  assert.deepEqual([...ao],[105,105,105,255]);
});

test('color maps downsample in linear light rather than gamma-incorrect 128', () => {
  const src=Uint8ClampedArray.from([0,0,0,255, 255,255,255,255]);
  const out=resamplePbr(src,2,1,1,1,'baseColor','rgb');
  assert.ok(out[0] >= 187 && out[0] <= 189, `expected approximately 188, got ${out[0]}`);
  assert.deepEqual([out[1],out[2],out[3]],[out[0],out[0],255]);
});

test('normal map is renormalized after averaging and retains tangent-space orientation', () => {
  const src=Uint8ClampedArray.from([255,128,128,255, 128,128,255,255]);
  const out=resamplePbr(src,2,1,1,1,'normal','rgb');
  const nx=out[0]/255*2-1, ny=out[1]/255*2-1, nz=out[2]/255*2-1;
  assert.ok(Math.abs(Math.hypot(nx,ny,nz)-1)<0.02);
  assert.ok(nx>0.5 && nz>0.5);
});

test('source mode keeps RGB bytes unmodified and names are safe', () => {
  const src=Uint8ClampedArray.from([1,2,3,40, 5,6,7,90]);
  assert.deepEqual([...resamplePbr(src,2,1,2,1,'normal','rgb')],[...src]);
  assert.equal(mapFilename({materialIndex:0,role:'normal'}),'m0_normal.png');
  assert.throws(()=>mapFilename({materialIndex:-1,role:'normal'}));
  assert.throws(()=>resamplePbr(src,2,1,4,1,'baseColor','rgb'));
});

test('RGBA alpha is weighted when resampling color maps', () => {
  const src=Uint8ClampedArray.from([255,0,0,255, 0,0,255,0]);
  const out=resamplePbr(src,2,1,1,1,'baseColor','rgb');
  assert.equal(out[0],255);
  assert.equal(out[1],0);
  assert.equal(out[2],0);
  assert.equal(out[3],128);
});
