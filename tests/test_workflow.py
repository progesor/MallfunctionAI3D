import copy
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
from prepare_workflows import (PILOT_CHANGES, ENGINE_CHANGES, EXPECTED_TYPES,
                               make_variant, validate_upstream, git_blob_sha1)


def fake_official():
    ids = dict(EXPECTED_TYPES)
    ids.update({x[0]: x[1] for x in PILOT_CHANGES})
    ids.update({x[0]: x[1] for x in ENGINE_CHANGES})
    ids.update({147: "BakeTextureFromVoxel", 261: "RenderUVAtlas", 314: "ComfySwitchNode",
                315: "ComfySwitchNode", 318: "ComfySwitchNode"})
    inputs = {id: {} for id in ids}
    for id, typ, k, i, v in PILOT_CHANGES:
        inputs[id][k] = (i, v)
    for id, typ, k, i in ENGINE_CHANGES:
        inputs[id][k] = (i, False)
    inputs[122] = {"image": (0, "viking_wolf_rune_axe.png")}
    inputs[322] = {"filename_prefix": (0, "3d/ComfyUI")}
    nodes = []
    for id, typ in ids.items():
        vals = inputs[id]
        length = 1 + max((i for i, _ in vals.values()), default=-1)
        indexed = [None] * length
        named = {}
        for k, (i, v) in vals.items(): indexed[i] = v; named[k] = v
        nodes.append({"id": id, "type": typ, "inputs": [], "outputs": [], "widgets_values": indexed,
                      "widgets_values_named": named, "properties": {}})
    edges = [(241,186,0),(196,224,0),(241,224,1),(196,233,0),(241,233,1),(288,196,1),(288,147,3),(285,322,0)]
    links = [[i+1,src,0,dst,slot,"TEST"] for i,(src,dst,slot) in enumerate(edges)]
    return {"id":"original", "revision": 15, "nodes":nodes,"links":links}


class TestOfficialClones(unittest.TestCase):
    def test_patches_both_engines_without_rewiring(self):
        original=fake_official(); snap=copy.deepcopy(original)
        validate_upstream(original)
        for name, state in (("pixal3d",False),("trellis2",True)):
            child,audit=make_variant(original,name,"PLAYBOX_FoxPlush_reference.png")
            self.assertEqual(child["links"], original["links"])
            self.assertNotEqual(child["id"], original["id"])
            self.assertEqual(len(child["nodes"]),len(original["nodes"]))
            byid={n["id"]:n for n in child["nodes"]}
            self.assertEqual(byid[316]["widgets_values_named"]["value"],state)
            self.assertEqual(byid[186]["widgets_values_named"]["target_face_count"],8000)
            self.assertEqual(byid[288]["widgets_values_named"]["value"],1024)
            self.assertTrue(len(audit)>=15)
        self.assertEqual(original,snap)

    def test_fail_closed_on_wrong_topology(self):
        w=fake_official();w["links"].pop()
        with self.assertRaisesRegex(ValueError,"topology"):
            validate_upstream(w)

    def test_git_blob_hash(self):
        import hashlib
        blob=b"abc\n"
        self.assertEqual(git_blob_sha1(blob),hashlib.sha1(b"blob 4\0abc\n").hexdigest())

if __name__=="__main__": unittest.main()
