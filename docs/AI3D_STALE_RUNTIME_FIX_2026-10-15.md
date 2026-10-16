# M2.7.1 — Prevent stale Studio/Blender worker on Windows workstations

Date: 2026-10-15 | Branch: feat/desktop-studio-foundation (Draft PR #1)
GitHub Actions are manual-only. No CI run was requested or triggered.

## Confirmed problem from user's latest uploaded files

UV_Plan(2).json and Repair_Run(3).log show:

  AI3D_UV_STAGE:rebuild_exact_topology
  UV_PREFLIGHT_REJECTED: Custom source corner normals could not be preserved

That is the OLD M2.6 worker. It calls rebuild_exact_topology_work_copy,
which reconstructs corner normals and rejected the Fox 7980-triangle GLB.

The current M2.7 branch includes the NEW worker at:

  AI3D_UV_STAGE:build_uv_topology_proxy

The newer implementation preserves the source PBR render mesh and original
custom normals and creates a separate UV-only connected helper mesh.

Therefore the user's supplied 11:43:57 log is NOT evidence that M2.7
failed. It proves M2.6 executed. The missing Blender/xatlas/Both selector
is a second indicator of an outdated frontend, wrong branch, stale Tauri
process, or a mismatched development directory.

## M2.7.1 changes

1. Runtime version handshake: uv_engine_status now returns
   backendRevision='M2.7.1-PROXY-XATLAS', proxyWorkerReady and a SHA256
   fingerprint of the Python worker EMBEDDED in the Rust binary.
   UVRepairPanel displays UI revision, backend revision and worker hash.
   If versions disagree, or worker lacks UV-proxy markers, it disables
   the new UV bake and tells the user to restart/update.
2. A visible UV / xatlas shortcut appears in the workspace toolbar.
   Clicking it reveals the right-side Inspector's UV MOTOR selector.
   Blender/xatlas/Both are selected BEFORE starting UV/PBR repair.
3. tools/Diagnose-Studio.ps1 is OFFLINE and read-only. Prints repository
   path, current Git branch/commit, required source markers and current
   worker SHA256. With optional -RepairDir it compares a failed archived
   worker source against the current checkout and reports last stage.
   It also warns about any Vite listener on port 1420. No processes
   are killed, no files are deleted, and no Git fetch is done implicitly.
4. 05_Start-Studio.ps1 now calls Diagnose-Studio BEFORE Cargo/Vite starts,
   refuses outdated/wrong-branch source markers and refuses port 1420
   conflicts. No source, ComfyUI, Blender plugins, Unity output or asset
   library content is overwritten.
5. Synthetic regressions check backend/frontend revision alignment,
   source markers and the E2E behavior of blocking a stale backend.

## How to recover cleanly on the affected PC

Close the old Mallfunction AI3D Studio window(s), Tauri and Vite first.
From PowerShell in the INTENDED repository checkout:

  cd C:\Works\MallfunctionAI3D
  git status --short
  git fetch origin feat/desktop-studio-foundation
  git switch feat/desktop-studio-foundation
  git pull --ff-only origin feat/desktop-studio-foundation
  .\tools\Diagnose-Studio.ps1
  .\05_Start-Studio.ps1

To compare a specific failed 7980-triangle run without modifying it:

  .\tools\Diagnose-Studio.ps1 -RepairDir
    '<APPDATA>\net.mallfunction.ai3dstudio\library\assets\<asset-UUID>\uv_repairs\<run-UUID>'

Expected new start: source diagnostic PASS. Under UV / xatlas, UI and
backend both show M2.7.1-PROXY-XATLAS and a nonempty SHA256 fingerprint.
First Blender log should start 'AI3D_UV_STAGE:build_uv_topology_proxy'
when the user enables vertex connectivity recovery.

Old interrupted/broken runs stay in the archive. An old run cannot be
converted into a new code run: each fresh UV attempt embeds the exact
currently compiled worker and uses a new, versioned directory.

## Why no UV bake should be repeated yet

If source revision is stale, further UV attempts only execute the same
obsolete normal-reconstruction failure and cannot test the new proxy.
Do not regenerate ComfyUI GLB or delete previous failed UV attempts.

## Offline checks

  python -m unittest discover -s tests -p test_studio_runtime_version.py
  cd studio; npm run check:syntax; npm run build
  cargo check --manifest-path src-tauri/Cargo.toml

Real local Windows Tauri/Blender startup must still be verified by the
user. The assistant cannot access currently running workstation processes.
