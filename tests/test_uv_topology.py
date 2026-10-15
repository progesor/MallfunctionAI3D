"""Offline topology contract tests (NO Blender, no GPU, no GitHub CI)."""
import sys
from pathlib import Path
import unittest

B=Path(__file__).resolve().parents[1]/"blender"
if str(B) not in sys.path:
    sys.path.insert(0,str(B))
from uv_topology import exact_topology_plan


class GeneralTopologyTests(unittest.TestCase):
    def test_glb_uv_split_vertices_become_one_connected_surface(self):
        # Two triangles that share an EDGE in 3D but have different UV/normal
        # vertices on that edge. Original polygon order and face IDs persist.
        verts=[(0.,0.,0.),(1.,0.,0.),(0.,1.,0.),
               (0.,0.,0.),(0.,1.,0.),(-1.,0.,0.)]
        faces=[(0,1,2),(3,4,5)]
        result=exact_topology_plan(verts,faces)
        self.assertEqual(result["weldedVertices"],2)
        self.assertEqual(result["sourceConnectedComponents"],2)
        self.assertEqual(result["weldedConnectedComponents"],1)
        self.assertEqual(result["triangleCount"],2)
        self.assertTrue(result["sourceCornerPositionsPreserved"])
        self.assertEqual(result["sourceBoundaryEdges"],6)
        self.assertEqual(result["weldedBoundaryEdges"],4)
        self.assertEqual(result["triangleIndices"][0],(0,1,2))

    def test_exact_only_never_merges_distinct_close_points(self):
        verts=[(0.,0.,0.),(1.,0.,0.),(0.,1.,0.),
               (1e-7,0.,0.),(1.,1.,0.),(0.,1.0000001,0.)]
        result=exact_topology_plan(verts,[(0,1,2),(3,4,5)])
        self.assertEqual(result["weldedVertices"],0)
        self.assertEqual(result["weldedConnectedComponents"],2)

    def test_zero_area_after_merge_is_explicitly_rejected(self):
        verts=[(0.,0.,0.),(1.,0.,0.),(0.,0.,0.)]
        with self.assertRaisesRegex(ValueError,"collapse"):
            exact_topology_plan(verts,[(0,1,2)])

    def test_collinear_geometry_is_explicitly_rejected(self):
        with self.assertRaisesRegex(ValueError,"zero area"):
            exact_topology_plan([(0.,0.,0.),(1.,0.,0.),(2.,0.,0.)],
                                [(0,1,2)])

    def test_invalid_or_non_finite_indices_rejected(self):
        verts=[(0.,0.,0.),(1.,0.,0.),(0.,1.,0.)]
        with self.assertRaisesRegex(ValueError,"index"):
            exact_topology_plan(verts,[(0,1,5)])
        with self.assertRaisesRegex(ValueError,"position"):
            exact_topology_plan([(float('nan'),0,0)] + verts[1:],
                                [(0,1,2)])

    def test_adapter_does_not_mutate_original_uv_or_mesh(self):
        # General production adapter must use a new from_pydata mesh and
        # restore original corner UVs and custom split normals per loop.
        code=(B/"uv_topology_blender.py").read_text(encoding="utf-8")
        self.assertIn("bpy.data.meshes.new(",code)
        self.assertIn("rebuilt.from_pydata(",code)
        self.assertIn("normals_split_custom_set(",code)
        self.assertIn("for old_name in original_uv_names",code)
        self.assertIn("origin_attribute.data",code)
        self.assertIn("target.data = rebuilt",code)
        self.assertNotIn("bmesh.ops.remove_doubles(",code)


if __name__=="__main__":
    unittest.main()
