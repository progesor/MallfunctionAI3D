# M2.5 · Quality-First UV / PBR Repair pilot
Status: implementation prepared · awaiting local Blender 5.1.2 and Unity HDRP visual acceptance
Branch: feat/desktop-studio-foundation · Draft PR #1
CI policy: workflow_dispatch only. Do NOT spend Windows Actions minutes on this patch.

## Why this sprint exists

The Fox pilot proved that reducing UV islands from 254 to 116, while keeping
Blender triangle count at 7993, does NOT mean the material improved.
The tail acquired artificial bands/stripes and BaseColor, Roughness and Metallic
maps lost detail relative to the original 7994-triangle ComfyUI GLB.

M2.4 permitted library publication after geometrical checks plus optional
single import-time-face acknowledgement. **The M2.5 contract is stricter:**
geometry acceptance is still mandatory, and texture fidelity is now a
separate, measurable gate. User triangle approval must NEVER override a
failed PBR fidelity gate.

## Changes implemented

### 1. Measured *surface-space* texture transfer

New Blender-free pure module: blender/uv_texture_fidelity.py.
It compares the actual original texture and the SAVED rebaked PNG at 4
barycentric points per Blender mesh triangle, sampling from **original UV0**
and **new UV** respectively. The ORIGINAL imported source face/UV state is
copied and assigned stable integer face IDs *before* any weld or BMesh change.
Each target triangle corner is matched to that reference by face ID and 3D
position. If welding shifts original face UVs, the worker rejects the
candidate BEFORE spending time on the five Cycles bakes. This samples the SAME points on the same mesh,
not whole PNGs; therefore the black atlas background does not falsify
results. Texture pixels are read through Blender Image.pixels once per map,
then 2D bilinear sampling is performed. Source UV uses glTF repeat; packed
destination atlas clamps. The four scored roles are independent:

| Role | Mean absolute error maximum | P95 absolute error maximum |
|---|---:|---:|
| BaseColor RGB | 0.065 | 0.180 |
| Metallic scalar | 0.075 | 0.210 |
| Roughness scalar | 0.075 | 0.210 |
| AO/Occlusion scalar | 0.080 | 0.220 |

These are **initial pilot thresholds, not yet calibrated against the real Fox**.
They operate on Blender image pixel values, on a [0,1] normalised scale.
Texture_Quality.json and Repair_Report.json each preserve metrics and status.
The scoring must sample the saved PNG, not a transient Cycles image before
PNG writing and color-space conversion.

Normal-map RGB values cannot be compared directly after UV changes because
the tangent frame changes. Blender still rebakes normals in tangent space,
but **normal seam/world-space equivalence requires actual visual acceptance**.

The new Rust function verify_texture_quality independently reads the JSON,
verifies presence of all four roles and sample counts, enforces the same
numeric thresholds and prevents publishing any candidate with missing or
failed fidelity. A failed bake leaves the original, Blender .blend, SVG,
five PNGs and texture quality report in the versioned UV repair folder.
**No new library asset is created on quality failure.**

### 2. Candidate UV layout selection optimises stretch, not just island count

For the user's selected seam angle, Blender now tries three angle thresholds:
the selected threshold, minus 12°, minus 24° (minimum 35°).
Every candidate undergoes UV atlas bounds, island count and p95 geometric
anisotropy checks. The lowest distortion candidate is re-applied so UV
coordinates and seam flags match. The choice and all trial metrics are
recorded as 'triedPlans' in Repair_Report.json.

Fully closed manifold mesh components get a hidden rear-meridian cut even
when there are already sharp-angle seam edges. This is intended to improve
the ability to unfold curved tails, cones and heads; the real tail still
requires on-device examination.

Smart UV remains a reported fallback. UV stretch metrics are **diagnostics**:
their acceptance thresholds are not yet calibrated; image fidelity is the
primary strict quality gate.

### 3. Conservative topology inspection and safe orphan cleanup

Mesh health is measured before and after UV work:
boundary edges, non-manifold edges, connected components, loose vertices,
zero-area faces, exactly-overlapping oriented face candidates and tiny closed
boundary-loop candidates. These are diagnostic facts, not proof of material quality.

The only new *automatic mesh edit* deletes vertices/wires not referenced
by any polygon on a disposable working mesh. It verifies that the visible
triangle count remains unchanged and retains the source GLB untouched.
This is an advanced checkbox that defaults to enabled.

Not yet automatic: filling holes, removing visible overlapping polygons,
repairing self-intersections, retopology, recalculating artistic normals.
These can damage the stylized tail/body intersection and must be governed
by an explicit size/material/mesh-error test and visual preview later.

Existing user-controlled weld is preserved as an independent advanced
option. It can rejoin many vertices, but the resulting surface must pass
texture QA; 2596 joined points is not by itself a success indicator.

### 4. Recovery / legacy behavior

Previously completed Fox UV Clean versions from M2.4 have no
Texture_Quality.json. They are NOT eligible for new automatic
"quality approved" publication. This intentionally prevents another
broken tail from being labelled as successfully repaired. Existing
already-registered library assets remain untouched and can be compared
side by side. New runs use fresh immutable version directories.

Current one-face import-time difference (7994 declared, 7993 imported) is
unchanged: an auditable duplicate/degenerate face is accepted; otherwise
explicit user permission can be supplied only for completed recovery
versions. It is not a general tolerance and does not bypass texture QA.

## Recommended local Fox test

1. Pull the branch; restart the desktop Tauri Studio.
2. Select the original PLAYBOX Fox Plush, SOURCE view.
3. Choose the existing validated 1024px PBR Texture Export.
4. Auto UV + Texture Repair: Preserve, 1024px, 62° initial plan.
   Enable 'Güvenli mesh temizliği', leave existing weld selectable.
5. Run ONCE. Blender tests more than one UV plan but performs only a
   single chosen set of five PBR bakes. No ComfyUI regeneration.
6. If accepted, review the source and new Fox side-by-side, particularly
   tail stripes, fur color boundaries, plastic eyes and tangent normals.
   The UI displays BaseColor/Metallic/Roughness/AO surface QA metrics
   and mesh-health data for successful versions.
7. If rejected, do NOT loosen thresholds blindly. Open the
   freshly created uv_repairs/<version>/Texture_Quality.json and send
   the four metrics + Repair_Report.json, or attach the two files.
   We can then improve atlas selection, bake supersampling or seam layout
   based on evidence without touching game-ready Unity exports.

## Offline tests (do not run GitHub Actions)

From repo root:

    python -m unittest discover -s tests -p "test_uv_fidelity.py"
    python -m unittest discover -s tests -p "test_uv_layer_lifecycle.py"
    python -m unittest discover -s tests -p "test_uv_repair_contract.py"
    python -m py_compile blender/repair_uv_pbr.py blender/uv_texture_fidelity.py
    cargo test --manifest-path studio/src-tauri/Cargo.toml --lib uv_repair
    cd studio
    npm run build
    npx playwright test e2e/uv-repair.spec.ts --workers=1

These tests exercise pure math, mock Blender UV lifetimes, Rust quality
contracts and frontend IPC; **they do not substitute for an actual Blender
Cycles bake or Unity HDRP/URP acceptance**. The assistant has no local
Blender/Rust toolchain for real Windows validation.

## M2.6 (only after observing pilot metrics)

- Calibrate texture fidelity thresholds and PBR colorspace behavior based
  on Fox's actual saved PNGs, not unsupported guesses.
- Optional supersampled reprojection / seam padding evaluation.
- Manual edits to saved .blend seam/UV layers -> new versioned rebake.
- Small-hole and overlap repair as individually previewable, controlled
  topology transforms with measured before/after mesh and UV effects.
- A concrete 3D/UV side-by-side review and explicit human acceptance
  before downstream Unity LOD/material/prefab export.

## Strict source UV provenance (part of the first Fox quality gate)

The earlier M2.4 worker merged 2596 vertices on the disposable working mesh,
then compared the new UV to the already-welded OLD UV. That could miss a
source-data mismatch introduced by BMesh weld operations.

M2.5 stamps AI3D_REPAIR_SOURCE_FACE_ID on triangles before ANY working
change and copies the imported mesh/old UV in memory. After weld and new UV
selection, the worker matches every original and repaired triangle by ID and
corner position (within a small object-scale tolerance). It compares the
original per-corner UV to the working old UV. If drift is observed, the
worker fails before running five costly bakes; try advanced Weld=OFF.
This ensures fidelity cannot be measured against an already-corrupted
intermediate "old UV" layer. The baseline GLB on disk is never edited.

M2.5 exports Texture_Quality.json with sourceUvProvenance:
matchedTriangleCount, originalCornerUvDriftCount, maxOriginalCornerUvDrift,
originalUvProvenanceIntact. Rust independently enforces these fields.

Extra tests: tests/test_uv_fidelity.py checks saved-PNG role metrics,
source-face UV drift and triangle movement; 
tests/test_uv_layer_lifecycle.py checks Blender RNA handle invalidation.
