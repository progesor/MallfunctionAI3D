# MallfunctionAI3D — Unity-native FBX, URP/HDRP and prefab integration
**Branch:** feat/desktop-studio-foundation (Draft PR #1)
**Reference:** AI3D_HANDOVER_2026-10-15.md and AI3D_UNITY_URP_HDRP_EXPORT_SPEC_2026-10-15.md
**Acceptance status:** Implementation and CI testing. Real Blender FBX + URP/HDRP Unity Editor validation must run on a Windows workstation with matching installed Unity projects. Do NOT call staged files Unity-ready.

## Implemented workflow

Studio asset inspector -> select explicit Unity project folder -> detect ProjectVersion and installed RP packages -> Auto/URP/HDRP -> one native export.
- Rust \`export_unity_native\` verifies Studio-owned asset, original GLB SHA256, three LOD paths and geometry, PBR map manifest and SHA256. It reads immutable sources and makes a new version every time.
- \`blender/export_unity_fbx.py\` builds one FBX with LOD0/LOD1/LOD2 child nodes, Y-up, normals and UV. It does **not** decimate again or guess scale. The worker reports triangles and UV for Rust validation.
- URP mask PNG: R Metallic, G AO, B unused (0), A Smoothness. HDRP mask PNG: R Metallic, G AO, B DetailMask (0 if no detail), A Smoothness. Source factors are applied and mask imported linear (sRGB off). glTF normal orientation is preserved.
- Unity Editor importer is installed once in the **selected** project's Assets/MallfunctionAI3D/Editor. Last-written AI3D_READY.txt marker and domain-reload scan trigger Editor import. It validates active GraphicsSettings.currentRenderPipeline, SHA256, FBX and texture importers.
- Editor creates one pipeline-specific Material shared by three renderers, LODGroup with heights 0.60/0.28/0.08, BoxCollider and a saved .prefab. It writes \`unity_import_report.json\`. Studio polls it; files copied is only STAGED_PENDING_UNITY_EDITOR, never "prefab ready."
- Historical M2.2 GLB staging continues unchanged.

## Outputs

Studio archive: \`assets/<id>/unity_exports/native_<pipeline>_<timestamp>_<uuid>/\`
Contains Meshes/Asset_LODs.fbx, Textures/{BaseColor,Normal,URP_Mask or HDRP_Mask}.png, manifest, source-target pointer, Blender log and validation report.

Selected Unity project: \`Assets/MallfunctionAI3D/Imported/<assetId>/<uniqueVersion>/\`.
Copies FBX/maps/manifest, triggers importer with AI3D_READY.txt, and receives Materials/<id>_<pipeline>.mat, Prefabs/<id>.prefab and unity_import_report.json from the Editor.

## Safety/limitations

Only an explicitly selected Unity project may be modified. Only the namespaced Assets/MallfunctionAI3D directory is written, not project settings, scenes or gameplay scripts. Every exported version has a unique identifier and prior assets are not overwritten. Existing Editor importer copies are never silently replaced. Current support: one opaque static PBR material (no emission, rig, multi-material or custom ShaderGraph). Imported pipeline is verified by Unity itself; the presence of packages is not sufficient proof of active pipeline. No new ComfyUI model, batch processing or texture-inspector redesign is involved.

Optional .unitypackage is deferred until actual Unity Editor acceptances are passed; portable FBX maps are not misrepresented as a native package.

## Acceptance checklist

1. G0 Windows CI: workflow/node tests, React build, Rust/Tauri check/unit tests, Playwright tests and Python FBX worker syntax pass. CI success does not prove Unity acceptance.
2. G1 Fox FBX worker in real Blender: compare LOD counts **7998/5000/3000**, UV, normals, dimensions and source SHA256.
3. G2 Real Unity URP test project: ensure Studio reports **UnityPrefabReady**; correct URP/Lit shader, shared material on all three LODs, linear RGBA mask used in both Metallic and Occlusion slots, normal map, BoxCollider and prefab.
4. G3 Real Unity HDRP test project: separate project, HDRP/Lit shader, _MaskMap, shared material, LODGroup and prefab; inspect normal +Y correctness.
5. G4 Save/reopen Unity project, drag prefab to scene, verify LOD transitions, visual comparison and persistent material references. Performance 100/1000 copies later, no huge scene batch now.

On Windows Unity test machine, with the selected project already containing native export files, run:
\`\`\`powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\Validate-UnityNativeExport.ps1 -ProjectPath "C:\Path\Unity_URP_Test" -Pipeline URP
powershell -NoProfile -ExecutionPolicy Bypass -File .\tools\Validate-UnityNativeExport.ps1 -ProjectPath "C:\Path\Unity_HDRP_Test" -Pipeline HDRP
\`\`\`
The helper invokes a real Unity Editor batch import and validates the saved importer report. It cannot verify final shading appearance; inspect visually in the actual Editor. Do not start batch import while the same Unity project is open.

## Verification status at authorship

Windows CI is triggered by branch pushes; Unity Editor and Blender are unavailable in this execution environment. No claim of a completed real Editor or material/shader acceptance test is made until logs and project paths are available.

## 2026-10-15 · Unity 6 ModelImporter uyumluluk düzeltmesi

- Unity 6'da `ModelImporter.importMaterials = false` artık derlenmez (`CS0619` / `CS0200`).
- Doğru karşılığı: `mi.materialImportMode = ModelImporterMaterialImportMode.None;`.
- Önceden HDRP/URP projesine aktarılmış varlıkların FBX/PNG dosyaları korunur. Unity projesindeki `Assets/MallfunctionAI3D/Editor/MallfunctionAI3DImporter.cs` dosyasındaki tek satır değiştirilerek Unity tekrar derlenir; `Tools > Mallfunction AI3D > Import pending assets` daha sonra bekleyen importları yeniden işler.
- Sonraki Studio native export'unda yalnızca **önceki, tamamı byte-byte tanınan eski importer sürümü** otomatik olarak yedeklenip düzeltilir; farklı/özelleştirilmiş dosyalara dokunulmaz. Mevcut `.cs.meta` korunur.
- Yedek dosyası `MallfunctionAI3DImporter.legacy_<hash>.cs.txt` olduğundan Unity ikinci bir C# scripti derlemez.
- GitHub Actions, kullanıcı isteği üzerine **otomatik push CI olmaksızın**, yalnız `workflow_dispatch` ile çalışır. Bu düzeltmede Actions testleri tetiklenmez.
- Test: `cargo test --manifest-path studio/src-tauri/Cargo.toml --lib unity6_importer_upgrade_is_exact_and_non_destructive` (Windows yerel, CI gerektirmez). Gerçek Unity HDRP Editor kabulü kullanıcı istasyonunda ayrıca yapılmalıdır.
