import { expect, test } from '@playwright/test';

const asset={id:'fox-test',title:'PLAYBOX Fox Plush',engine:'Pixal3D',station:'5080',
  createdAt:'2026-10-14T22:00:00+03:00',fileSize:3339600,triangleCount:7998,vertexCount:6252,
  materialCount:1,textureCount:3,sha256:'0'.repeat(64),filePath:'C:\\Test\\fox.glb'};

function glb(): string {
  const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGOsCLjDwMDAxMDAwMDAAAATdgGo32ybXAAAAABJRU5ErkJggg==','base64');
  const bin=Buffer.concat([png,png,png]);
  const pad=(size:number)=>(size+3)&~3;
  const json={asset:{version:'2.0'},buffers:[{byteLength:bin.length}],
    bufferViews:[0,png.length,2*png.length].map(byteOffset=>({buffer:0,byteOffset,byteLength:png.length})),
    images:[0,1,2].map(bufferView=>({bufferView,mimeType:'image/png'})),textures:[{source:0},{source:1},{source:2}],
    materials:[{pbrMetallicRoughness:{baseColorTexture:{index:0},metallicRoughnessTexture:{index:1}},normalTexture:{index:2},occlusionTexture:{index:1}}],
    scenes:[{nodes:[]}],scene:0};
  const txt=Buffer.from(JSON.stringify(json));
  const bytes=Buffer.alloc(12+8+pad(txt.length)+8+pad(bin.length));
  bytes.write('glTF',0);bytes.writeUInt32LE(2,4);bytes.writeUInt32LE(bytes.length,8);
  bytes.writeUInt32LE(pad(txt.length),12);bytes.write('JSON',16);txt.copy(bytes,20);
  bytes.fill(32,20+txt.length,20+pad(txt.length));
  bytes.writeUInt32LE(pad(bin.length),20+pad(txt.length));bytes.write('BIN\0',24+pad(txt.length));
  bin.copy(bytes,28+pad(txt.length));return bytes.toString('base64');
}

const lod={assetId:'fox-test',lodDirectory:'C:\\Test\\lods\\one',generatedAt:'2026-10-15T00:00:00+03:00',
  report:{lods:[{triangles:7998,file_bytes:3191196},{triangles:5000,file_bytes:3122456},{triangles:3000,file_bytes:3065752}]}};

async function start(page:import('@playwright/test').Page, ready:boolean) {
  const base64=glb();
  await page.addInitScript(({model,asset,lod,ready})=>{
    const calls: unknown[]=[];
    (window as unknown as {__unityCalls:unknown[]}).__unityCalls=calls;
    (window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={invoke:async(cmd:string,args?:Record<string,unknown>)=>{
      switch(cmd){
        case 'get_environment':return {settings:{comfyUrl:'http://127.0.0.1:8191',station:'5080',blenderPath:'',libraryRoot:''},appDataPath:'C:\\Test',detectedBlender:null,source:'local'};
        case 'get_workflows':return [];
        case 'list_assets':return [asset];
        case 'comfy_health':return {};
        case 'gpu_snapshot':return {name:'RTX 5080',index:0,vramTotalMiB:16376,vramUsedMiB:2000,utilPercent:1,temperatureC:34};
        case 'read_asset_glb':return model;
        case 'blender_lod_report':return ready ? lod : null;
        case 'list_asset_texture_exports':return ready?[{outputDir:'C:\\Test\\library\\assets\\fox-test\\texture_exports\\pbr-test',preset:'512',generatedAt:'2026-10-14T23:00:00+03:00',mapCount:5}]:[];
        case 'list_unity_exports':return [];
        case 'export_unity_urp':calls.push(args);return {outputDir:'C:\\Test\\gamepack',manifestPath:'C:\\Test\\gamepack\\unity_asset_manifest.json',lodTriangles:[7998,5000,3000],sourceLodBytes:9379404,geometryLodBytes:553296,sharedTextureBytes:500000,sourceUntouched:true};
        default: throw new Error('Mocked IPC unavailable: '+cmd);
      }
    }};
  },{model:base64,asset,lod,ready});
  await page.goto('/');
  await page.getByRole('button',{name:/Asset Library/}).click();
  await page.getByRole('button',{name:/PLAYBOX Fox Plush/}).first().click();
  await expect(page.getByTestId('unity-export-panel')).toBeVisible();
}

test('Unity export is blocked until LODs and PBR maps exist',async({page})=>{
  await page.setViewportSize({width:1480,height:920});
  await start(page,false);
  await expect(page.getByTestId('unity-export-button')).toBeDisabled();
  await expect(page.getByText(/Önce Blender LOD Pipeline/)).toBeVisible();
  expect(await page.evaluate(()=>(window as unknown as {__unityCalls:unknown[]}).__unityCalls)).toEqual([]);
});

test('Unity staging command references selected owned asset and texture version',async({page})=>{
  await page.setViewportSize({width:1480,height:920});
  await start(page,true);
  const btn=page.getByTestId('unity-export-button');
  await expect(btn).toBeEnabled();
  await btn.click();
  await expect(page.getByTestId('unity-export-success')).toBeVisible();
  await expect(page.getByText(/Game staging paketi hazır/)).toBeVisible();
  const calls=await page.evaluate(()=>(window as unknown as {__unityCalls:unknown[]}).__unityCalls);
  expect(calls).toHaveLength(1);
  expect(calls[0]).toEqual({assetId:'fox-test',textureExportDir:'C:\\Test\\library\\assets\\fox-test\\texture_exports\\pbr-test'});
  await expect(page.locator('.viewport-frame')).toBeVisible();
});
