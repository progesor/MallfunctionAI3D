"""Offline contract for detecting stale M2.6 worker/UI on a different PC.

Does not need Blender, Cargo, Vite, Windows, GitHub CI, or network access.
"""
from pathlib import Path
import re
import unittest

ROOT=Path(__file__).resolve().parents[1]
RUST=ROOT/"studio/src-tauri/src/uv_engines.rs"
UI=ROOT/"studio/src/components/UVRepairPanel.tsx"
WORKER=ROOT/"blender/repair_uv_pbr.py"
LAUNCHER=ROOT/"05_Start-Studio.ps1"
DIAG=ROOT/"tools/Diagnose-Studio.ps1"


class UvRuntimeSourceContract(unittest.TestCase):
    def test_frontend_and_backend_revision_are_identical(self):
        rust=RUST.read_text(encoding="utf-8")
        ui=UI.read_text(encoding="utf-8")
        ver_r=re.search(r'UV_RUNTIME_REVISION:&str="([^"]+)"',rust)
        ver_ui=re.search(r"UV_RUNTIME_REVISION = '([^']+)'",ui)
        self.assertIsNotNone(ver_r)
        self.assertIsNotNone(ver_ui)
        self.assertEqual(ver_r.group(1),ver_ui.group(1))
        self.assertEqual(ver_r.group(1),"M2.7.1-PROXY-XATLAS")

    def test_bundled_python_worker_is_new_proxy_version(self):
        script=WORKER.read_text(encoding="utf-8")
        self.assertIn('stage("build_uv_topology_proxy")',script)
        self.assertIn('transfer_clean_uv_from_proxy(',script)
        self.assertNotIn('rebuild_exact_topology_work_copy(',script)
        backend=RUST.read_text(encoding="utf-8")
        self.assertIn('WORKER_SOURCE.contains("stage(',backend)
        self.assertIn('worker_sha256()',backend)
        self.assertIn('proxy_worker_ready:worker_has_proxy()',backend)

    def test_stale_worker_is_checked_before_tauri_launch(self):
        launch=LAUNCHER.read_text(encoding="utf-8")
        diagnostic=DIAG.read_text(encoding="utf-8")
        self.assertIn('tools\\Diagnose-Studio.ps1',launch)
        self.assertLess(launch.index('& $diag'),launch.index('& npm run desktop:dev'))
        self.assertIn('refs/remotes/origin/feat/desktop-studio-foundation',diagnostic)
        self.assertIn('rebuild_exact_topology_work_copy(',diagnostic)
        self.assertIn('repair_uv_pbr.py',diagnostic)
        self.assertIn('Get-FileHash',diagnostic)
        self.assertIn('Get-NetTCPConnection',diagnostic)

    def test_mismatched_runtime_blocks_unsafe_uv_operations(self):
        text=UI.read_text(encoding="utf-8")
        self.assertIn('engineStatus?.backendRevision===UV_RUNTIME_REVISION',text)
        self.assertIn('engineStatus?.proxyWorkerReady===true',text)
        self.assertIn('!installingXatlas && runtimeOk',text)
        self.assertIn('data-testid="uv-runtime-mismatch"',text)
        tests=(ROOT/"studio/e2e/uv-repair.spec.ts").read_text(encoding="utf-8")
        self.assertIn("openStudio(page,true,false,true)",tests)
        self.assertIn("toBeDisabled()",tests)


if __name__=="__main__":
    unittest.main()
