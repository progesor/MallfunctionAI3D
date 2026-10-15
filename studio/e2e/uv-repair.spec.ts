import { expect, test } from '@playwright/test';

const asset = {id:'ac1922e1-a1a1-4777-a911-fbedfccdfe81', title:'PLAYBOX Fox Plush', engine:'Pixal3D',
  station:'5080',createdAt:'2026-10-15T00:00:00+03:00', fileSize:3339600,
  triangleCount:7994,vertexCount:6252,materialCount:1,textureCount:3,
  sha256:'a'.repeat(64),filePath:'C:\\Studio\\original.glb'};

function simpleGlb():string {
  const json={asset:{version:'2.0'},buffers:[{byteLength:4}],scenes:[{nodes:[]}],scene:0};
  const raw=Buffer.from(JSON.stringify(json));
  const length=(raw.length+3)&~3;
  const bytes=Buffer.alloc(12+8+length+8+4);
  bytes.write('glTF');bytes.writeUInt32LE(2,4);bytes.writeUInt32LE(bytes.length,8);
  bytes.writeUInt32LE(length,12);bytes.write('JSON',16);raw.copy(bytes,20);
  bytes.fill(32,20+raw.length,20+length);
  bytes.writeUInt32LE(4,20+length);bytes.write('BIN\0',24+length);
  return bytes.toString('base64');
}

async function openStudio(
  page:import('@playwright/test').Page,ready:boolean,
  recovery: boolean | 'failed' = false,
) {
  const base64=simpleGlb();
  await page.addInitScript(({asset,base64,ready,recovery})=>{
    const calls:unknown[]=[];
    (window as unknown as {__repairCalls:unknown[]}).__repairCalls=calls;
    (window as unknown as {__TAURI_INTERNALS__:unknown}).__TAURI_INTERNALS__={
      invoke:async(name:string,args?:Record<string,unknown>)=>{
        switch(name) {
          case 'get_environment':return {settings:{comfyUrl:'http://127.0.0.1:8191',station:'5080',blenderPath:'',libraryRoot:''},appDataPath:'C:\\Test',detectedBlender:null,source:'desktop'};
          case 'get_workflows':return [];
          case 'list_assets':return [asset];
          case 'comfy_health':return {};
          case 'gpu_snapshot':return {name:'RTX 5080',index:0,vramTotalMiB:16384,vramUsedMiB:1000,utilPercent:0,temperatureC:32};
          case 'read_asset_glb':return base64;
          case 'blender_lod_report':return null;
          case 'list_unity_exports':return [];
          case 'list_asset_texture_exports':return ready?
            [{outputDir:'C:\\Test\\texture_exports\\pbr_1024',preset:'1024',generatedAt:'2026-10-15',mapCount:5}]:[];
          case 'list_uv_repair_attempts':return recovery ? [{
            versionDir:'C:\\Test\\uv_repairs\\641e3869-4357-40dc-a543-bdce90bd3ece',
            profile:'preserve',resolution:recovery==='failed' ? 2048 : 1024,
            oldIslands:254,newIslands:recovery==='failed' ? 269 : 116,
            inputTriangles:7993,
            qualityStatus:recovery==='failed'?'needs-review':'passed',
            qualityIssues:recovery==='failed'
              ? ['occlusion PBR kalite kaybı','UV gerilmesi yüksek','UV ada sayısı azaltılamadı']
              : [],
          }] : [];
          case 'save_uv_review':
            calls.push({review:args});
            return {asset:{...asset,id:'5d20ee70-e6ae-4d7a-a7ef-999314d5a7e2',
                title:'PLAYBOX Fox Plush · UV REVIEW (QA FAIL)',
                triangleCount:7993},
              versionDir:String(args?.versionDir),
              reportPath:'C:\\Test\\uv_repairs\\v3\\Repair_Report.json',
              workingBlend:'C:\\Test\\uv_repairs\\v3\\Repair_Working.blend',
              uvLayout:'C:\\Test\\uv_repairs\\v3\\UV_Layout.svg',
              uvStressLayout:'C:\\Test\\uv_repairs\\v3\\UV_Stress.svg',
              quality:{passed:false,qualityStatus:'needs-review',
                maps:{occlusion:{role:'occlusion',sampleCount:31972,
                  meanAbsoluteError:0.014,p95AbsoluteError:0.064,
                  p99AbsoluteError:0.300,severeSampleFraction:0.0077,passed:false}}},
              strategy:'curvature-seams-quality-ranked',
              oldIslands:254,newIslands:269,sourceUntouched:true,reviewOnly:true};
          case 'resume_uv_repair':
            calls.push({resume:args});
            return {asset:{...asset,id:'fe996cde-4e2f-462a-8b57-0aa5cb221122',title:'PLAYBOX Fox Plush · UV Clean'},
              versionDir:String(args?.versionDir),reportPath:'C:\\Test\\uv_repairs\\v2\\Repair_Report.json',
              workingBlend:'C:\\Test\\uv_repairs\\v2\\Repair_Working.blend',uvLayout:'C:\\Test\\uv_repairs\\v2\\UV_Layout.svg',
              strategy:'curvature-seams-angle-unwrap',oldIslands:254,newIslands:116,sourceUntouched:true};
          case 'repair_asset_uv':
            calls.push(args);
            return {quality:{passed:true,qualityStatus:'passed',
                maps:{baseColor:{role:'baseColor',sampleCount:30,meanAbsoluteError:0.012,
                  p95AbsoluteError:0.06,passed:true}}},
              meshHealth:{source:{boundaryEdges:9,nonManifoldEdges:13,looseVertices:2,
                zeroAreaFaces:0,connectedComponents:9},
                repairedWorking:{boundaryEdges:9,nonManifoldEdges:13,looseVertices:0,
                zeroAreaFaces:0,connectedComponents:9},
                safeCleanup:{applied:true,removedLooseVertices:2,visibleTrianglesPreserved:true}},
              asset:{...asset,id:'e33aca7a-61fe-49b1-a885-e23f6f97e7ef',title:'PLAYBOX Fox Plush · UV Clean'},
              versionDir:'C:\\Test\\uv_repairs\\v2',reportPath:'C:\\Test\\uv_repairs\\v2\\Repair_Report.json',
              workingBlend:'C:\\Test\\uv_repairs\\v2\\Repair_Working.blend',uvLayout:'C:\\Test\\uv_repairs\\v2\\UV_Layout.svg',
              strategy:'curvature-seams-angle-unwrap',oldIslands:168,newIslands:18,sourceUntouched:true};
          default:throw new Error('Unexpected IPC in UV Repair test: '+name);
        }
      }
    };
  },{asset,base64,ready,recovery});
  await page.goto('/');
  await page.getByRole('button',{name:/Asset Library/}).click();
  await page.getByRole('button',{name:/PLAYBOX Fox Plush/}).first().click();
  await expect(page.getByTestId('uv-repair-panel')).toBeVisible();
}

test('UV repair is disabled when a verified PBR version is missing',async({page})=>{
  await openStudio(page,false);
  await expect(page.getByTestId('uv-repair-button')).toBeDisabled();
  await expect(page.getByText(/Önce yukarıdan 5 PBR haritasını dışa aktar/)).toBeVisible();
  expect(await page.evaluate(()=>(window as unknown as {__repairCalls:unknown[]}).__repairCalls)).toHaveLength(0);
});

test('one-click UV repair preserves existing source and exposes optional overrides',async({page})=>{
  await openStudio(page,true);
  await page.getByRole('combobox',{name:'UV Repair profil'}).selectOption('plush');
  await page.getByRole('button',{name:/UV dikiş ayarı/}).click();
  await page.getByRole('combobox',{name:'UV Repair kenar açısı'}).selectOption('78');
  await expect(page.getByTestId('uv-repair-button')).toBeEnabled();
  await page.getByTestId('uv-repair-button').click();
  await expect(page.getByTestId('uv-repair-success')).toBeVisible();
  await expect(page.getByTestId('uv-repair-quality')).toBeVisible();
  await expect(page.getByText(/168 → 18/)).toBeVisible();
  const calls=await page.evaluate(()=>(window as unknown as {__repairCalls:unknown[]}).__repairCalls);
  expect(calls).toEqual([{assetId:asset.id,textureExportDir:'C:\\Test\\texture_exports\\pbr_1024',
    preset:'plush',resolution:1024,seamAngle:78,weld:true,safeCleanup:true}]);
  await expect(page.locator('.viewport-frame')).toBeVisible();
});

test('can resume completed 5-map bake without launching Blender or a new UV job',async({page})=>{
  await openStudio(page,true,true);
  await expect(page.getByTestId('uv-repair-recovery')).toBeVisible();
  await expect(page.getByTestId('uv-resume-button')).toBeEnabled();
  await page.getByTestId('uv-resume-button').click();
  await expect(page.getByTestId('uv-repair-success')).toBeVisible();
  await expect(page.getByText(/254 → 116/)).toBeVisible();
  const calls=await page.evaluate(()=>(window as unknown as {__repairCalls:unknown[]}).__repairCalls);
  expect(calls).toHaveLength(1);
  expect(calls[0]).toMatchObject({resume:{assetId:asset.id,preset:'preserve',resolution:1024,
    textureExportDir:'C:\\Test\\texture_exports\\pbr_1024',approveOneImportFaceLoss:false}});
});

test('single import-face mismatch requires explicit opt-in before backend override',async({page})=>{
  await openStudio(page,true,true);
  const checkbox=page.getByRole('checkbox',{name:'Blender içe aktarma tek üçgen farkını bilinçli kabul et'});
  await expect(checkbox).toBeVisible();
  await expect(checkbox).not.toBeChecked();
  await checkbox.check();
  await page.getByTestId('uv-resume-button').click();
  await expect(page.getByTestId('uv-repair-success')).toBeVisible();
  const calls=await page.evaluate(()=>(window as unknown as {__repairCalls:unknown[]}).__repairCalls);
  expect(calls).toHaveLength(1);
  expect(calls[0]).toMatchObject({resume:{assetId:asset.id,approveOneImportFaceLoss:true}});
});

test('failed 2048px bake is visible from 1024px default and can be saved as QA FAIL review only',async({page})=>{
  await openStudio(page,true,'failed');
  // The new-bake dropdown defaults to 1024. Saved 2048 attempts must still
  // appear, with exactly the profile/size recorded by Blender.
  await expect(page.getByRole('combobox',{name:'UV Repair çözünürlük'})).toHaveValue('1024');
  await expect(page.getByTestId('uv-repair-recovery')).toBeVisible();
  await expect(page.getByText(/QA BAŞARISIZ/)).toBeVisible();
  await expect(page.getByRole('combobox',{name:'Önceki UV onarım sürümü'})).toContainText('2048px');
  await expect(page.getByTestId('uv-resume-button')).toHaveCount(0);
  const save=page.getByTestId('uv-save-review-button');
  await expect(save).toBeDisabled();
  await page.getByRole('checkbox',{
    name:'QA başarısız sürümü inceleme taslağı olarak kaydetmeyi onayla'
  }).check();
  await page.getByRole('checkbox',{
    name:'Blender içe aktarma tek üçgen farkını bilinçli kabul et'
  }).check();
  await expect(save).toBeEnabled();
  await save.click();
  await expect(page.getByTestId('uv-repair-success')).toContainText('QA FAIL');
  const calls=await page.evaluate(()=>(window as unknown as {__repairCalls:unknown[]}).__repairCalls);
  expect(calls).toHaveLength(1);
  expect(calls[0]).toMatchObject({review:{
    assetId:asset.id,
    resolution:2048,
    preset:'preserve',
    approveOneImportFaceLoss:true,
    acceptFailedQuality:true,
    textureExportDir:'C:\\Test\\texture_exports\\pbr_1024'
  }});
  // Never start Blender a second time in review recovery.
  expect(calls.every((call:unknown)=>{
    const item=call as Record<string,unknown>;
    return !('assetId' in item);
  })).toBeTruthy();
});
