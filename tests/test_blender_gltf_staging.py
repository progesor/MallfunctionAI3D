"""Offline regression for Blender 5.1 Windows glTF WinError 123.

Only pure helper functions are loaded from the real worker AST. No Blender
installation, ComfyUI workload, public CI minutes or asset source files needed.
"""
import ast
import ntpath
import os
from pathlib import Path
import shutil
import struct
import tempfile
import unittest

WORKER = Path(__file__).resolve().parents[1] / "blender" / "repair_uv_pbr.py"


def load_actual_helpers():
    tree = ast.parse(WORKER.read_text(encoding="utf-8"), filename=str(WORKER))
    wanted = {"blender_compatible_gltf_path", "stage_geometry_glb"}
    found = [node for node in tree.body
             if isinstance(node, ast.FunctionDef) and node.name in wanted]
    if {node.name for node in found} != wanted:
        raise AssertionError("Blender glTF staging helpers are missing")
    module = ast.fix_missing_locations(ast.Module(body=found, type_ignores=[]))
    context = {
        "os": os, "ntpath": ntpath, "shutil": shutil, "struct": struct,
        "tempfile": tempfile, "Path": Path,
    }
    exec(compile(module, str(WORKER), "exec"), context)
    return context


def minimal_glb():
    """36-byte synthetic GLB 2.0 with JSON/BIN chunks (no actual meshes)."""
    payload = b"glTF" + struct.pack("<II", 2, 36)
    payload += struct.pack("<I4s", 4, b"JSON") + b"{}  "
    payload += struct.pack("<I4s", 4, b"BIN\x00") + b"\x00" * 4
    assert len(payload) == 36
    return payload


class BlenderWindowsGltfStagingRegression(unittest.TestCase):
    def setUp(self):
        funcs = load_actual_helpers()
        self.blender_path = funcs["blender_compatible_gltf_path"]
        self.export = funcs["stage_geometry_glb"]

    def test_extended_windows_archive_path_is_never_passed_to_addon(self):
        original = (
            r"\\?\C:\Users\TestUser\AppData\Roaming\net.mallfunction.ai3dstudio"
            r"\library\assets\aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
            r"\uv_repairs\bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
            r"/geometry.glb"
        )
        self.assertTrue(original.startswith("\\\\?\\"))
        # This same path originally produced WinError 123 when the addon
        # appended a slash and called os.makedirs on the extended prefix.
        fixed = self.blender_path(original, windows=True)
        self.assertTrue(fixed.startswith("C:\\Users\\TestUser\\"))
        self.assertNotIn("\\\\?\\", fixed)
        self.assertNotIn("/", fixed)
        self.assertTrue(fixed.endswith("geometry.glb"))

    def test_unsafe_temp_unc_and_overlong_native_paths_are_rejected(self):
        with self.assertRaises(ValueError):
            self.blender_path(r"\\?\UNC\server\share\asset.glb", windows=True)
        with self.assertRaises(ValueError):
            self.blender_path("C:\\" + "a" * 270 + ".glb", windows=True)

    def test_real_staging_helper_exports_to_temp_then_safely_archives(self):
        payload = minimal_glb()
        calls = []
        with tempfile.TemporaryDirectory() as root:
            destination = Path(root) / "archive" / "geometry.glb"
            destination.parent.mkdir()
            def fake_blender_exporter(**args):
                calls.append(args)
                actual = Path(args["filepath"])
                self.assertTrue(actual.parent.is_dir())
                self.assertEqual(actual.name, "geometry.glb")
                self.assertEqual(args["export_format"], "GLB")
                self.assertEqual(args["export_materials"], "NONE")
                self.assertFalse(str(actual).startswith("\\\\?\\"))
                actual.write_bytes(payload)
                return {"FINISHED"}
            self.export(destination, fake_blender_exporter)
            self.assertEqual(destination.read_bytes(), payload)
            self.assertEqual(len(calls), 1)
            # Immutable version: already-present output must not be overwritten.
            with self.assertRaises(FileExistsError):
                self.export(destination, fake_blender_exporter)
            self.assertEqual(destination.read_bytes(), payload)

    def test_exporter_failure_or_invalid_glb_never_publishes_a_file(self):
        with tempfile.TemporaryDirectory() as root:
            dest = Path(root) / "geometry.glb"
            def broken(**args):
                Path(args["filepath"]).write_bytes(b"not a valid GLB")
                return {"FINISHED"}
            with self.assertRaisesRegex(RuntimeError, "header/length"):
                self.export(dest, broken)
            self.assertFalse(dest.exists())
            with self.assertRaisesRegex(RuntimeError, "did not finish"):
                self.export(dest, lambda **kwargs: {"CANCELLED"})
            self.assertFalse(dest.exists())


if __name__ == "__main__":
    unittest.main()
