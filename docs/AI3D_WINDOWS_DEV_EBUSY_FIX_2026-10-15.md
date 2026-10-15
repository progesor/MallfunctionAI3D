# Windows development: Vite EBUSY locked Rust DLL

2026-10-15 | MallfunctionAI3D Studio | feat/desktop-studio-foundation

## Actual failure and scope

On a second Windows development PC (Node v24.18.0), the Tauri dev command
failed in beforeDevCommand because the Vite/Chokidar watcher attempted
fs.watch on:

  C:\Works\MallfunctionAI3D\studio\src-tauri\target\debug\deps\mallfunction_ai3d_studio_lib.dll

Windows reports EBUSY while Cargo is rebuilding/loading that DLL. Vite
tries to watch the entire studio root unless src-tauri is excluded.
The warning 'function verify_repair_report is never used' was unrelated
to the startup failure; only Rust unit tests call that compatibility wrapper.

## Fix

- studio/vite.config.ts: configure server.watch.ignored for both
  '**/src-tauri' and '**/src-tauri/**'. Excludes Rust sources, target,
  generated schemas and compiled DLLs from Vite's frontend HMR watcher.
- Tauri's own Rust project watcher is independent and still rebuilds the
  backend when source changes. React/Three.js source under studio/src
  remains watched and hot-reloaded normally.
- studio/src-tauri/src/uv_repair.rs: mark the test-only wrapper
  verify_repair_report with #[cfg(test)], removing dead_code warning
  during normal application builds.

## Start on Windows

From the updated branch at the project root:

    cd C:\Works\MallfunctionAI3D
    git pull --ff-only
    .\05_Start-Studio.ps1

This should not require deleting Cargo target files or re-installing
ComfyUI, Blender, Unity, Node.js or Rust.

If EBUSY persists, close stale Studio/Tauri/Vite processes belonging
to THIS project (do not blindly terminate unrelated Node apps). You
can identify the process on local port 1420 with:

    Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue

Restart Studio after the previous instance fully exits. Check antivirus
file locking if Windows still reports DLL access errors. Do not delete
target folders while Cargo/Tauri is running.

## Offline regression without GitHub Actions

    python -m unittest discover -s tests -p test_windows_dev_watcher.py
    cd studio; npm run build
    cargo check --manifest-path src-tauri/Cargo.toml

Static watcher contract can be verified without Vite installation.
The final acceptance is running .\05_Start-Studio.ps1 on the affected
Windows PC while Cargo emits DLLs and ensuring Vite stays alive.
GitHub CI remains manual-only and is not triggered by this fix.
