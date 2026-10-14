# MallfunctionAI3D — Desktop Studio M1 foundation

**Date:** 2026-10-14
**Branch:** `feat/desktop-studio-foundation`
**Objective:** ComfyUI Desktop + Blender integration behind a modular Windows desktop UI; existing pilots and scripts remain untouched.

## Implemented source modules

- `studio/src-tauri/src/comfy.rs` — health, `/object_info`, unique image upload, `/prompt` queue, `/queue`/`/history` polling, `/view` GLB capture.
- `studio/src/lib/workflow.ts` — verified native Pixal3D/TRELLIS.2 UI graph selection and conversion to API prompt using *live ComfyUI metadata*, plus safe preset edits and immutable pinned source graph.
- `studio/src-tauri/src/storage.rs` — SQLite archive, immutable `original.glb`, content SHA256, triangle counts, generation manifest.
- `studio/src-tauri/src/blender.rs` — Windows installed `blender.exe` lookup, background LOD with packaged Python script, output validation, log, new variants.
- `studio/src/components/ModelViewport.tsx` — embedded GLB viewing with orbit camera and PBR/Clay/Wireframe/Normals.
- `studio/src/lib/modules.ts` — modular registry, future image generation / texture lab / Unity export marked **planned**.
- `05_Start-Studio.ps1` — initial prerequisites check and launch. Node.js/Rust installed separately, no modification of ComfyUI/Blender/model weights.

## Development rules

- Single-asset generation default. Batch not enabled.
- Separate workstation settings for RTX 4070 Ti SUPER and RTX 5080; no assumed shared model cache.
- Reference/GLB/working asset data are kept in application data folder, not in GitHub.
- Compiled desktop and ComfyUI end-to-end success **NOT yet established**: Rust toolchain/npm package registry inaccessible in code-authoring container. CI/workstation tests must verify this before stable release.
- First interactive smoke: launch Tauri UI; import the previously accepted Fox Plush GLB to test archive + 3D viewport without generating; then submit Pixal3D single run and verify GLB captured via ComfyUI history; Blender LOD; restart library persistence.

## Follow-up priorities after first Windows acceptance

1. Convert ComfyUI raw JSON history's GLB result handling into a versioned capture adapter for each saver family if necessary.
2. Native file streaming for 100 MB+ GLBs, texture map thumbnails/UV atlas inspector.
3. Robust Blender versioned workers with timeout/cancellation and normal/AO rebake.
4. PNG/PBR map export, texture channel packing/KTX2 options and real Unity prefab exporter.
5. Image generation as **optional separate module**, not forced into 3D workflow.

For instructions see `studio/README_TR.md`.
