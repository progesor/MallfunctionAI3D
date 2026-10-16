import assert from 'node:assert/strict';
import { preparePilot, toApiPrompt, validatePilot, DEFAULT_OPTIONS } from '../lib/workflow.ts';

const make=(id,type, widgets_values_named={},widgets_values=[],inputs=[])=>({ id,type,widgets_values_named, widgets_values,inputs });
const nodes=[
  make(122,'LoadImage',{image:'old.png',upload:'image'},['old.png','image']),
  make(186,'DecimateMesh',{target_face_count:8000},[8000]),
  make(288,'PrimitiveInt',{value:1024},[1024,'fixed']),
  make(147,'BakeTextureFromVoxel',{texture_size:1024},[1024]),
  make(196,'UnwrapMesh',{resolution:1024},['pec',1024,1,.0002]),
  make(224,'BakeNormalMapFromMesh',{resolution:1024},[1024]),
  make(233,'BakeAmbientOcclusion',{resolution:1024,samples:32},[1024,32]),
  make(261,'RenderUVAtlas',{resolution:1024},[1024]),
  make(241,'RemeshMesh',{resolution:512},[512]),
  make(94,'Trellis2UpsampleStage',{target_resolution:'1024'},['1024']),
  ...[3,12,18,23].map(id=>make(id,'KSampler',{seed:42},[42])),
  make(285,'MeshToFile3D',{},[],[{name:'mesh',link:1}]),
  make(322,'Save3DAdvanced',{filename_prefix:'3d/Mallfunction/fox'},['3d/Mallfunction/fox'],[{name:'model_3d',link:2}]),
];
const links=[[1,122,0,285,0,'IMAGE'],[2,285,0,322,0,'FILE3D']];
const graph={nodes,links};
validatePilot(graph);
assert.equal(graph.nodes[0].widgets_values_named.image,'old.png');
const next=preparePilot(graph,{...DEFAULT_OPTIONS, triangles:5000,textureSize:2048,seed:99},'fox.png');
assert.equal(nodes[0].widgets_values_named.image,'old.png');
assert.equal(next.nodes.find(n=>n.id===186).widgets_values_named.target_face_count,5000);
assert.equal(next.nodes.find(n=>n.id===196).widgets_values[1],2048);
assert.equal(next.nodes.find(n=>n.id===94).widgets_values[0],'1024');
assert.throws(()=>preparePilot(graph,{...DEFAULT_OPTIONS, textureSize:8192},'fox.png'),/Texture/);
assert.throws(()=>preparePilot(graph,DEFAULT_OPTIONS,'../fake.png'),/geçerli/);
const def={
 LoadImage:{input:{required:{image:['STRING',{}]}}},
 MeshToFile3D:{input:{required:{mesh:['IMAGE',{}]}}},
 Save3DAdvanced:{input:{required:{model_3d:['FILE3D',{}],filename_prefix:['STRING',{}]}}},
};
const output=toApiPrompt(next,def);
assert.equal(Object.keys(output.prompt).length,3);
assert.deepEqual(output.prompt['322'].inputs.model_3d,['285',0]);
assert.deepEqual(output.prompt['285'].inputs.mesh,['122',0]);
assert.equal(output.prompt['322'].inputs.filename_prefix,'3d/Mallfunction/fox');
assert.equal(output.prompt['122'].inputs.image,'fox.png');
assert.throws(()=>toApiPrompt(next,{...def,MeshToFile3D:undefined}),/gerekli node/);
const broken=structuredClone(next);broken.links[0][1]=999;
assert.throws(()=>toApiPrompt(broken,def),/bağlantısı bulunmayan/);
console.log('PASS: workflow adapter — validation, immutability, widget edit, API conversion, missing-node safety');
