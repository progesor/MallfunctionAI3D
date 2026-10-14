import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'tools'))
from validate_glb import glb_manifest


class TestGlb(unittest.TestCase):
    def test_minimal_pbr_glb(self):
        obj={'asset':{'version':'2.0'},'accessors':[{'count':3},{'count':3}],
             'meshes':[{'primitives':[{'attributes':{'POSITION':0,'NORMAL':0,'TEXCOORD_0':0},'indices':1,'material':0}]}],
             'materials':[{'pbrMetallicRoughness':{'baseColorTexture':{'index':0},'roughnessFactor':1.0},
                           'normalTexture':{'index':0},'occlusionTexture':{'index':0}}],
             'textures':[{'source':0}], 'images':[{'bufferView':0,'mimeType':'image/png'}],
             'bufferViews':[{'buffer':0,'byteOffset':0,'byteLength':4}],
             'buffers':[{'byteLength':4}], 'nodes':[]}
        j=json.dumps(obj,separators=(',',':')).encode('utf8')
        j+=b' ' *((-len(j))%4)
        b=b'1234'
        data=struct.pack('<4sII',b'glTF',2,12+8+len(j)+8+len(b))+struct.pack('<I4s',len(j),b'JSON')+j+struct.pack('<I4s',len(b),b'BIN\0')+b
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'pbr.glb';p.write_bytes(data)
            report=glb_manifest(p)
            self.assertEqual(report['triangles_unique_meshes'],1)
            self.assertEqual(report['uv_primitive_count'],1)
            self.assertEqual(report['pbr_used_materials_with_ao_map'],1)
            self.assertEqual(report['image_count'],1)
            self.assertEqual(report['warnings'],[])

    def test_bad_file_fails(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'bad.glb';p.write_bytes(b'junk')
            with self.assertRaises(ValueError):glb_manifest(p)

if __name__=='__main__':unittest.main()
