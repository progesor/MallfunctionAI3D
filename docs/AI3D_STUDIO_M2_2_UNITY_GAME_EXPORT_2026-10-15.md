# Mallfunction AI3D Studio · M2.2 — Unity URP Asset Staging

**Status:** Source prepared; requires Windows CI and RTX 5080 user acceptance. This stage DOES NOT create a Unity prefab or modify existing Unity scenes.

## Scope

The existing Pixal3D/TRELLIS.2/ComfyUI backend, seed fidelity and 3D Inspector are unchanged. This release adds a **non-destructive Unity URP staging export** to the model Inspector.

1. Requires an asset in Studio's managed archive, a completed Blender LOD report with three GLB files, and a completed PBR Texture Export (source/1024/512/256) for the **same asset**.
2. Select a source texture export version; click **Unity URP Paketi Oluştur**.
3. Backend verifies owning asset directory, original GLB SHA256, selected PBR export provenance and per-map SHA256, LOD directory ownership, correct 3D geometry and compatible material constraints.
4. Produces new, immutable directory `assets/<id>/unity_exports/urp_<timestamp>_<UUID>/`. Prior asset data is never overwritten. Failed staging directories are removed; source is not edited.
5. Each LOD GLB has the same triangles/vertices/UVs/normals and scene transforms but no duplicated materials/images/texture binary. Three geometry-only GLBs are accompanied by *one common* texture set.
6. Packs Unity URP channels: `MetallicSmoothness.R` = metallic, `.A` = `255 - roughness` (factors applied); `Occlusion.G` = glTF AO. Base Color and OpenGL +Y normal are preserved. Material tint/occlusion strength/normal scale recorded in JSON, *not* baked into files.
7. Output includes file SHA256, material settings, 3 LOD counts, source GLB provenance and manual Unity import steps. Unity's built-in editor does not directly import GLB; use glTFast or equivalent.
8. The Blender LOD generator now uses a safe asset-based filename instead of hardcoded `PLAYBOX_FoxPlush`. Previously generated Fox LOD files remain readable via the filenames recorded in each Blender report.

## Output contract

```
asset/unity_exports/urp_DATE_UUID/
├── Meshes/
│   ├── LOD0.glb   # geometry + normals + UV, no materials/texture images
│   ├── LOD1.glb
│   └── LOD2.glb
├── Textures/
│   ├── BaseColor.png
│   ├── Normal.png
│   ├── MetallicSmoothness.png
│   └── Occlusion.png
├── unity_asset_manifest.json
└── UNITY_URP_SETUP.txt
```

### Supported in this initial stage

- **Single-material static** mesh assets with standard embedded GLB 2.0 and 3 Blender LOD files.
- One PBR exported map of each: `baseColor`, `normal`, `metallic`, `roughness`, `occlusion`. All linear packed masks must share dimensions. 512 or 1024 presets recommended. Texture PNG files must match recorded SHA256.
- No skinning, animation, sparse accessors, Draco/meshopt/extensions, transparent/multi-material or emissive assets in v0.2.2. Unsupported formats stop with an actionable error rather than dropping content silently.

### Unity acceptance still required

1. Import all 3 geometry GLBs via installed glTF importer (e.g., glTFast) into Unity project.
2. Create one `Universal Render Pipeline/Lit` material. Assign BaseMap (sRGB), normal (Normal Map), metallic smoothness packed map (R metallic, A smoothness), occlusion map (G ambient occlusion).
3. Apply material to **all three LOD renderers**. Build LODGroup with height transitions 0.60 / 0.28 / 0.08 as starting suggestions (tune by actual object size).
4. Confirm normal map orientation in Unity, local model scale, Unity object pivot, material shading and visible LOD transitions at actual shelf camera distances. Add one primitive collider; measure GPU/draw calls in Unity Profiler.
5. Only after these checks, mark game-ready. No automatic Unity prefab installation or private Unity repository modifications in M2.2.

## Production/UX principles

- **No new AI run is required** to test this release. It works with the PLAYBOX Fox existing GLB+LOD+Texture Export history.
- No change to workflow JSON, ComfyUI installation, Blender installation or model weights.
- Single-asset operation only, as preferred by user; no automatic batch queue is activated.
- The right Inspector shows prerequisites (LOD + texture export), version selection, output summary, copy-path action, and game export history.

## Verification

- Source unit tests: GLB trim removes image bytes, preserves bufferViews/accessors; URP channel packing with material factors, rejection cases, safe owned paths and full synthetic staging package.
- Browser e2e: package button disabled without prerequisites, selected asset/version passed, success and viewport remain visible.
- Local reference comparison using the real PLAYBOX LOD0/1/2 GLBs: geometry-only size ~553,296 B vs 9,379,404 B for original three LOD GLBs (**94.1% less on mesh files**, shared textures not counted).
- Windows CI and actual Studio output on RTX 5080 are separate mandatory acceptance gates.
