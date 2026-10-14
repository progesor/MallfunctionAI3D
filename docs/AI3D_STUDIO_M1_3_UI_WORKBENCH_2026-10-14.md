# Mallfunction AI3D Studio M1.3 — Workbench UI overhaul

**Date:** 2026-10-14  
**Scope:** User interface and 3D viewport only. Working ComfyUI prompt construction, immutable asset archive, Blender LOD and production model settings are unchanged.

## Root cause

The prior UI styles were appended in several M1/M1.1/M1.2 layers, with late `.main-grid` declarations restoring a three-column template even when `.inspector-expanded` hid both side panels. The center viewer remained trapped in a single grid column, so the button labelled "Görünümü büyüt" made the model appear narrower.

## Changes

- Replace the cascading override stack with a single semantic, responsive CSS workbench.
- `main-grid.inspector-expanded` always has one **full-width** grid column; an additional `focus-mode` on the shell hides the sidebar, title, bottom footer and activity strip. The model canvas grows in both directions.
- ESC exits focus mode; the same button toggles in/out, with clearly visible state labels.
- Add independent left and right panel visibility controls. Render canvas stays mounted; visibility toggles do not modify the model or the production job.
- Replace the oversized title/chrome with a compact, readable action header.
- Left production settings have their own scroll container and a permanently visible **3D Model Üret** button. Source-workflow fidelity is not altered.
- Right inspector has an independent scroll container and fixed GPU telemetry at its bottom.
- Activity monitoring becomes a one-line status strip rather than a large dashboard card.
- Reference image displays as a small image + filename row instead of consuming excessive vertical space.
- Broken GLBs now show an in-view error rather than silently displaying a blank canvas.
- At narrow desktop sizes, navigation condenses and the inspector can be collapsed using the header toggle.

## Layout checks

A Chromium static layout harness was exercised locally at 1920×1080, 1480×920, 1120×760, 1090×710 and 900×720. No global horizontal overflow was measured. At 1480×920 the viewer frame grew from about **720×714** to **1466×868** in focus mode, and ESC restored the original size. The same expand/revert behavior passed at the other four sizes. The 900×720 fixture mimicked the default inspector-hidden condition at narrow sizes.

The fixture checks **CSS composition** and interaction geometry, not the full Tauri or Three.js runtime. Additional Playwright real-React UI tests have been added to Windows GitHub Actions. Passing builds/tests and user testing are still required for end-to-end acceptance.

## Manual acceptance on RTX 5080 station

1. Close the running Studio app, fetch/pull `feat/desktop-studio-foundation`, start using `05_Start-Studio.ps1`.
2. Ensure ComfyUI remains connected to `http://127.0.0.1:8191` and "Workflow aynen" is selected. Do **not** rerun expensive model generation for this UI acceptance pass.
3. Open a previously archived GLB. Verify PBR/Clay/Wire/Normals rendering and orbit/zoom.
4. Click "Görünümü büyüt". Viewer should occupy practically the entire window instead of narrowing. Press ESC or click the button again to revert.
5. Collapse/reopen the left and right panels separately; model and selected asset should remain loaded.
6. Confirm reference thumbnail, fixed Generate button, asset delete confirmation and Blender LOD panel are accessible without obscuring the viewer.
7. Resize the window; check text truncation, panel scroll and overlay behavior. Send a screenshot only if something breaks.

**M1.3 does not alter GPU settings, model preset, ComfyUI workload or generation fidelity.**
