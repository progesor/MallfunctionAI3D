import { expect, test } from '@playwright/test';

const source={id:'aaa1971b-64d8-4d79-a9d9-3845e90a3333',title:'General Fox PBR',
  engine:'Pixal3D',station:'5080',createdAt:'2026-10-15T12:00:00+03:00',
  fileSize:3500000,triangleCount:7980,vertexCount:6639,materialCount:1,
  textureCount:3,sha256:'a'.repeat(64),filePath:'C:\\Test\\original.glb'};
const review={...source,id:'bbb1971b-64d8-4d79-a9d9-3845e90a3333',
  title:'General Fox PBR · Texture Seam REVIEW'};
const approved={...source,id:'ccc1971b-64d8-4d79-a9d9-3845e90a3333',
  title:'General Fox PBR · Texture Repair Approved'};

function tinyGlb() {
  const data={asset:{version:'2.0'},buffers:[{byteLength:4}],scenes:[{nodes:[]}],scene:0};
  const json=Buffer.from(JSON.stringify(data));
  const len=(json.length+3)&~3;
  const bytes=Buffer.alloc(12+8+len+8+4);
  bytes.write('glTF');bytes.writeUInt32LE(2,4);bytes.writeUInt32LE(bytes.length,8);
  bytes.writeUInt32LE(len,12);bytes.write('JSON',16);
  json.copy(bytes,20);bytes.fill(32,20+json.length,20+len);
  bytes.writeUInt32LE(4,20+len);bytes.write('BIN\0',24+len);
  return bytes.toString('base64');
}
async function openStudio(page:import('@playwright/test').Page) {
  await page.addInitScript(({source,review,approved,glb})=>{
    const calls:unknown[]=[];
    let created=false,accepted=false;
    (window as unknown as {__seamCalls:unknown[]}).__seamCalls=calls;
    (window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
      invoke:async(name:string,args?:Record<string,unknown>)=>{
        switch(name){
          case 'get_environment':return {settings:{comfyUrl:'http://127.0.0.1:8188',
            station:'5080',blenderPath:'',libraryRoot:''},appDataPath:'C:\\Test',
            detectedBlender:null,source:'local'};
          case 'get_workflows':return [];
          case 'list_assets':return [source,...(created?[review]:[]),...(accepted?[approved]:[])];
          case 'comfy_health':return {};
          case 'gpu_snapshot':return {name:'RTX 5080',index:0,
            vramTotalMiB:16000,vramUsedMiB:1500,utilPercent:5,temperatureC:35};
          case 'read_asset_glb':return glb;
          case 'blender_lod_report':return null;
          case 'list_unity_exports':return [];
          case 'list_uv_repair_attempts':return [];
          case 'uv_engine_status':return {blenderReady:true,xatlasReady:false,
            backendRevision:'M2.8-XATLAS-TRADEOFF',
            workerSha256:'a'.repeat(64),proxyWorkerReady:true,
            pythonVersion:'3.13',pluginPath:'C:\\Test\\plugins',
            message:'Blender ready'};
          case 'list_asset_texture_exports':return args?.assetId===source.id?
            [{outputDir:'C:\\Test\\texture_exports\\pbr_1024',preset:'1024',
              generatedAt:'2026-10-15',mapCount:5}]:[];
          case 'get_asset_manifest':return {provenance:args?.assetId===review.id?
            {origin:'texture-seam-repair',reviewOnly:true,parentAssetId:source.id}:
            {origin:'comfy',reviewOnly:false}};
          case 'analyze_texture_seams':
            calls.push({inspect:args});
            return {
              sourceUntouched:true,uvUnchanged:true,sourceNormalUnchanged:true,
              eligibleSeamPairs:1187,editablePixels:5023,
              workflow:'source-UV-3D-seam-bands-v1',
              warning:'No normals edited',
              geometry:{triangles:7980,vertices:6639,geometricEdges:12000,
                candidateUvSeams:1187,continuousEdges:9000,
                intentionalSharpEdgesSkipped:1000,nonManifoldEdgesSkipped:15,
                openBoundaryEdges:6,degenerateFacesSkipped:0},
              maps:[
                {role:'baseColor',changedPixels:2010,changedFraction:0.001916},
                {role:'metallic',changedPixels:600,changedFraction:0.000572},
                {role:'roughness',changedPixels:1713,changedFraction:0.001633},
                {role:'occlusion',changedPixels:700,changedFraction:0.000667},
              ]
            };
          case 'repair_texture_seams':
            calls.push({repair:args});created=true;
            return {asset:review,versionDir:'C:\\Test\\texture_repairs\\run01',
              reportPath:'C:\\Test\\texture_repairs\\run01\\Texture_Repair_Report.json',
              inspection:{sourceUntouched:true,uvUnchanged:true,
                sourceNormalUnchanged:true,editablePixels:5000},
              sourceUntouched:true,uvUnchanged:true,
              sourceNormalUnchanged:true,reviewOnly:true};
          case 'approve_texture_seam_review':
            calls.push({approve:args});accepted=true;return approved;
          default:throw new Error('Unexpected Tauri command in texture surface spec: '+name);
        }
      }
    };
  },{source,review,approved,glb:tinyGlb()});
  await page.goto('/');
  await page.getByRole('button',{name:/Asset Library/}).click();
  await page.getByRole('button',{name:/General Fox PBR/}).first().click();
  await expect(page.getByTestId('texture-surface-panel')).toBeVisible();
}
test('analysis reads UV health without writing a new model',async({page})=>{
  await openStudio(page);
  await page.getByTestId('texture-seam-analyze').click();
  // Studio deliberately renders Turkish grouping (1.187, not 1187).
  // Keep the numeric expectation aligned with the explicitly configured locale.
  await expect(page.getByTestId('texture-seam-metrics')).toContainText(
    (1187).toLocaleString('tr-TR')
  );
  await expect(page.getByTestId('texture-seam-metrics')).toContainText('Normal');
  const c=await page.evaluate(()=>(window as unknown as {__seamCalls:unknown[]}).__seamCalls);
  expect(c).toHaveLength(1);
  expect(c[0]).toMatchObject({inspect:{assetId:source.id,mode:'gentle',radius:2}});
});
test('repair creates REVIEW asset and only selected PBR roles',async({page})=>{
  await openStudio(page);
  await page.getByTestId('texture-seam-analyze').click();
  await page.getByTestId('texture-seam-apply').click();
  await expect(page.getByTestId('texture-seam-result')).toContainText('Unity export');
  const c=await page.evaluate(()=>(window as unknown as {__seamCalls:unknown[]}).__seamCalls);
  expect(c[1]).toMatchObject({repair:{assetId:source.id,
    roles:['baseColor','metallic','roughness','occlusion']}});
  await page.getByRole('button',{name:'Düzeltilen modeli 3D görüntüle'}).click();
  await expect(page.getByTestId('texture-seam-review')).toBeVisible();
  await expect(page.getByTestId('texture-approve-button')).toBeDisabled();
  await page.getByRole('checkbox',{name:'Görsel karşılaştırmayı tamamladım'}).check();
  await page.getByTestId('texture-approve-button').click();
  await expect(page.getByRole('button',{name:'Onaylı modeli aç'})).toBeVisible();
  const calls=await page.evaluate(()=>(window as unknown as {__seamCalls:unknown[]}).__seamCalls);
  expect(calls[2]).toMatchObject({approve:{
    reviewAssetId:review.id,confirmedVisualReview:true}});
});
test('source normals never offered as editable seam map',async({page})=>{
  await openStudio(page);
  await expect(page.getByText(/Normal map korunur/)).toBeVisible();
  await page.getByTestId('texture-seam-analyze').click();
  await expect(page.getByTestId('texture-seam-metrics').getByRole('checkbox')).toHaveCount(4);
});
