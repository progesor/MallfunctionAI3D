import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateApiProduction, prepareApiProduction, productionGraphChanges, assertInstalledNodes } from '../lib/api-workflow.ts';
import { DEFAULT_OPTIONS } from '../lib/workflow.ts';

const root=path.dirname(fileURLToPath(import.meta.url));
// This is the user's proven ComfyUI API export, not a synthetic small graph.
const raw=JSON.parse(fs.readFileSync(path.resolve(root,'../../../workflows/PLAYBOX_FoxPlush_pixal3d_api_v01.json'),'utf8'));
const stats=validateApiProduction(raw);
assert.equal(stats.mode,'pixal3d');
assert.equal(stats.nodeCount,63);
assert.ok(stats.outputNodes.includes('322'));
assert.ok(raw['322'].inputs.viewport_state.camera_info);
const original=JSON.stringify(raw);
const expectedSeeds=Object.fromEntries(['3','12','18','23'].map(id=>[id,raw[id].inputs.seed]));
assert.deepEqual(expectedSeeds,{'3':56,'12':43,'18':42,'23':42});

// Source-fidelity mode must not reconfigure anything except the uniquely uploaded image name.
const faithful=prepareApiProduction(raw,{...DEFAULT_OPTIONS,triangles:20000,textureSize:2048,seed:118},'fox_uploaded.png','trellis2');
assert.equal(JSON.stringify(raw),original,'source API export mutated');
assert.equal(faithful['122'].inputs.image,'fox_uploaded.png');
assert.equal(faithful['316'].inputs.value,false,'switch unexpectedly forced to TRELLIS.2');
assert.deepEqual(Object.fromEntries(['3','12','18','23'].map(id=>[id,faithful[id].inputs.seed])),expectedSeeds);
assert.equal(faithful['186'].inputs.target_face_count,raw['186'].inputs.target_face_count);
assert.equal(faithful['288'].inputs.value,raw['288'].inputs.value);
assert.equal(faithful['241'].inputs.resolution,raw['241'].inputs.resolution);
assert.deepEqual(faithful['322'].inputs.viewport_state,raw['322'].inputs.viewport_state);
assert.equal(faithful['322'].inputs.filename_prefix,raw['322'].inputs.filename_prefix);
assert.ok(faithful['323'],'additional 3D preview retained');
assert.ok(faithful['262'],'UV preview retained');
assert.deepEqual(productionGraphChanges(raw,faithful),['#122.image']);

// Explicit customization may edit geometry/bake, but must keep all original seeds by default.
const custom=prepareApiProduction(raw,{...DEFAULT_OPTIONS,triangles:5000,textureSize:2048,seed:118},'fox_uploaded.png','trellis2', {mode:'custom'});
assert.equal(custom['316'].inputs.value,true);
assert.equal(custom['186'].inputs.target_face_count,5000);
assert.equal(custom['288'].inputs.value,2048);
assert.deepEqual(custom['147'].inputs.texture_size,['288',0]);
assert.deepEqual(custom['196'].inputs.resolution,['288',0]);
assert.deepEqual(Object.fromEntries(['3','12','18','23'].map(id=>[id,custom[id].inputs.seed])),expectedSeeds);
assert.ok(custom['323'] && custom['262']);
const reseeded=prepareApiProduction(raw,{...DEFAULT_OPTIONS,seed:118},'fox_uploaded.png','pixal3d',{mode:'custom',overrideAllSeeds:true});
for (const id of ['3','12','18','23']) assert.equal(reseeded[id].inputs.seed,118);
assert.throws(()=>prepareApiProduction(raw,DEFAULT_OPTIONS,'../bad.png','pixal3d'),/dosya adı/);

const missingSave=structuredClone(raw);delete missingSave['322'];
assert.throws(()=>validateApiProduction(missingSave),/Save3DAdvanced/);
const maskOnly=structuredClone(raw);maskOnly['322'].inputs.model_3d=['303',0];
assert.throws(()=>validateApiProduction(maskOnly),/Save3DAdvanced/);
const viewportBad=structuredClone(raw);viewportBad['322'].inputs.viewport_state='';
assert.throws(()=>validateApiProduction(viewportBad),/viewport_state/);
const brokenRef=structuredClone(raw);brokenRef['285'].inputs.mesh=['999999',0];
assert.throws(()=>validateApiProduction(brokenRef),/999999/);
const info=Object.fromEntries([...new Set(Object.values(raw).map(n=>n.class_type))].map(x=>[x,{}]));
assertInstalledNodes(raw,info);
assert.throws(()=>assertInstalledNodes(raw,{Save3DAdvanced:{}}),/eksik/);
console.log('PASS: source-fidelity Pixal3D API workflow, exact 4 seed preservation, explicit opt-in overrides, full original 63-node graph and saved outputs');
