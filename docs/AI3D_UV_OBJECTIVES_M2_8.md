# M2.8 · UV Atlas Objectives + xatlas Chart Presets

Branch: feat/desktop-studio-foundation, Draft PR #1
Status: code prepared; REAL Blender/Unity test still pending.
CI: GitHub Actions manual-only; no CI run.

## Evidence: user UV_Plan(3).json and Repair_Run(4).log

The new connected UV proxy WORKS: 7,980 triangles, source 6,639 vertex
records, 2,786 joined only on a planning proxy, components 262 to 4.
The original PBR render geometry/UVs/normals and archive remain intact.

xatlas 0.0.11 already runs successfully. Source has 262 UV islands,
P95 anisotropy 1.24099 and worst 30.14263. Default xatlas has 459
islands, P95 1.09790 and worst 3.16874, with NO degenerate UV faces.
Thus its p95 improved 11.53%, maximum improved 89.49%, but the atlas
has 75.19% MORE islands. The older compact-only checker rejected it
before running ANY PBR bake.

Five-angle Blender candidates produced: 62 deg /266 islands/P95 4.87956;
50 deg /362/P95 3.10859 (24 degenerate UV edges); 38 deg/490/P95 invalid;
26 deg/740/invalid; 20 deg/1057/invalid. None qualifies.

## General-purpose atlas objective

Choose UV ATLAS HEDEFI in Studio:

- AUTO (default): prefer compact/low-stretch; if not available,
  allow a bounded, strong improvement of geometric UV distortion even
  when the atlas has more pieces. Mark result explicitly.
- COMPACT: strictly fewer UV islands than input AND safe UV stretch.
  The reported 459-island xatlas plan will be rejected.
- FIDELITY: favor lower UV distortion within a disclosed and bounded
  increase in island count, still subject to geometry and texture QA.

The fidelity-tier rule is STRICT and independently checked in Python
AND Rust: new/source island ratio at most 1.8; new/source P95 ratio
at most 0.90; new/source WORST stretch ratio at most 0.50; all original
triangle corners and normals preserved; no degenerate UV triangles.

459/262 = 1.752, 1.09790/1.24099 = 0.885,
3.16874/30.14263 = 0.105. The real uploaded xatlas candidate is
PRE-BAKE eligible for Auto or Fidelity, but not Compact. This does
NOT mean BaseColor, Metallic, Roughness, AO and Normal quality pass!

If a fidelity candidate gets through all five-map bake/QA and geometry
checks it is named UV Fidelity (More Islands). The UI shows the true
UV count increase and warns that hand-editing may be harder. It is
NOT falsely called UV Clean or a semantic atlas.

Older reports without an objective keep previous compact-only rules.

## xatlas chart-growth experiments (not a Fox-specific rule)

The xatlas Python library offers native ChartOptions. Studio now
tests THREE optional xatlas candidates before GPU/Cycles bake:
- default: previous proven xatlas.parametrize baseline.
- joined: ChartOptions.max_cost=5, normal_deviation_weight=1,
  normal_seam_weight=1, straightness_weight=2, max_iterations=2.
- broad: max_cost=10, normal_deviation_weight=0.4,
  normal_seam_weight=0.2, straightness_weight=0.5, max_iterations=3.

High chart max_cost allows larger face groups, potentially fewer UV
islands, but actual outcomes are NOT predetermined; every candidate
must pass the shared strict UV preflight and every map must still pass
PBR fidelity checks. Failures on optional native chart options do not
break the existing xatlas default or Blender angle-based UV engine.

References:
https://github.com/jpcy/xatlas/blob/master/source/xatlas/xatlas.h
https://github.com/mworchel/xatlas-python/blob/main/README.md

## Offline checks, NO GitHub CI

  python -m unittest discover -s tests -p test_uv_planner.py
  python -m unittest discover -s tests -p test_uv_xatlas_provider.py
  python -m unittest discover -s tests -p test_uv_repair_contract.py
  python -m unittest discover -s tests -p test_uv_layer_lifecycle.py
  cargo test --manifest-path studio/src-tauri/Cargo.toml --lib uv_repair
  cd studio
  npm run build
  npx playwright test e2e/uv-repair.spec.ts --workers=1

Real 5.1.2 acceptance is still pending: choose original Fox source,
1024px Preserve PBR, UV engine Both, atlas objective Auto and run ONE
trial. If xatlas wins but map quality fails, the versioned
UV_Plan.json, Repair_Run.log and Texture_Quality.json identify the
specific issue. No ComfyUI model regeneration or old Unity modification.

Next: validate another static retail prop to avoid overfitting Fox.
Then add manual seam-edit .blend -> versioned PBR rebake.
