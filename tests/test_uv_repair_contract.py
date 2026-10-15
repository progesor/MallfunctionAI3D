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
        self.assertIn('from uv_texture_fidelity import (', source)
        self.assertIn('def sample_texture_fidelity(', source)
        self.assertIn('def remove_orphan_geometry_only(', source)
        self.assertIn('stage("texture_fidelity")', source)
        self.assertIn('Texture_Quality.json', source)
        self.assertIn('UV_Stress.svg', source)
        self.assertIn('from uv_planner import (', source)
        self.assertIn('from uv_topology_blender import rebuild_exact_topology_work_copy', source)
        self.assertIn('from uv_provider_xatlas import apply_xatlas_uv', source)
        self.assertIn('stage("rebuild_exact_topology")', source)
        self.assertIn('UV_PREFLIGHT_REJECTED', source)
        self.assertIn('"weld": weld_diagnostics', source)
        self.assertIn('sourceCornerPositionsPreserved',
            (ROOT / "blender" / "uv_topology.py").read_text(encoding="utf-8"))
        self.assertIn('UV_PREFLIGHT_REJECTED', source)
        self.assertIn('UV_Plan.json', source)
        self.assertIn('acceptedForBake', source)
        self.assertIn('UV_Plan_Working.blend', source)
        # Never split a failed topology into 1733 islands and run Cycles.
        candidate = source[source.index('def new_uv('):source.index('def emit_uv_stress_layout(')]
        self.assertNotIn('bpy.ops.uv.smart_project(', candidate)
        self.assertIn('select_bake_candidate(attempted)', candidate)
        self.assertLess(source.index('if not uv_report["accepted"]:'),
                        source.index('stage("bake_" + role)'))
        self.assertIn('"uvStretchGate": stretch_gate', source)
        self.assertIn('"schemaVersion": 2', source)
        self.assertIn('"p99AbsoluteError"', (
            ROOT / "blender" / "uv_texture_fidelity.py").read_text(encoding="utf-8"))
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
        self.assertIn("fn verify_repair_report(", rust)
        self.assertIn("if before!=after || before.is_none()", rust)
        self.assertIn("explicit-user-approved-one-import-face-loss", rust)
        self.assertIn("sourceImportTriangleDelta", rust)
        self.assertIn("doğrulanmış sıfır-alan üçgeni=", rust)
        self.assertIn("fn finalize_repair(", rust)
        self.assertIn("fn list_uv_repair_attempts(", rust)
        self.assertIn("fn resume_uv_repair(", rust)
        self.assertIn("audit_source_triangles(&original)", rust)
        self.assertIn("const FIDELITY_WORKER:", rust)
        self.assertIn("const UV_PLANNER_WORKER:", rust)
        self.assertIn("const TOPOLOGY_WORKER:", rust)
        self.assertIn("const TOPOLOGY_ADAPTER_WORKER:", rust)
        self.assertIn("const XATLAS_ADAPTER_WORKER:", rust)
        self.assertIn('("uv_provider_xatlas.py",XATLAS_ADAPTER_WORKER)', rust)
        self.assertIn("UV atlası bake öncesinde reddedildi", rust)
        self.assertIn("fn verify_texture_quality(", rust)
        self.assertIn('"qualityGate":"surface-fidelity-local-distortion-v2"', rust)
        self.assertIn('UV gerilmesi:', rust)
        self.assertIn('"uvAtlasSimplified"', rust)
        self.assertIn("fn verify_texture_quality(", rust)
        self.assertIn("fn check_texture_quality(", rust)
        self.assertIn("if !accepted && !allow_review_quality_failure", rust)
        self.assertIn("pub async fn save_uv_review(", rust)
        self.assertIn("fn ensure_unity_exportable(", rust)
        self.assertIn("quality_status:if quality", rust)
        self.assertIn("review_only:prepared.review_only", rust)
        self.assertIn("verify_repair_report_mode(", rust)
        self.assertIn("safe_cleanup:bool", rust)
        self.assertIn("new UV island count is greater than source count", rust)


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


class RejectedQualityRecoveryContract(unittest.TestCase):
    def test_failed_bake_visible_but_not_unity_exportable(self):
        app = (ROOT / "studio/src/App.tsx").read_text(encoding="utf-8")
        panel = (ROOT / "studio/src/components/UVRepairPanel.tsx").read_text(encoding="utf-8")
        backend = RUST.read_text(encoding="utf-8")
        native = (ROOT / "studio/src-tauri/src/unity_native.rs").read_text(encoding="utf-8")
        legacy = (ROOT / "studio/src-tauri/src/unity_export.rs").read_text(encoding="utf-8")
        self.assertIn("quality_status:if quality", backend)
        self.assertIn("!review_only && matches!((old,new)", backend)
        self.assertIn("accept_failed_quality:bool", backend)
        self.assertIn("productionExportBlocked", backend)
        self.assertIn("ensure_unity_exportable(", native)
        self.assertIn("ensure_unity_exportable(", legacy)
        self.assertIn("!reviewOnlyAsset", app)
        self.assertIn("const matches = recoverable;", panel)
        self.assertIn("recovery.profile,recovery.resolution", panel)
        self.assertIn("QA başarısız sürümü taslak olarak kurtar", panel)
        self.assertIn("acceptFailedQuality", panel)

if __name__ == "__main__":
    unittest.main()
