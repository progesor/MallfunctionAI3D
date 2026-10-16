# M3.0 — UV-Preserving 3D Texture Seam Repair + Mesh Health

2026-10-15 | Branch feat/desktop-studio-foundation | Draft PR #1
**Code integrated pending local Rust/Tauri/Windows acceptance.**
GitHub Actions workflow remains manual-only; it was not executed.

## Why we changed direction

The original Fox and M2.8 logs showed a fundamental tradeoff: fewer
UV islands can increase surface distortion, while xatlas-generated
high-fidelity atlases become much harder to edit by hand. The user
requested a generally useful, model-agnostic workflow rather than
repeating Fox-specific UV tweaks.

M3 starts with the original UV0 and original mesh, and instead analyzes
PBR seams in three-dimensional space. It is NOT another auto-unwrap.

## Actual uploaded Fox asset baseline

The October 15 source GLB has 7,980 triangles, 6,639 vertex records,
1,024-pixel embedded PBR textures and one opaque material. A
standalone analysis matched approximately 1,187 reasonably smooth
geometric edges whose two UV chart representations differ. These
represent repair CANDIDATES, not automatically faulty edges.

Independent byte-level samples from original embedded images found
~7,163 conservative BaseColor samples and ~8,088 Roughness samples
in low-to-moderate-difference ranges. Large differences were skipped
and not assumed to be errors. These are exploratory measurements,
not results from the new compiled Rust engine.

## New pipeline and user workflow

The Inspector now includes **3D Texture & Mesh Health** immediately
after TextureInspector and before experimental Auto UV.

1. Select the original SOURCE model and an existing 5-map PBR Texture
   Export version (all role bytes validated and owned by the asset).
2. Select Gentle or Balanced, a seam band of 1/2/3 pixels, and run
   **3D seam analysis**. No PBR output, GLB, geometry or catalog record
   is written during analysis.
3. Report includes source triangles/vertices, continuous edges,
   smooth UV seam candidates, true open boundary edges, skipped sharp
   edges and skipped non-manifold edges. The four editable PBR maps
   show estimated changed-pixel counts, fractions and large-difference
   skip counts.
4. Select BaseColor, Metallic, Roughness and/or Occlusion. A separate
   user action creates a NEW versioned texture repair archive, with
   every edited PNG, an RGBA change-mask diagnostic for changed maps,
   a JSON report, and a newly constructed GLB.
5. UV0, index/position/normal accessors are re-compared bytewise via
   the geometry-only GLB representation. The normal texture is copied
   byte-for-byte from the selected immutable Texture Export snapshot.
   PBR packing uses the existing glTF ORM and material-factor builder.
6. New asset is visibly named "Texture Seam REVIEW" and carries
   reviewOnly/productionExportBlocked flags. Native Unity URP/HDRP
   and older Unity game-export entrypoints must refuse it.
7. Open the new REVIEW model in the 3D viewport, compare it to the
   original model (the panel links back to the original), explicitly
   confirm you completed a visual comparison, and then create a
   SECOND separate "Texture Repair Approved" asset, which can then
   proceed to regular Texture Export/LOD/Unity preparation.
   No existing version is mutated or silently re-labeled.

## Conservative algorithm / constraints

- Match exact 3D geometric edges from the original glTF indexed
  triangles; a seam candidate must be represented by exactly TWO
  faces with differing UV0 endpoints.
- Skip edges with 3+ faces (non-manifold), open borders, zero-area
  faces, same UV endpoints, UV outside 0..1 and face geometric
  normal dot < 0.90. Hard seams cannot be automatically softened
  without risking intentional product surface boundaries.
- Sample the source image on both sides of each 3D-matched seam.
  Use side-specific inward texel directions inferred from that
  triangle's third UV corner. The source image stays immutable
  while contributions are accumulated deterministically.
- Gentle tolerance: mean BaseColor difference <= 0.11, scalar
  difference <= 0.15. Balanced: BaseColor <= 0.18, scalar <= 0.24.
  Very small differences under 0.015 skip unnecessary changes.
- Blend only 1..3 inward texels per UV boundary. Strong hue jumps
  are skipped rather than blurred. The source image's alpha is
  kept unchanged. Scalar maps remain grayscale.
- Maximum edited fraction: 8% of a map, edited-pixel mean
  absolute delta <= 0.085, maximum delta <= 0.22. If a
  role exceeds these caps the operation fails closed.
- Normal is NOT an editable map: tangent-space normals across
  different UV islands must be compared in the same surface frame
  before any automatic normal repair; M3 does not fake this.
- Source GLB hash and every Texture Export map hash are validated,
  source geometry is compared to output geometry before publish,
  and no source Blender file or Unity prefab is touched.

## What this does NOT solve

The new algorithm is a first conservative seam-healing pass, not
general AI texture restoration. It will NOT automatically remove
wide-band tail texture noise away from seams, repaint invented
details, merge UV islands or fix arbitrary missing holes/overlap
faces. Mesh Health is READ-ONLY in M3: visible face deletion and
hole filling require a reversible, separately approved workflow.
Skinned/multiple material/animated/transparent models remain
unsupported by this pilot and are rejected, not partially exported.

## Source files

- studio/src-tauri/src/texture_seam_geometry.rs — pure glTF surface
  geometry, UV edge pairing and read-only mesh seam diagnostics
- studio/src-tauri/src/texture_seam_pixels.rs — per-PBR-role small
  texel-band repair and transparent change overlays
- studio/src-tauri/src/texture_surface.rs — immutable archive,
  geometry/normal checksum gates, Tauri IPC and manual approval
- studio/src/components/TextureRepairPanel.tsx — analyze, controlled
  repair, 3D preview link, explicit visual approval
- tests/test_texture_seam_contract.py and
  studio/e2e/texture-surface.spec.ts — source/E2E safeguards

## Local acceptance, no GitHub CI

From repository root:

    python -m unittest discover -s tests -p test_texture_seam_contract.py
    cd studio
    npm run check:syntax
    npm run build
    cargo test --manifest-path src-tauri/Cargo.toml --lib texture_seam
    npx playwright test e2e/texture-surface.spec.ts --workers=1

For a genuine test, use original Fox and 1024 PBR Texture Export.
Run **Analyze** (no writes), evaluate role pixels and edge counts,
then apply **Gentle, 2 px, BaseColor/Roughness** only as the first
trial. Compare original / REVIEW in the Studio 3D viewport under
lighting, particularly at tail-body seams. Do NOT approve an asset
if broad tail texture noise remains. The product goal is measured
quality, not merely creating a new texture.

## Follow-up M3.1 / M3.2

- M3.1: interactive 3D brush masking and seam heatmap directly on
  the model, region-selective repairing instead of all seam pairs.
- M3.2: explicit read-only mesh hole/overlap candidates → reversible
  geometry repair proposals with before/after preview, no blind
  deletion or forced normal retopology.
- M3.3: world-space normal conversion + region-local roughness/AO
  artifact repair, with holdout and visual acceptance tests.
- Calibrate on at least Fox plus an unrelated electronic/furniture
  prop to avoid overfitting a single model.

## M3.0 Windows Rust decimal-literal hotfix (15 October 2026)

The user's second-workstation Cargo build stopped in
`texture_seam_geometry.rs` and `texture_seam_pixels.rs`:
Rust numeric literals `.10`, `.15`, etc. lack a mandatory integer part.
Exactly **24** such literals appeared in two synthetic Rust unit tests.
Production seam processing was not reached. All 24 now use `0.10`,
`0.15`, etc. No PBR/GLB algorithm or user's UVRepairPanel.tsx bracket
fix was changed.

A dependency-free Node guard in
`studio/scripts/check-rust-literals.mjs` now scans Rust modules before
the Vite frontend starts. The same check runs with
`npm run check:syntax` and `npm run check:rust-literals`.
It ignores valid tuple-index expressions like `foo?.0` and `pixel.0`.
It is a narrow safeguard, **not** a complete Rust parser; `cargo check`
and `cargo test` on the Windows workstation remain necessary.

Local manual validation (GitHub CI intentionally NOT run):

    cd C:\Works\MallfunctionAI3D
    git pull --ff-only
    cd studio
    npm run check:rust-literals
    npm run check:syntax
    cargo check --manifest-path src-tauri/Cargo.toml
    cargo test --manifest-path src-tauri/Cargo.toml --lib texture_seam
    cargo test --manifest-path src-tauri/Cargo.toml --lib texture_surface
    cd ..
    .\05_Start-Studio.ps1

No Blender, ComfyUI, Unity, original GLB or archived texture maps need
to be deleted or regenerated for this source-only fix.
