# Windows TSX syntax hotfix — 2026-10-15

Repo: progesor/MallfunctionAI3D
Branch: feat/desktop-studio-foundation (Draft PR #1)
CI policy: manual workflow_dispatch only, not run for this correction.

## Observed failure

On the secondary Windows workstation Tauri started Vite but React's
Babel transform failed parsing studio/src/App.tsx around line 408:

  Adjacent JSX elements must be wrapped in an enclosing tag.

The prior QA FAIL / UV REVIEW banner was introduced as conditional JSX:

  {reviewOnlyAsset && <p role="alert">...contents...</p>

The closing } was omitted. The next stats-grid <div> therefore appeared
inside the unfinished JSX JavaScript expression, triggering the parser
error. This was not a Rust, DLL, ComfyUI, asset or Blender failure.

## Fix and guard

- studio/src/App.tsx: close the conditional JSX with </p>}.
- studio/scripts/check-jsx-syntax.mjs: use the existing TypeScript
  devDependency to parse all src/**/*.ts and src/**/*.tsx plus
  vite.config.ts. Print exact file/line/column and exit nonzero on
  syntax errors; no network, CI, or new packages required.
- studio/package.json: run the syntax checker BEFORE invoking Vite's
  development server. Exposes npm run check:syntax and --self-test
  option for the exact missing-brace regression.
- Keep the Unity review-only export guards and all original source assets
  unchanged.

## Windows local check

  cd C:\Works\MallfunctionAI3D
  git pull --ff-only
  cd studio
  npm run check:syntax
  node scripts/check-jsx-syntax.mjs --self-test
  npm run build
  cd ..
  .\05_Start-Studio.ps1

Expected local syntax check: TS/TSX syntax PASS (N files).
The final Windows Vite/React/Rust desktop acceptance requires the user's
local machine because no full repo/node_modules/Rust toolchain is mounted
in this assistant container. GitHub Actions stays manual-only.
