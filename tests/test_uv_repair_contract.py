"""Offline contract checks for the UV worker. No Blender installation needed.
This does not substitute for a real bake on the PLAYBOX Fox model.
Run: python -m unittest discover -s tests -p "test_uv_repair_contract.py"
"""
import ast
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
WORKER = ROOT / "blender" / "repair_uv_pbr.py"
RUST = ROOT / "studio" / "src-tauri" / "src" / "uv_repair.rs"


class UVRepairContractTest(unittest.TestCase):
    def test_worker_python_parses_and_has_major_stages(self):
        source = WORKER.read_text(encoding="utf-8")
        tree = ast.parse(source, filename=str(WORKER))
        names = {n.name for n in tree.body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))}
        self.assertTrue({
            "main", "parse_args", "attempt_safe_weld", "new_uv",
            "mark_curvature_seams", "cut_closed_component", "uv_island_count",
            "bake_image", "preview_material", "emit_uv_layout",
        }.issubset(names))
        self.assertIn('bpy.ops.uv.unwrap(', source)
        self.assertIn('method="ANGLE_BASED"', source)
        self.assertIn('bpy.ops.object.bake(type="NORMAL"', source)
        self.assertIn('bpy.ops.object.bake(type="EMIT"', source)
        self.assertIn('"SourceMaps"', RUST.read_text(encoding="utf-8"))
        # Blender 5.1 glTF exporter cannot write directly to Windows long
        # canonicalized archive paths (\\\\?\\C:\\...). Keep it in short temp.
        self.assertIn('stage_geometry_glb(output / "geometry.glb", bpy.ops.export_scene.gltf)', source)
        self.assertIn('tempfile.TemporaryDirectory(prefix="ai3d_uv_glb_")', source)
        self.assertIn('destination.open("xb")', source)

    def test_worker_does_not_generate_ai_jobs_or_remesh_models(self):
        source = WORKER.read_text(encoding="utf-8")
        # Geometry can be welded on a disposable copy, never retopologized,
        # decimated, or remeshed by the UV repair operation.
        self.assertNotIn("quadriflow_remesh(", source)
        self.assertNotIn("modifier_apply(", source)
        self.assertNotIn("comfy_submit_job", source)
        self.assertIn("sourceUntouched", source)
        self.assertIn('"Repair_Working.blend"', source)

    def test_shader_mapping_is_explicit_and_content_versioned(self):
        rust = RUST.read_text(encoding="utf-8")
        self.assertIn('Rgba([occlusion.get_pixel(x,y)[0],r,m,255])', rust)
        self.assertIn('"occlusionTexture":{"index":1', rust)
        self.assertIn('"metallicRoughnessTexture":{"index":1}', rust)
        self.assertIn('state.store_asset(&prepared.source', rust)
        self.assertIn("verify_sha256(&file_path,&source_sha)", rust)
        self.assertIn("if new_islands==0 || new_islands>old_islands", rust)


class MissingReportDiagnosticsRegression(unittest.TestCase):
    def test_blender_python_exception_exit_code_and_stage_reporting(self):
        repo = ROOT
        worker = (repo / "blender" / "repair_uv_pbr.py").read_text(encoding="utf-8")
        backend = (repo / "studio" / "src-tauri" / "src" / "uv_repair.rs").read_text(encoding="utf-8")
        self.assertIn('.arg("--python-exit-code").arg("23")', backend)
        self.assertIn('fn read_blender_result(', backend)
        self.assertIn('Blender başarı raporunu üretmedi', backend)
        self.assertIn('Repair_Run.log', backend)
        self.assertIn('PBR kaynak haritası doğrulanamadı', backend)
        self.assertIn('def stage(label):', worker)
        self.assertIn('traceback.print_exc()', worker)
        self.assertIn('sys.exit(23)', worker)

if __name__ == "__main__":
    unittest.main()
