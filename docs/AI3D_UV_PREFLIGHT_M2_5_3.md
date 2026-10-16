# M2.5.3 — Fail-fast UV planning before Blender PBR bake

Branch: feat/desktop-studio-foundation (Draft PR #1).
GitHub CI: workflow_dispatch only; no CI run was requested.
Status: Python policy independently tested; real Blender 5.1/Windows
end-to-end plan acceptance still requires user workstation validation.

## Actual reported regression

On new PC the user produced another PLAYBOX Fox model with 262
source UV islands. Auto UV Repair generated 1733 islands, changed
the textured appearance, and failed all four sampled PBR quality roles.
The user correctly saved the result as UV REVIEW (QA FAIL).

Code-level cause: new_uv tried five curvature seam plans but would
silently run bpy.ops.uv.smart_project on ANY failure to find an
acceptable curvature plan. Smart UV had NO island count rejection
before running Cycles PBR bakes; the count check occurred only later.
In the current five-angle code regular candidates were limited to
about 1.35 x 262 islands (~354). Thus 1733 islands indicates the
Smart UV fallback rather than a valid curvature-based candidate.

## New non-destructive behavior

- Added blender/uv_planner.py (pure, Blender-independent). The same
  source-relative distortion gate used by M2.5.1 is now mandatory
  during UV planning, BEFORE baking. Every candidate must meet:
  UV bounds sanity, fewer islands than the source, source-relative
  P95 and maximum anisotropy, and zero degenerate UV triangles.
- Candidate search still tries up to five seam-angle plans. Only
  qualified candidates may be selected for a five-map Cycles bake.
  A least-bad plan is NOT silently promoted to a repaired asset.
- Removed automatic Smart UV fallback from the production path.
  In absence of a qualifying plan, worker writes UV_Plan.json
  including every attempted plan with reasons, original and working
  topology diagnostics and whether optional vertex weld was applied.
- Saves UV_Layout.svg, UV_Stress.svg and UV_Plan_Working.blend
  as diagnostic-only versions. Deliberately skips the five PBR bakes.
- Blender returns a distinct UV_PREFLIGHT_REJECTED stage and Rust
  displays a direct, informative message with the UV_Plan.json path.
  No new UV Clean / UV REVIEW catalog asset is automatically created.
- Exact source GLB SHA, source Texture Exports, Unity native URP/HDRP
  and previous archived versions remain unchanged.
- Previous *completed* 2048px QA FAIL bakes are still eligible for
  explicit review-only recovery; preflight-rejected jobs have no
  texture maps and are NOT incorrectly offered as recoverable bakes.

## Value and limitations

This prevents wasting Blender/Cycles time and an increasingly messy
model archive when the available curvature seam strategy cannot meet
the requirement. It does NOT itself guarantee a new, better UV atlas.
For this new Fox, the best next technical action is to inspect the
saved UV_Plan.json and original.glb geometry. That tells us whether
vertex welding failed and what topology/seam paths are available.

Future real improvement is model-/region-aware seam selection or
a versioned manual Blender seam edit/rebake loop. Reducing island count
alone is not a useful metric for textured fidelity.

## Offline verification (no GitHub Actions)

From repository root:

    python -m unittest discover -s tests -p test_uv_planner.py
    python -m unittest discover -s tests -p test_uv_layer_lifecycle.py
    python -m unittest discover -s tests -p test_uv_repair_contract.py
    python -m py_compile blender/uv_planner.py blender/repair_uv_pbr.py
    cargo check --manifest-path studio/src-tauri/Cargo.toml
    cd studio; npm run build

Planner tests cover the actual reported 262 -> 1733 regression,
the original Fox's 254-island candidate set and a synthetic
good atlas that should pass both topology and source-relative
UV distortion gates. No Rust/Cargo or Blender runtime is
installed in the assistant's environment; C#/Unity execution
and full Windows/Tauri acceptance remain unverified.

## If the new preflight rejects

Do not repeat the expensive bake. Open the newly created UV_Plan.json
and inspect fields:
  uv.triedPlans[].reasons
  weld.requested / weld.applied / weld.weldedVertices / weld.reason
  sourceMeshHealth.connectedComponents vs workingMeshHealth
  sourceMeshHealth.boundaryEdges vs workingMeshHealth.boundaryEdges
  uv.sourceStretch and candidate stretch metrics

For Blender visual review open UV_Plan_Working.blend and UV_Stress.svg.
If topology cannot be rejoined without changing source UVs, the
repair algorithm needs new seam/mesh logic rather than looser QC.
