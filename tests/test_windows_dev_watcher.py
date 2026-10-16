"""Static regression guard for Windows Vite + Tauri EBUSY DLL watchers.

Run locally without GitHub Actions:
  python -m unittest discover -s tests -p "test_windows_dev_watcher.py"

Actual Vite file watching is validated on the user's Windows workstation.
"""
import fnmatch
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
VITE = ROOT / "studio" / "vite.config.ts"
RUST = ROOT / "studio" / "src-tauri" / "src" / "uv_repair.rs"


def watch_ignored_patterns(config):
    match = re.search(r"ignored:\s*\[([^\]]+)\]", config, re.S)
    if match is None:
        raise AssertionError("Vite watch.ignored config absent")
    return re.findall(r"['\"]([^'\"]+)['\"]", match.group(1))


def matched(patterns, path):
    # Chokidar/anymatch glob paths are slash-normalized on Windows. Exercise
    # the exact generated DLL location reported in the user's crash.
    name = path.replace("\\", "/")
    return any(fnmatch.fnmatchcase(name, item) for item in patterns)


class WindowsDevWatcherContract(unittest.TestCase):
    def test_generated_dll_and_all_rust_build_outputs_are_not_watched(self):
        config = VITE.read_text(encoding="utf-8")
        ignored = watch_ignored_patterns(config)
        for relative in (
            r"studio\src-tauri\target",
            r"studio\src-tauri\target\debug\deps\mallfunction_ai3d_studio_lib.dll",
            r"studio\src-tauri\target\debug\mallfunction-ai3d-studio.exe",
            r"studio\src-tauri\gen\schemas\windows-schema.json",
            r"studio\src-tauri\src\uv_repair.rs",
        ):
            with self.subTest(relative=relative):
                path = "C:\\Works\\MallfunctionAI3D\\" + relative
                self.assertTrue(matched(ignored, path), path)

    def test_frontend_files_still_get_hmr(self):
        ignored = watch_ignored_patterns(VITE.read_text(encoding="utf-8"))
        for relative in ("studio/src/App.tsx", "studio/src/components/UVRepairPanel.tsx",
                         "studio/vite.config.ts"):
            self.assertFalse(matched(ignored, "C:/Works/MallfunctionAI3D/" + relative))

    def test_legacy_test_wrapper_does_not_warn_in_rust_dev_build(self):
        rust = RUST.read_text(encoding="utf-8")
        self.assertRegex(rust, r"#\[cfg\(test\)\]\s*fn verify_repair_report\(")
        self.assertIn("fn verify_repair_report_mode(", rust)


if __name__ == "__main__":
    unittest.main()
