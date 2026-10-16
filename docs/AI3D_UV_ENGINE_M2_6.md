# M2.6 · Model-Agnostic Topology Repair + Pluggable UV Engines

2026-10-15 · feat/desktop-studio-foundation · Draft PR #1
CI: manual-only; no Windows GitHub Actions invoked.
Status: structural integration ready; real Blender 5.1 bake acceptance pending.

## Scope

The UV/PBR repair engine must process many future AI-created models, not
only PLAYBOX Fox Plush. No hardcoded head/tail/body segmentation or
special-case transforms are introduced. The strategy is a *provider*
pipeline: validate source data -> reconstruct safe shared POSITION
topology on a fresh work mesh -> trial Blender angle-based UV charts and
optional MIT xatlas charts -> evaluate source-relative UV stretch and
strict island count BEFORE bake -> five map role-aware PBR reproject/bake
-> M2.5.1 surface quality acceptance -> optional manual intervention.

Original GLB, ComfyUI workflow, Unity native export and all previous
arbitrary model versions remain immutable.

## Real root cause confirmed by uploaded source (no texture decoding)

Uploaded original.glb is one static glTF triangle primitive, with:
- 7,980 indexed triangles, 6,639 vertex records, one opaque PBR material.
- EXACT unique XYZ positions: 3,853 (2,786 redundant UV-split vertices).
- Indexed mesh has 262 disconnected groups; exact position reconstruction
  has only 4 connected groups (face indices and orientation preserved).
- Boundary edges: 4,871 before -> only 6 after exact position weld.
- Over-subscribed/non-manifold edges: 1 before -> 15 after geometric weld.
- No triangle collapses or coordinate drift; imported source normals at
  coincident XYZ are consistent in this sample. Other assets may differ.

Uploaded UV_Plan.json reports Blender weld attempted but rejected:
  internal error setting the array
and therefore 262 components and all five UV candidates being invalid.
Every attempt has null UV stretch / 7,980 degenerate UV-edge triangles.
The reported 494/586/714/967/1290 islands are consequences of missing
shared topology. This is a general GLB vertex-splitting failure, not
evidence that the Fox requires an anatomy-specific UV algorithm.

## Generic exact-position topology engine

blender/uv_topology.py is pure Python and has no Blender dependency.
It calculates an index-preserving weld PLAN from triangle POSITION
coordinates (strict exact float equality by default; no speculative
epsilon merge), checks for face collapse, validates each 3D corner,
and reports connected groups, boundaries and non-manifold edges.
Compatible with future static triangular meshes from many generators.

blender/uv_topology_blender.py uses this plan to CREATE A FRESH Blender
Mesh with from_pydata instead of calling remove_doubles/to_mesh on
a GLB-owned BMesh (which failed on Blender 5.1.2). It copies each source
UV layer per FACE CORNER, copied material/shading indices, stable face IDs,
and attempts to restore full custom split normals per loop using
Mesh.normals_split_custom_set. Verifies UV/XYZ/normals/triangle counts
BEFORE atomically replacing the disposable target mesh.

Unsafe or unsupported data (non-triangles, shape keys, vertex colors,
armature/vertex groups, topology collapse or normal mismatch) causes
an explicit pre-bake failure. No visible faces are removed, holes
automatically filled, PBR maps baked, or source GLB overwritten.

Any BMesh/Blender runtime API incompatibility on the user's 5.1.2
workstation remains possible until a real validation run. Diagnostic
files and exact reason are preserved rather than silently continuing.

## Optional ready-made UV framework

Primary choice: xatlas C++ by jpcy, MIT license.
  https://github.com/jpcy/xatlas
Unofficial Python binding, also MIT:
  https://github.com/mworchel/xatlas-python
PyPI xatlas 0.0.11 has a CPython 3.13 win_amd64 wheel:
  https://pypi.org/project/xatlas/0.0.11/
Blender 5.1 embeds CPython 3.13 and does not load user site dirs by default.

blender/uv_provider_xatlas.py is an optional provider. If installed
under Studio's app-owned library/plugins/xatlas directory it supplies
an additional candidate; if absent, the normal Blender angle-based
provider keeps working. Each produced UV face/vertex mapping MUST match
the input triangle faces. A missing, reordered or out-of-bounds mapping
declines the candidate. A successful xatlas call is NOT an automatic
approval: it passes through the SAME island, stretch and PBR quality
gates as Blender. xatlas is never downloaded during a bake.

Optional explicit installation (NOT required for basic topology repair):

  .\tools\Install-XatlasForStudio.ps1 -BlenderPython
      'C:\Program Files\Blender Foundation\Blender 5.1\5.1\python\bin\python.exe'

This uses xatlas==0.0.11, --no-deps, --only-binary=:all:, an app-specific
plugin directory, and a local import check; it does not modify Blender
site-packages. If Blender has no pip, the script stops with a diagnosis.
On machines with customized Studio library location, use -LibraryRoot.
Do not run the optional install unless you want the extra dependency.

Secondary reviewed framework: Microsoft UVAtlas (MIT), but Microsoft
labels the repository archived/not recommended for new projects and
it recommends manifold conditioning for non-manifold input. Therefore
not chosen as the primary engine.
  https://github.com/microsoft/UVAtlas

## Generic offline diagnostics

Read-only CLI accepts an arbitrary glTF 2.0 embedded GLB, including
multiple triangle primitives (each is audited separately):

  python tools/inspect_glb_topology.py path/to/original.glb

No extra pip, Blender or image decompress step is required.
Output is a JSON report with source and rebuilt topology measurements.

## Offline tests without GitHub CI

  python -m unittest discover -s tests -p test_uv_topology.py
  python -m unittest discover -s tests -p test_uv_xatlas_provider.py
  python -m unittest discover -s tests -p test_uv_layer_lifecycle.py
  python -m unittest discover -s tests -p test_uv_planner.py
  python -m unittest discover -s tests -p test_uv_repair_contract.py
  python -m py_compile blender/uv_topology.py
  python -m py_compile blender/uv_topology_blender.py
  python -m py_compile blender/uv_provider_xatlas.py
  python -m py_compile blender/repair_uv_pbr.py
  cargo check --manifest-path studio/src-tauri/Cargo.toml

Offline pure-topology validation on the attached Fox GLB PASSED:
6,639 -> 3,853 positions, 262 -> 4 groups, 7,980 faces unchanged.
Blender 5.1.2 normals transfer and xatlas-native chart quality are not
yet measured on the user's Windows station; no game-ready claim.

## Acceptance matrix for future content types

- Soft toys/cartoon characters: larger coherent islands, no color
  boundary drift, matched world-space normal appearance.
- Furniture/electronics/retail props: retain original hard edges,
  material slots and non-manifold boundaries; no unapproved overlap.
- Garments/cloth: preserve fine folds and authored normals; if
  skinned or vertex-colored, reject until corresponding transfer
  adapter exists rather than silently dropping vertex data.
- Multi-material or animated input: not yet covered by the current
  single-opaque-static production export; future provider interfaces
  must explicitly support them before enabling such conversions.

Each run records engine selected, source SHA, topology before/after
metrics, UV island count, relative distortion, five PBR map scores and
separate Unity game-export acceptance. Manual UV edit/rebake remains
a downstream controlled versioned operation.
