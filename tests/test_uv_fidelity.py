"""Offline fidelity regression: actual production math, no Blender/GitHub CI."""
import importlib.util
from pathlib import Path
import math
import unittest

CORE=Path(__file__).resolve().parents[1] / "blender" / "uv_texture_fidelity.py"
spec=importlib.util.spec_from_file_location("uv_texture_fidelity",CORE)
quality=importlib.util.module_from_spec(spec)
spec.loader.exec_module(quality)


def pixels(width,height,color):
    return list(color)*(width*height)


class UVFidelityRegression(unittest.TestCase):
    def test_linear_sample_uses_same_uv_values_for_identical_maps(self):
        image=pixels(8,8,(0.65,0.25,0.18,1.0))
        uv_pairs=[((0.2,0.3),(0.2,0.3)),((0.6,0.7),(0.6,0.7))]
        result=quality.compare_surface_maps(
            uv_pairs,image,(8,8),image,(8,8),"baseColor")
        self.assertEqual(result["sampleCount"],2)
        self.assertEqual(result["meanAbsoluteError"],0.)
        self.assertTrue(result["passed"])

    def test_large_color_shift_cannot_be_auto_accepted(self):
        original=pixels(8,8,(0.9,0.45,0.16,1.))
        candidate=pixels(8,8,(0.13,0.12,0.11,1.))
        uv_pairs=[((0.33,0.33),(0.33,0.33)) for _ in range(30)]
        result=quality.compare_surface_maps(
            uv_pairs,original,(8,8),candidate,(8,8),"baseColor")
        self.assertFalse(result["passed"])
        self.assertGreater(result["meanAbsoluteError"],.2)
        self.assertGreater(result["p95AbsoluteError"],.2)

    def test_roughness_and_metallic_are_checked_independently(self):
        scalar_good=pixels(4,4,(.8,.8,.8,1.))
        scalar_bad=pixels(4,4,(0.0,0.0,0.0,1.))
        uv=[((.25,.25),(.25,.25)) for _ in range(12)]
        a=quality.compare_surface_maps(
            uv,scalar_good,(4,4),scalar_bad,(4,4),"roughness")
        b=quality.compare_surface_maps(
            uv,scalar_good,(4,4),scalar_good,(4,4),"metallic")
        self.assertFalse(a["passed"])
        self.assertTrue(b["passed"])
        self.assertRaises(ValueError,quality.compare_surface_maps,
                          uv,scalar_good,(4,4),scalar_good,(4,4),"normal")

    def test_repeat_texture_uv_and_clamped_bake_uv(self):
        image=[
            1.,0.,0.,1., 0.,1.,0.,1.,
            0.,0.,1.,1., 1.,1.,1.,1.,
        ]
        a=quality.bilinear_rgba(image,2,2,.25,.25,repeat=True)
        self.assertEqual(a[:3],(1.,0.,0.))
        self.assertEqual(quality.bilinear_rgba(image,2,2,1.25,.25,True),
                         a)
        self.assertEqual(quality.bilinear_rgba(image,2,2,-.2,.25,False),
                         quality.bilinear_rgba(image,2,2,0.,.25,False))

    def test_uv_stretch_is_scale_invariant_and_detects_banding_risk(self):
        balanced=quality.triangle_uv_stretch([1.,1.,math.sqrt(2)],
                                              [2.,2.,2*math.sqrt(2)])
        self.assertAlmostEqual(balanced,1.,places=7)
        bad=quality.triangle_uv_stretch([1.,1.,1.],[.01,1.,1.])
        self.assertGreater(bad,50)
        self.assertIsNone(quality.triangle_uv_stretch([1.,1e-14,1.],[1.,1.,1.]))

    def test_reference_face_mapping_detects_uv_data_corruption_after_weld(self):
        from types import SimpleNamespace as NS
        def fake(uv, new_uv=None):
            return NS(
                attributes={quality.REFERENCE_FACE_ID:
                    NS(data=[NS(value=0)])},
                polygons=[NS(index=0,loop_indices=(0,1,2))],
                vertices=[
                    NS(co=(0.,0.,0.)),NS(co=(1.,0.,0.)),NS(co=(0.,1.,0.))],
                loops=[NS(vertex_index=i) for i in range(3)],
                uv_layers={"UVMap":NS(data=[NS(uv=xy) for xy in uv]),
                           **({"AI3D_Clean":NS(data=[NS(uv=xy) for xy in new_uv])}
                              if new_uv is not None else {})},
                loop_triangles=[NS(polygon_index=0,loops=(0,1,2))],
                calc_loop_triangles=lambda:None,
            )
        source=[(0.,0.),(1.,0.),(0.,1.)]
        new=[(.15,.15),(.85,.15),(.15,.85)]
        original=fake(source)
        target=fake(source,new)
        pairs,proof=quality.surface_uv_pairs_reference(
            original,target,"UVMap","AI3D_Clean")
        self.assertEqual(proof["matchedTriangleCount"],1)
        self.assertEqual(proof["originalCornerUvDriftCount"],0)
        self.assertTrue(proof["originalUvProvenanceIntact"])
        self.assertEqual(len(pairs),4)
        target.uv_layers["UVMap"].data[1].uv=(.7,0.)
        _,damaged=quality.surface_uv_pairs_reference(
            original,target,"UVMap","AI3D_Clean")
        self.assertEqual(damaged["originalCornerUvDriftCount"],1)
        self.assertFalse(damaged["originalUvProvenanceIntact"])

    def test_reference_rejects_hidden_triangle_movement(self):
        from types import SimpleNamespace as NS
        def fake(z, has_new=False):
            return NS(
                attributes={quality.REFERENCE_FACE_ID:NS(data=[NS(value=4)])},
                polygons=[NS(index=0,loop_indices=(0,1,2))],
                vertices=[NS(co=(0.,0.,0.)),NS(co=(1.,0.,0.)),
                          NS(co=(0.,1.,z))],
                loops=[NS(vertex_index=i) for i in range(3)],
                uv_layers={"UVMap":NS(data=[NS(uv=xy) for xy in
                            [(0.,0.),(1.,0.),(0.,1.)]]),
                           **({"AI3D_Clean":NS(data=[NS(uv=xy) for xy in
                            [(0.,0.),(1.,0.),(0.,1.)]])} if has_new else {})},
                loop_triangles=[NS(polygon_index=0,loops=(0,1,2))],
                calc_loop_triangles=lambda:None,
            )
        with self.assertRaisesRegex(ValueError,"moved source surface corner"):
            quality.surface_uv_pairs_reference(
                fake(0.),fake(.01,True),"UVMap","AI3D_Clean")

    def test_reported_fox_uv_distortion_fails_even_if_pbr_means_pass(self):
        # Exact geometry/UV values from the user's 2026-10-15 screenshot
        # reports. Four texture roles passed, yet tail texture is striped.
        original={"trianglesSampled":7993,"p95Anisotropy":1.24353,
                  "maximumAnisotropy":8.62372}
        rebuilt={"trianglesSampled":7993,"p95Anisotropy":3.67474,
                 "maximumAnisotropy":88.76794}
        result=quality.evaluate_uv_stretch(original,rebuilt)
        self.assertFalse(result["passed"])
        self.assertGreater(result["p95Ratio"],2.95)
        self.assertGreater(result["maximumRatio"],10.0)
        self.assertLess(result["allowedP95Anisotropy"],2.25)

    def test_large_local_pbr_failure_is_not_hidden_by_good_average(self):
        # 98% samples are perfect; only two concentrated surface samples
        # fail badly. Global mean and P95 alone would have accepted them.
        good=[((.25,.25),(.25,.25)) for _ in range(98)]
        bad=[((.25,.25),(.75,.75)) for _ in range(2)]
        pixels=[0.1,0.1,0.1,1.,0.1,0.1,0.1,1.,
                0.1,0.1,0.1,1.,0.9,0.9,0.9,1.]
        result=quality.compare_surface_maps(
            good+bad,pixels,(2,2),pixels,(2,2),"baseColor")
        self.assertLess(result["meanAbsoluteError"],0.065)
        self.assertLess(result["p95AbsoluteError"],0.18)
        self.assertGreater(result["p99AbsoluteError"],0.25)
        self.assertFalse(result["passed"])

    def test_good_uv_stretch_accepted(self):
        original={"trianglesSampled":102,"p95Anisotropy":1.25,
                  "maximumAnisotropy":6.3}
        rebuilt={"trianglesSampled":102,"p95Anisotropy":1.7,
                 "maximumAnisotropy":9.2}
        self.assertTrue(quality.evaluate_uv_stretch(original,rebuilt)["passed"])
        with self.assertRaisesRegex(ValueError,"identical source geometry"):
            quality.evaluate_uv_stretch(original,{**rebuilt,"trianglesSampled":103})

    def test_real_worker_has_unconditional_texture_gate(self):
        root=CORE.parents[1]
        worker=(root/"blender"/"repair_uv_pbr.py").read_text(encoding="utf-8")
        rust=(root/"studio"/"src-tauri"/"src"/"uv_repair.rs").read_text(encoding="utf-8")
        self.assertIn('stage("texture_fidelity")',worker)
        self.assertIn('surface_uv_pairs_reference(',worker)
        self.assertIn('originalUvProvenanceIntact',worker)
        self.assertIn('Texture_Quality.json',worker)
        self.assertIn('fn verify_texture_quality(',rust)
        self.assertIn('verify_texture_quality(&report,&folder)?',rust)
        self.assertIn('"qualityStatus": "passed" if passed else "needs-review"',worker)


if __name__=="__main__":
    unittest.main()
