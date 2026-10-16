import { expect, test } from '@playwright/test';

// Browser-mode tests use a local Tauri IPC mock. No ComfyUI / Blender work is performed.
function pbrFixture(): string {
  const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAFklEQVR4nGOsCLjDwMDAxMDAwMDAAAATdgGo32ybXAAAAABJRU5ErkJggg==','base64');
  const bin=Buffer.concat([png,png,png]);
  const pad=(size: number)=> (size+3)&~3;
  const imageOffsets=[0,png.length,png.length*2];
  const gltf={asset:{version:'2.0'},buffers:[{byteLength:bin.length}],
    bufferViews:imageOffsets.map(offset=>({buffer:0,byteOffset:offset,byteLength:png.length})),
    images:[{bufferView:0,mimeType:'image/png'},{bufferView:1,mimeType:'image/png'},{bufferView:2,mimeType:'image/png'}],
    textures:[{source:0},{source:1},{source:2}],
    materials:[{name:'Fox Fabric',pbrMetallicRoughness:{baseColorTexture:{index:0},metallicRoughnessTexture:{index:1}},normalTexture:{index:2},occlusionTexture:{index:1}}],
    scenes:[{nodes:[]}],scene:0,
  };
  const payload=Buffer.from(JSON.stringify(gltf));
  const jlen=pad(payload.length), blen=pad(bin.length);
  const bytes=Buffer.alloc(12+8+jlen+8+blen);
  bytes.write('glTF',0);bytes.writeUInt32LE(2,4);bytes.writeUInt32LE(bytes.length,8);
  bytes.writeUInt32LE(jlen,12);bytes.write('JSON',16);payload.copy(bytes,20);
  bytes.fill(0x20,20+payload.length,20+jlen);
  bytes.writeUInt32LE(blen,20+jlen);bytes.write('BIN\0',24+jlen);bin.copy(bytes,28+jlen);
  return bytes.toString('base64');
}

const asset={id:'fox-test',title:'PLAYBOX Fox Plush',engine:'Pixal3D',station:'5080',
  createdAt:'2026-10-14T22:00:00+03:00',fileSize:1000,triangleCount:7998,vertexCount:6252,
  materialCount:1,textureCount:3,sha256:'1234567890abcdef1234567890abcdef',filePath:'C:\\Test\\fox.glb'};

test('embedded PBR map cards show original/packed channels and enlarged preview', async ({page})=>{
  await page.setViewportSize({width:1480,height:920});
  const glb=pbrFixture();
  await page.addInitScript(({base64,savedAsset})=>{
    (window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
      invoke:async(command:string)=>{
        switch(command){
          case 'get_environment':return {settings:{comfyUrl:'http://127.0.0.1:8191',station:'5080',blenderPath:'',libraryRoot:''},appDataPath:'C:\\Test',detectedBlender:null,source:'desktop'};
          case 'get_workflows':return [];
          case 'list_assets':return [savedAsset];
          case 'comfy_health':return {};
          case 'gpu_snapshot':return {name:'NVIDIA GeForce RTX 5080',index:0,vramTotalMiB:16376,vramUsedMiB:1200,utilPercent:0,temperatureC:32};
          case 'read_asset_glb':return base64;
          case 'blender_lod_report':return null;
          default:throw new Error('IPC unavailable in e2e mock: '+command);
        }
      },
    };
  },{base64:glb,savedAsset:asset});
  await page.goto('/');
  await page.getByRole('button',{name:/Asset Library/}).click();
  await page.getByRole('button',{name:/PLAYBOX Fox Plush/}).first().click();
  await expect(page.getByTestId('pbr-texture-inspector')).toBeVisible();
  await expect(page.getByRole('button',{name:/Roughness haritasını büyüt/})).toBeEnabled();
  await expect(page.getByRole('button',{name:/Metallic haritasını büyüt/})).toBeEnabled();
  await expect(page.getByRole('button',{name:/AO haritasını büyüt/})).toBeEnabled();
  await expect(page.locator('.texture-map-card')).toHaveCount(5);
  await page.getByRole('button',{name:/Roughness haritasını büyüt/}).click();
  await expect(page.getByRole('dialog',{name:/Roughness haritası/})).toBeVisible();
  await expect(page.getByText('G · Pürüzlülük')).toBeVisible();
  await page.getByLabel('Packed orijinali göster').check();
  await expect(page.getByLabel('Packed orijinali göster')).toBeChecked();
  await page.getByRole('button',{name:'Texture önizlemesini kapat'}).click();
  await expect(page.getByRole('dialog',{name:/Roughness haritası/})).not.toBeVisible();
});

test('texture export creates a new immutable map set and keeps viewport usable', async ({page})=>{
  await page.setViewportSize({width:1480,height:920});
  const glb=pbrFixture();
  await page.addInitScript(({base64,savedAsset})=>{
    const calls: unknown[] = [];
    (window as unknown as {__textureExportCalls: unknown[]}).__textureExportCalls = calls;
    (window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
      invoke:async(command:string, args?:Record<string,unknown>)=>{
        switch(command){
          case 'get_environment':return {settings:{comfyUrl:'http://127.0.0.1:8191',station:'5080',blenderPath:'',libraryRoot:''},appDataPath:'C:\\Test',detectedBlender:null,source:'desktop'};
          case 'get_workflows':return [];
          case 'list_assets':return [savedAsset];
          case 'comfy_health':return {};
          case 'gpu_snapshot':return {name:'NVIDIA GeForce RTX 5080',index:0,vramTotalMiB:16376,vramUsedMiB:1200,utilPercent:0,temperatureC:32};
          case 'read_asset_glb':return base64;
          case 'blender_lod_report':return null;
          case 'export_asset_textures':{
            calls.push({assetId:args?.assetId,preset:args?.preset,files:(args?.files as Array<{filename:string,width:number,height:number,contentBase64:string}>)?.map(x=>({filename:x.filename,width:x.width,height:x.height,bytes:x.contentBase64.length}))});
            return {outputDir:'C:\\Test\\library\\assets\\fox-test\\texture_exports\\pbr-test', manifestPath:'C:\\Test\\manifest.json', sourceUntouched:true,
              files:((args?.files??[]) as Array<{filename:string,width:number,height:number}>).map(x=>({filename:x.filename,role:'PBR',width:x.width,height:x.height,fileBytes:300,sha256:'test'}))};
          }
          default:throw new Error('IPC unavailable in e2e mock: '+command);
        }
      },
    };
  },{base64:glb,savedAsset:asset});
  await page.goto('/');
  await page.getByRole('button',{name:/Asset Library/}).click();
  await page.getByRole('button',{name:/PLAYBOX Fox Plush/}).first().click();
  await expect(page.getByTestId('texture-export-panel')).toBeVisible();
  await page.getByRole('combobox',{name:'Texture export çözünürlüğü'}).selectOption('512');
  await page.getByRole('button',{name:/5 PBR haritasını dışa aktar/}).click();
  // Canvas PNG encoding can be slow on GitHub-hosted software-rendered Chromium.
  // Wait for actual end-to-end export completion, not an arbitrary 5s UI tick.
  await expect(page.getByText('5 PNG kaydedildi')).toBeVisible({timeout:25_000});
  const calls=await page.evaluate(()=>(window as unknown as {__textureExportCalls:unknown[]}).__textureExportCalls);
  expect(calls).toHaveLength(1);
  const record=calls[0] as {assetId:string,preset:string,files:Array<{filename:string,width:number,height:number,bytes:number}>};
  expect(record.assetId).toBe('fox-test');
  expect(record.preset).toBe('512');
  expect(record.files).toHaveLength(5);
  expect(record.files.every(x=>x.width<=512 && x.height<=512 && x.bytes>0)).toBe(true);
  expect(record.files.map(x=>x.filename)).toEqual(['m0_baseColor.png','m0_normal.png','m0_roughness.png','m0_metallic.png','m0_occlusion.png']);
  await expect(page.locator('.viewport-frame')).toBeVisible();
});
