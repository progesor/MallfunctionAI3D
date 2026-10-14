# M2 — GLB PBR Texture Inspector (first production increment)

Status: **implementation ready for Windows CI; local RTX 5080 desktop acceptance pending**.

## Scope

- The selected **source or LOD GLB** is read in memory. The inspector uses the actual glTF material -> texture -> image references from the embedded GLB (it does **not** infer maps from file names).
- Base Color and Normal display the original PNG/JPEG/WebP. Roughness is taken from the **G** channel, Metallic from **B**, and AO from **R**. The same packed image can correctly produce three separate channel previews.
- Clicking any map opens a larger view with original dimensions, map role and channel information. On packed maps users can toggle **Packed orijinali göster** to compare channel isolation against the source image.
- Multi-material GLBs receive a material selector. Unsupported external image links are **not fetched**. Known unsupported image references are reported as warnings. No model, UV, shader, or original texture is overwritten.
- Preview blobs are local and revoked when switching assets/materials or unmounting. Channel previews are capped at 2048px for viewer responsiveness; displayed dimensions refer to the original map.

## Not included in this milestone

Texture export, editing/compression, new normal/AO baking, texture synthesis, Unity asset packaging, and full geometry QC are **not implemented yet**. Do not present the inspector as a working Texture Lab optimizer.

## Checks

- `node --experimental-strip-types --test src/tests/glb-inspector.test.mjs`: 5 synthetic parser/channel tests.
- With `MALLFUNCTION_TEST_GLB=<path_to_original_fox_glb>` (Windows `$env:MALLFUNCTION_TEST_GLB = ...`), test also checks the real 3-image / 5-map PLAYBOX Pixal3D GLB.
- `npm run test:adapter` runs the production workflow tests **and** this new map extractor test.
- `npm run test:ui` includes a mocked-Tauri Playwright browser scenario checking map cards and packed/original lightbox controls, besides the existing responsive/focus tests.
- React/Vite build and Rust/Tauri checks continue under `.github/workflows/studio-ci.yml`.

## Windows acceptance after CI

1. Update `feat/desktop-studio-foundation`, launch Studio via `05_Start-Studio.ps1`.
2. Open the existing PLAYBOX Fox Plush from **Model Arşivi** (no GPU generation needed).
3. On the right, scroll to **PBR Texture Inspector**. Expect `3 görsel · 5 kanal`, with Base Color, Normal, Roughness, Metallic and AO cards.
4. Click Roughness -> check grayscale G and **Packed orijinali göster** -> confirm colored packed R/G/B image. Repeat with Metallic and AO. Try Source and LOD1 to confirm maps load from the currently selected GLB.
5. Fullscreen the model, restore with Esc, and ensure Inspector still works. Avoid accepting the feature if any texture fails to display, selection is stale, or the viewport shrinks.

**Known caveat:** Showing the map does not verify UV island overlaps, normal orientation or physically correct materials; these still require visual checks and Unity preview.
