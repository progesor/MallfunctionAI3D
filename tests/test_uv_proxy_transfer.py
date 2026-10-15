"""Run real UV proxy corner-transfer function against a fake Blender mesh.

The test compiles production functions from Blender source with AST and injects
minimal fake Blender data. No Blender/GPU/Cargo dependencies required.
"""
import ast
import math
from pathlib import Path
from types import SimpleNamespace as NS
import unittest

PATH=Path(__file__).resolve().parents[1]/"blender"/"uv_topology_blender.py"


class UVItem:
    def __init__(self,uv): self.uv=list(uv)


class UVLayer:
    def __init__(self,coords,name="UV"):
        self.data=[UVItem(p) for p in coords]
        self.active_render=False
        self.name=name


class UVLayers:
    def __init__(self, mapping):
        self.layers={k:UVLayer(v,k) for k,v in mapping.items()}
        self.active=None
    def get(self,key):return self.layers.get(key)
    def __getitem__(self,key):return self.layers[key]
    def __iter__(self):return iter(self.layers.values())
    def keys(self):return list(self.layers)
    def new(self,*,name):
        if name in self.layers:raise RuntimeError("Duplicate UV")
        self.layers[name]=UVLayer([(0,0)]*6,name)
        return self.layers[name]
    def remove(self,layer):
        for key,v in list(self.layers.items()):
            if v is layer:del self.layers[key]


class Mesh:
    def __init__(self,positions,uv,clean,change_normals_on_uv=False):
        self.vertices=[NS(co=v) for v in positions]
        faces=((0,1,2),(3,4,5))
        self.polygons=[NS(index=i,loop_indices=range(i*3,(i+1)*3)) for i in range(2)]
        self.loops=[NS(vertex_index=i) for i in range(6)]
        self.attributes={"FID":NS(data=[NS(value=0),NS(value=1)])}
        self.uv_layers=UVLayers({"UVMap":uv,**({"AI3D_Clean":clean} if clean else {})})
        self.corner_normals=[NS(vector=(0,0,1)) for _ in range(6)]
        self.original_vertices=len(positions)
    def set_synthetic_normals(self):
        self.corner_normals[0].vector=(0,0,-1)


def load_transfer():
    tree=ast.parse(PATH.read_text(encoding="utf-8"))
    needed={"_vec3","_normal_snapshot","_normal_drift",
            "transfer_clean_uv_from_proxy"}
    body=[x for x in tree.body if isinstance(x,ast.FunctionDef)
          and x.name in needed]
    if {x.name for x in body}!=needed:
        raise AssertionError("Production helper not found")
    scope={"math":math}
    exec(compile(ast.fix_missing_locations(ast.Module(body=body,type_ignores=[])),
                 str(PATH),"exec"),scope)
    return scope["transfer_clean_uv_from_proxy"]


class UVProxyTransferRegression(unittest.TestCase):
    def data(self):
        # Source repeats two geometric vertices at different indices,
        # with different authored UV coordinates at the old seam.
        verts=((0.,0.,0.),(1.,0.,0.),(0.,1.,0.),
               (0.,0.,0.),(0.,1.,0.),(-1.,0.,0.))
        src_uv=((0,0),(1,0),(0,1),(.2,.2),(.2,.9),(1,.4))
        new_uv=((.1,.1),(.9,.1),(.1,.9),
                (.1,.1),(.1,.9),(.3,.4))
        return Mesh(verts,src_uv,None),Mesh(verts,src_uv,new_uv)

    def test_transfer_new_uv_only_preserves_source_normals_and_uv0(self):
        source,proxy=self.data()
        before=[tuple(p.uv) for p in source.uv_layers["UVMap"].data]
        before_normal=[n.vector for n in source.corner_normals]
        fun=load_transfer()
        result=fun(NS(data=proxy),NS(data=source),"FID","AI3D_Clean")
        self.assertEqual(result["transferredCorners"],6)
        self.assertEqual(result["renderTrianglesPreserved"],2)
        self.assertEqual(result["maximumCornerNormalDelta"],0)
        self.assertEqual([tuple(i.uv) for i in source.uv_layers["UVMap"].data],
                         before)
        self.assertEqual([n.vector for n in source.corner_normals],before_normal)
        self.assertEqual(tuple(source.uv_layers["AI3D_Clean"].data[5].uv),(.3,.4))
        self.assertEqual(len(source.vertices),6)

    def test_changed_proxy_face_identity_is_rejected(self):
        source,proxy=self.data()
        proxy.attributes["FID"].data[1].value=999
        with self.assertRaisesRegex(RuntimeError,"Source face missing"):
            load_transfer()(NS(data=proxy),NS(data=source),"FID","AI3D_Clean")
        self.assertNotIn("AI3D_Clean",source.uv_layers.keys())

    def test_modified_source_face_position_is_rejected(self):
        source,proxy=self.data()
        proxy.vertices[3].co=(.1,0.,0.)
        with self.assertRaisesRegex(RuntimeError,"mapping ambiguous"):
            load_transfer()(NS(data=proxy),NS(data=source),"FID","AI3D_Clean")
        self.assertNotIn("AI3D_Clean",source.uv_layers.keys())


if __name__=="__main__":
    unittest.main()
