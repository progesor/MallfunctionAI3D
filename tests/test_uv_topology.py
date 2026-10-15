"""Offline topology contract tests (NO Blender, no GPU, no GitHub CI)."""
import sys
from pathlib import Path
import unittest
import importlib.util
import json
import struct
import tempfile

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

    def test_read_only_cli_handles_minimal_triangle_glb(self):
        tool=Path(__file__).resolve().parents[1]/"tools"/"inspect_glb_topology.py"
        spec=importlib.util.spec_from_file_location("topology_cli",tool)
        module=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        vertex=struct.pack("<9f",0.,0.,0.,1.,0.,0.,0.,1.,0.)
        indices=struct.pack("<3H",0,1,2)+b"\x00\x00"
        binary=vertex+indices
        doc={"asset":{"version":"2.0"},"buffers":[{"byteLength":len(binary)}],
             "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},
                            {"buffer":0,"byteOffset":36,"byteLength":6}],
             "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                          {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
             "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}]}
        j=json.dumps(doc,separators=(",",":")).encode("utf-8")
        j+=b" "*((-len(j))%4)
        total=12+8+len(j)+8+len(binary)
        glb=(b"glTF"+struct.pack("<II",2,total)+
             struct.pack("<I4s",len(j),b"JSON")+j+
             struct.pack("<I4s",len(binary),b"BIN\x00")+binary)
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"triangle.glb"
            path.write_bytes(glb)
            analysis=module.analyze_glb(path)
            self.assertEqual(analysis["primitives"][0]["triangleCount"],1)
            self.assertEqual(analysis["primitives"][0]["status"],"AUDIT_OK")

    def test_uv_proxy_does_not_rewrite_source_normals_or_old_uv(self):
        # The proxy is ONLY for UV charts; source render normals stay on
        # the original imported mesh and are never copied through Blender's
        # lossy normals_split_custom_set reconstruction.
        code=(B/"uv_topology_blender.py").read_text(encoding="utf-8")
        self.assertIn("bpy.data.meshes.new(",code)
        self.assertIn("proxy_mesh.from_pydata(",code)
        self.assertIn("for name in original_uv_names",code)
        self.assertIn("id_layer.data[i].value=fid",code)
        self.assertIn("def transfer_clean_uv_from_proxy(",code)
        self.assertIn("_normal_drift(source_normals,_normal_snapshot(rs))",code)
        self.assertIn("return proxy_obj,{",code)
        self.assertNotIn("target.data = rebuilt",code)
        self.assertNotIn("normals_split_custom_set(",code)
        self.assertNotIn("bmesh.ops.remove_doubles(",code)


if __name__=="__main__":
    unittest.main()
