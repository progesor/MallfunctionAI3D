"""Regression: emulate Blender 5.x UV RNA handles invalidated by BMesh writes
and Object/Edit mode switches, without requiring Blender or GitHub CI.

Real Blender 5.1.2 Fox failure on 2026-10-15:
  UnicodeDecodeError in new_uv(): uv_island_count(mesh, layer.name)
After an operator, Blender no longer guarantees the original UV layer wrapper
has a valid .name or .data pointer. This fake raises that exact error.
"""
import ast
import math
from pathlib import Path
from types import SimpleNamespace
import unittest

WORKER = Path(__file__).resolve().parents[1] / "blender" / "repair_uv_pbr.py"


class UVHandle:
    def __init__(self, mesh, key):
        self.mesh = mesh
        self.key = key
        self.created_at = mesh.generation

    def assert_alive(self):
        if self.created_at != self.mesh.generation:
            raise UnicodeDecodeError(
                "utf-8", b"\x89\x8b", 1, 2, "invalidated UV layer RNA handle"
            )

    @property
    def name(self):
        self.assert_alive()
        return self.key

    @property
    def data(self):
        self.assert_alive()
        return [SimpleNamespace(uv=uv) for uv in self.mesh.layers.items[self.key]]

    @property
    def active_render(self):
        self.assert_alive()
        return True

    @active_render.setter
    def active_render(self, _value):
        self.assert_alive()


class Layers:
    def __init__(self, mesh):
        self.mesh = mesh
        self.items = {"UVMap": [(0.0, 0.0), (1.0, 1.0)]}
        self.active_key = "UVMap"

    def get(self, name):
        return UVHandle(self.mesh, name) if name in self.items else None

    def new(self, *, name):
        candidate = name
        offset = 1
        while candidate in self.items:
            candidate = f"{name}.{offset:03d}"
            offset += 1
        self.items[candidate] = [(0.10, 0.15), (0.90, 0.85)]
        return self.get(candidate)

    @property
    def active(self):
        return self.get(self.active_key)

    @active.setter
    def active(self, value):
        self.active_key = value.name


class Mesh:
    def __init__(self):
        self.generation = 0
        self.uv_layers = Layers(self)


class Target:
    def __init__(self):
        self.data = Mesh()

    def select_set(self, _value):
        pass


class FakeBlender:
    def __init__(self, target, old_count=168, new_count=12):
        self.target = target
        self.old_count = old_count
        self.new_count = new_count
        self.context = SimpleNamespace(
            view_layer=SimpleNamespace(objects=SimpleNamespace(active=None))
        )
        self.ops = SimpleNamespace(
            object=SimpleNamespace(
                select_all=lambda **_kw: None,
                mode_set=self.mode_set,
            ),
            mesh=SimpleNamespace(select_all=lambda **_kw: None),
            uv=SimpleNamespace(
                unwrap=lambda **_kw: {"FINISHED"},
                average_islands_scale=lambda **_kw: {"FINISHED"},
                pack_islands=lambda **_kw: {"FINISHED"},
                smart_project=lambda **_kw: {"FINISHED"},
            ),
        )

    def mode_set(self, **_kw):
        self.target.data.generation += 1
        return {"FINISHED"}

    def curvature_seams(self, mesh, _threshold):
        # BMesh.to_mesh() also invalidates UVLayer wrappers.
        mesh.generation += 1
        return 7, 2

    def uv_island_count(self, _mesh, name):
        return self.old_count if name == "UVMap" else self.new_count

    def compile_new_uv(self):
        parsed = ast.parse(WORKER.read_text(encoding="utf-8"), filename=str(WORKER))
        func = next(n for n in parsed.body
                    if isinstance(n, ast.FunctionDef) and n.name == "new_uv")
        program = ast.fix_missing_locations(ast.Module(body=[func], type_ignores=[]))
        context = {
            "bpy": self,
            "math": math,
            "uv_island_count": self.uv_island_count,
            "mark_curvature_seams": self.curvature_seams,
        }
        exec(compile(program, str(WORKER), "exec"), context)
        return context["new_uv"]


class UVModeSwitchRegression(unittest.TestCase):
    def test_angle_unwrap_does_not_read_expired_uv_layer(self):
        target = Target()
        fake = FakeBlender(target)
        old, clean, report = fake.compile_new_uv()(target, 62, 24, 1024)
        self.assertEqual(old, "UVMap")
        self.assertEqual(clean, "AI3D_Clean")
        self.assertEqual(report["strategy"], "curvature-seams-angle-unwrap")
        self.assertEqual(report["newUVIslands"], 12)
        self.assertGreaterEqual(target.data.generation, 3)

    def test_smart_fallback_reacquires_handle_after_mode_switch(self):
        target = Target()
        fake = FakeBlender(target, old_count=50, new_count=80)
        old, clean, report = fake.compile_new_uv()(target, 62, 24, 1024)
        self.assertEqual((old, clean), ("UVMap", "AI3D_Clean"))
        self.assertEqual(report["strategy"], "smart-project-fallback")
        self.assertEqual(report["newUVIslands"], 80)

    def test_new_atlas_uses_new_name_without_overwriting_existing(self):
        target = Target()
        target.data.uv_layers.items["AI3D_Clean"] = [(0.0, 0.0), (1.0, 1.0)]
        fake = FakeBlender(target)
        _, clean, _ = fake.compile_new_uv()(target, 62, 24, 1024)
        self.assertEqual(clean, "AI3D_Clean.001")
        self.assertIn("AI3D_Clean", target.data.uv_layers.items)


if __name__ == "__main__":
    unittest.main()
