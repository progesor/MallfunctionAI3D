/** ComfyUI API JSON is the authoritative executable format for 3D. */
import type { ApiPrompt, GenerateOptions, ObjectInfo } from './workflow';
export type ApiWorkflow=ApiPrompt;
export type WorkflowDocument={format:'api';prompt:ApiWorkflow}|{format:'litegraph';graph:unknown};
export interface ApiGraphInfo {nodeCount:number;outputNodes:string[];mode:'pixal3d'|'trellis2'}
const REQUIRED:Record<string,string>={
 '122':'LoadImage','186':'DecimateMesh','288':'PrimitiveInt','241':'RemeshMesh',
 '285':'MeshToFile3D','316':'PrimitiveBoolean','322':'Save3DAdvanced'
};
const MODEL_STAGES=new Set(['KSampler','Trellis2ShapeStage','Trellis2UpsampleStage','Trellis2TextureStage']);
function entry(prompt:ApiWorkflow,id:string,expected?:string){
 const node=prompt[id];
 if(!node || (expected && node.class_type!==expected))
  throw new Error('#'+id+' '+(expected??'node')+' bulunamadı. Pixal3D/TRELLIS.2 API workflow seç.');
 if(!node.inputs || typeof node.inputs!=='object')throw new Error('#'+id+' inputs geçersiz.');
 return node;
}
function isReference(value:unknown):value is [string,number]{
 return Array.isArray(value)&&value.length===2&&/^\d+$/.test(String(value[0]))
   &&Number.isInteger(value[1])&&Number(value[1])>=0;
}
export function validateApiProduction(prompt:ApiWorkflow):ApiGraphInfo{
 if(!prompt||typeof prompt!=='object'||Array.isArray(prompt))
  throw new Error('ComfyUI API JSON değil. ComfyUI > Export API Format seç.');
 const count=Object.keys(prompt).length;
 if(count<25||count>600)throw new Error('Üretim grafı boyutu geçersiz: '+count);
 for(const [id,type] of Object.entries(REQUIRED))entry(prompt,id,type);
 const save=entry(prompt,'322','Save3DAdvanced');
 if(!isReference(save.inputs.model_3d)||String(save.inputs.model_3d[0])!=='285')
  throw new Error('Save3DAdvanced #322 MeshToFile3D #285 çıkışına bağlı değil (mask-only).');
 if(!save.inputs.viewport_state||typeof save.inputs.viewport_state!=='object'||Array.isArray(save.inputs.viewport_state))
  throw new Error('Save3DAdvanced viewport_state yok. Workflow’u ComfyUI API formatında dışa aktar.');
 const visited=new Set<string>(),stack=new Set<string>();
 function visit(id:string):void{
  if(visited.has(id))return;
  if(stack.has(id))throw new Error('#'+id+' düğümünde döngü.');
  const node=entry(prompt,id);stack.add(id);
  for(const value of Object.values(node.inputs))if(isReference(value))visit(String(value[0]));
  stack.delete(id);visited.add(id);
 }
 visit('322');
 if(!visited.has('122')||visited.size<25||![...visited].some(id=>MODEL_STAGES.has(prompt[id].class_type)))
  throw new Error('GLB çıkışına bağlı tam 3D üretim zinciri yok (yalnızca mask/preview).');
 const switcher=entry(prompt,'316','PrimitiveBoolean').inputs.value;
 if(typeof switcher!=='boolean')throw new Error('Pixal3D/TRELLIS.2 seçici boolean değil.');
 const outputNodes=Object.entries(prompt).filter(([,node])=>node.class_type==='Save3DAdvanced'||node.class_type==='SaveGLB').map(([id])=>id);
 return {nodeCount:count,outputNodes,mode:switcher?'trellis2':'pixal3d'};
}
function writeLiteral(prompt:ApiWorkflow,id:string,expected:string,key:string,value:unknown){
 const node=entry(prompt,id,expected);
 if(!(key in node.inputs))throw new Error('#'+id+' ('+expected+'): '+key+' yok.');
 if(isReference(node.inputs[key]))throw new Error('#'+id+' '+key+' bir bağlantı; korunmalı.');
 node.inputs[key]=value;
}
function keepSaveDependencies(prompt:ApiWorkflow):ApiWorkflow{
 const wanted=new Set<string>();
 function walk(id:string){
  if(wanted.has(id))return;
  const node=entry(prompt,id);wanted.add(id);
  for(const value of Object.values(node.inputs))if(isReference(value))walk(String(value[0]));
 }
 walk('322');
 return Object.fromEntries(Object.entries(prompt).filter(([id])=>wanted.has(id)));
}
export function prepareApiProduction(raw:ApiWorkflow,options:GenerateOptions,imageName:string,targetMode:'pixal3d'|'trellis2'):ApiWorkflow{
 validateApiProduction(raw);
 if(!imageName||/[\\/]/.test(imageName)||imageName.includes('..'))throw new Error('Geçersiz ComfyUI upload adı.');
 if(![1024,2048].includes(options.textureSize)||!Number.isInteger(options.triangles)||options.triangles<1000||options.triangles>80000
 ||![512,768,1024,1536].includes(options.upsample)||![256,384,512,768].includes(options.remesh)
 ||![16,32,64].includes(options.aoSamples)||!Number.isSafeInteger(options.seed)||options.seed<0||options.seed>2147483647)
  throw new Error('Preset limitleri dışındaki üretim ayarları geçersiz.');
 const p=structuredClone(raw);
 writeLiteral(p,'122','LoadImage','image',imageName);
 writeLiteral(p,'186','DecimateMesh','target_face_count',options.triangles);
 writeLiteral(p,'288','PrimitiveInt','value',options.textureSize);
 // 147.texture_size and 196.resolution are linked to PrimitiveInt #288.
 writeLiteral(p,'224','BakeNormalMapFromMesh','resolution',options.textureSize);
 writeLiteral(p,'233','BakeAmbientOcclusion','resolution',options.textureSize);
 writeLiteral(p,'233','BakeAmbientOcclusion','samples',options.aoSamples);
 writeLiteral(p,'261','RenderUVAtlas','resolution',options.textureSize);
 writeLiteral(p,'241','RemeshMesh','resolution',options.remesh);
 writeLiteral(p,'94','Trellis2UpsampleStage','target_resolution',String(options.upsample));
 for(const id of ['3','12','18','23'])writeLiteral(p,id,'KSampler','seed',options.seed);
 writeLiteral(p,'316','PrimitiveBoolean','value',targetMode==='trellis2');
 writeLiteral(p,'322','Save3DAdvanced','filename_prefix','3d/Mallfunction/Studio_'+targetMode);
 const executable=keepSaveDependencies(p);
 validateApiProduction(executable);
 return executable;
}
export function assertInstalledNodes(prompt:ApiWorkflow,info:ObjectInfo):void{
 const missing=[...new Set(Object.values(prompt).map(x=>x.class_type))].filter(type=>!info[type]);
 if(missing.length)throw new Error('ComfyUI düğümleri eksik: '+missing.join(', '));
}
