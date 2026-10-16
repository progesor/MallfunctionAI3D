# M2.7 — UV-Only Connected Proxy + In-App Verified xatlas

2026-10-15 · branch feat/desktop-studio-foundation · Draft PR #1
Status: code integrated; real Blender 5.1.2 and xatlas/Unity acceptance pending.
GitHub Actions: manual workflow_dispatch only; none triggered.

## Verified failure from the attached workstation log

The user-uploaded UV_Plan(1).json reported 7,980 source Blender triangles,
262 disconnected source mesh components and a failed topology rebuild:
  topology-rebuild-rejected: Custom source corner normals could not be preserved
The Repair_Run(2).log ends at AI3D_UV_STAGE:rebuild_exact_topology, before
any UV unwrap or PBR bake. All five maps were NOT touched.

Analysis of the earlier user-uploaded GLB (a locally available source
file from the same Fox asset) found that duplicate positions typically
share the same normal direction, but some original glTF vertex normals
are inconsistent with their indexed triangle's winding: 973 face-corner
normal samples have negative dot product with the geometric face normal.
Therefore a Blender-built *welded mesh* need not be able to reproduce
the exact original corner-normal data via normals_split_custom_set.
The previous M2.6 step rejected the entire UV operation on that mismatch.

## General architecture fix

**UV topology proxy != original PBR render mesh**.

1. Keep the original imported render mesh, face winding, material,
   custom corner normals, previous UV0, GLB PBR assets untouched.
2. Duplicate source XYZ/face structure on a separate in-memory UV-only
   connected proxy via exact-position-topology-v1 (no tolerance welding).
3. Copy old UV per loop and stable source face ID into that proxy.
4. Try Blender 5.1 Angle unwrap and optionally MIT xatlas charting on
   the PROXY only. Candidate preflight requires fewer islands AND
   source-relative UV stretch. Neither proxy normals nor proxy mesh
   are used for the final texture bake or exported PBR game model.
5. If a candidate passes, match EACH selected proxy UV loop to the
   original imported render mesh by face ID and exact 3D corner
   coordinate. Transfer only new UV values onto a new UV layer on
   the original work object. Reject ambiguous/missing face IDs.
6. Re-verify all original UV channels and exact source render loop
   normals after transfer (norm drift <= 0.002). Abort if changed.
7. Only then run the existing PBR 5-map bake and strict texture QA
   on the ORIGINAL render mesh with the new UV selected. The final
   glTF still uses original shading and face topology.

Result: Blender-generated proxy normals no longer block UV planning.
Custom normals are not silently discarded to hide a problem.
No unsafe BMesh normal restoration or geometry mutation is performed.

## Dual UV engines — installed and compared from Studio

UVRepairPanel has a new UV MOTOR selector:
- Both (default): Blender angle-based + xatlas if installed.
- Blender only: built-in Blender native UV, no xatlas dependency.
- xatlas only: disabled until xatlas is detected as installed.

Studio checks Blender's embedded Python 3.13 and whether the app-owned
plugin can import xatlas. The explicit 'Install xatlas' button invokes
the embedded blender/install_xatlas_wheel.py setup script (no pip).
The script queries PyPI's pinned xatlas 0.0.11 release metadata, selects
only xatlas-0.0.11-cp313-cp313-win_amd64.whl, downloads only over
HTTPS from files.pythonhosted.org, validates the PyPI SHA256 and expected
file size, safely rejects path traversal in wheel ZIP entries, checks
native .pyd payload and publishes atomically to library/plugins/xatlas.
Import verification uses a fresh Blender Python process AFTER publishing
to avoid Windows DLL locks. No system Blender, ComfyUI or Unity
installation is modified. No dependency is downloaded during UV bake.

Sources:
- xatlas C++ MIT: https://github.com/jpcy/xatlas
- xatlas Python MIT: https://github.com/mworchel/xatlas-python
- Pinned release/wheel metadata: https://pypi.org/project/xatlas/0.0.11/
- Blender custom normals limitations: Blender 4.1+ modeling docs

Source PBR render mesh stays separate from the UV proxy in BOTH engines.
UV island, distortion and per-map fidelity policies remain shared.
Even an installed xatlas engine does not guarantee QA PASS.

## Local acceptance matrix

1. Pull latest branch, launch Studio; original Fox remains available.
2. Open Auto UV + Texture Repair; first select Blender only;
   Preserve profile / 1024 px, Weld UV proxy=ON.
3. Run once. Check Repair_Run.log for:
   AI3D_UV_STAGE:build_uv_topology_proxy
   AI3D_UV_STAGE:auto_unwrap
   AI3D_UV_STAGE:transfer_proxy_uv_to_render_mesh (if preflight PASS).
   If no valid candidate, expected UV_Plan.json without PBR bake.
4. In the same panel click 'Install xatlas 0.0.11' if not ready.
   Wait until the status shows both engines ready.
5. Run xatlas only in a fresh, versioned attempt on the original Fox.
   Compare UV_Plan.json, number of islands, UV stress and real PBR maps.
6. Finally run Both mode to pick the best QA-eligible atlas.
7. Review 3D Inspector base color/tail, seam normals and masks
   visually before any game Unity export.

## Offline checks (no GitHub Actions)

  python -m unittest discover -s tests -p test_uv_proxy_transfer.py
  python -m unittest discover -s tests -p test_uv_topology.py
  python -m unittest discover -s tests -p test_uv_xatlas_provider.py
  python -m unittest discover -s tests -p test_xatlas_installer.py
  python -m unittest discover -s tests -p test_uv_layer_lifecycle.py
  python -m unittest discover -s tests -p test_uv_repair_contract.py
  python -m py_compile blender/uv_topology_blender.py
  python -m py_compile blender/install_xatlas_wheel.py
  python -m py_compile blender/repair_uv_pbr.py
  cd studio; npm run build
  cargo check --manifest-path studio/src-tauri/Cargo.toml

Neither a Blender binary nor Rust toolchain is available in the
assistant execution container. The real Windows runtime and shader
fidelity after actual UV transfer remain unverified. Never label
the newly rebuilt asset game-ready without on-device acceptance.

## Important restrictions

- Current automatic PBR pipeline supports static, one opaque material.
- Vertex colors, shapekeys and armature models require separate adapters.
- If proxy face IDs or source loop normals drift, fail early;
  DO NOT auto-approve damaged normals.
- The previous 262->1733 Smart UV fallback is still disabled.
- Source SHA/immutable archive, native URP/HDRP and current LOD work
  remain untouched.
