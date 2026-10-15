"""Offline xatlas provider tests with fake native library (no pip install).

The real xatlas C++/Python wheel must be installed explicitly by a user
before any workstation Blender bake can use it.
"""
from pathlib import Path
from types import SimpleNamespace as NS
from unittest.mock import patch
import importlib.util
import sys
import unittest

ROOT=Path(__file__).resolve().parents[1]/"blender"
if str(ROOT) not in sys.path:
    sys.path.insert(0,str(ROOT))
from uv_provider_xatlas import apply_xatlas_uv

try:
    import numpy as np
except ImportError:
    np=None


class Layers:
    def __init__(self):
        self.layer=NS(data=[NS(uv=(-2.0,-2.0)) for _ in range(6)],
                      active_render=False)
        self.active=None
    def get(self,name):
        return self.layer if name=="AI3D_Clean" else None


class TriangleMesh:
    def __init__(self):
        self.vertices=[NS(co=(0.,0.,0.)),NS(co=(1.,0.,0.)),
                       NS(co=(0.,1.,0.)),NS(co=(1.,1.,0.))]
        self.polygons=[NS(vertices=(0,1,2),loop_indices=(0,1,2)),
                       NS(vertices=(1,3,2),loop_indices=(3,4,5))]
        self.loops=[NS(vertex_index=i) for i in (0,1,2,1,3,2)]
        self.uv_layers=Layers()


@unittest.skipIf(np is None,"Blender-bundled numpy / local numpy unavailable")
class XatlasProviderContract(unittest.TestCase):
    def test_identity_chart_mapping_restores_all_face_corner_uv(self):
        mesh=TriangleMesh()
        def parametrize(vertices,faces):
            # Stub exact xatlas Python API documented upstream.
            return (np.arange(len(vertices),dtype=np.uint32),
                    faces.copy(),vertices[:,0:2].copy())
        with patch.dict(sys.modules,{'xatlas':NS(parametrize=parametrize,
                                                   __version__='mock')}):
            result=apply_xatlas_uv(mesh,'AI3D_Clean')
        self.assertTrue(result["available"])
        self.assertEqual(result["triangles"],2)
        self.assertEqual(result["engine"],"xatlas")
        self.assertFalse(result["accepted"]) # Shared quality gate decides.
        self.assertEqual(tuple(mesh.uv_layers.layer.data[0].uv),(0.,0.))
        self.assertEqual(tuple(mesh.uv_layers.layer.data[3].uv),(1.,0.))

    def test_changed_face_order_fails_without_touching_uvs(self):
        mesh=TriangleMesh()
        def invalid(vertices,faces):
            return (np.arange(len(vertices),dtype=np.uint32),
                    faces[::-1].copy(),vertices[:,0:2].copy())
        with patch.dict(sys.modules,{'xatlas':NS(parametrize=invalid)}):
            with self.assertRaisesRegex(ValueError,"triangle identity/order"):
                apply_xatlas_uv(mesh,'AI3D_Clean')
        self.assertEqual(tuple(mesh.uv_layers.layer.data[0].uv),(-2.,-2.))

    def test_python_native_wheel_is_not_installed_automatically(self):
        code=(ROOT/"uv_provider_xatlas.py").read_text(encoding="utf-8")
        self.assertNotIn("pip install",code)
        self.assertNotIn("subprocess",code)
        self.assertIn("import xatlas",code)
        self.assertIn("set(mesh_vertices) != set(assigned_vertices)",code)


if __name__=="__main__":
    unittest.main()
