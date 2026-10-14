#!/usr/bin/env python3
"""Clone a pinned Comfy-Org UI workflow with minimal, audited pilot changes.
No ComfyUI installation, model weights or user workflows are modified.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import re
import sys
import urllib.request
import uuid
from pathlib import Path

# As inspected on 2026-10-14: repository commit and Git blob SHA are deliberately pinned.
UPSTREAM_COMMIT = "3e89967349dd95fa68ce3b36966f41d642af8ea7"
UPSTREAM_BLOB_SHA1 = "d5fc9347a0d0f331f8da1556344fea5e7af14182"
UPSTREAM_URL = ("https://raw.githubusercontent.com/Comfy-Org/workflow_templates/"
                + UPSTREAM_COMMIT + "/templates/3d_pixal3d_trellis2_image_to_model.json")

# Node IDs are validated against official graph rather than matched by type alone.
# (node ID, expected type, widget name, positional index, pilot value)
PILOT_CHANGES = [
    (119, "VaeDecodeStructureTrellis2", "resolution", 0, "32"),
    (94, "Trellis2UpsampleStage", "target_resolution", 0, "1024"),
    (241, "RemeshMesh", "resolution", 0, 512),
    (186, "DecimateMesh", "target_face_count", 0, 8000),
    (196, "UnwrapMesh", "resolution", 1, 1024),
    (288, "PrimitiveInt", "value", 0, 1024),
    (147, "BakeTextureFromVoxel", "texture_size", 0, 1024),
    (224, "BakeNormalMapFromMesh", "resolution", 0, 1024),
    (233, "BakeAmbientOcclusion", "resolution", 0, 1024),
    (233, "BakeAmbientOcclusion", "samples", 1, 32),
    (261, "RenderUVAtlas", "resolution", 0, 1024),
]

ENGINE_CHANGES = [
    (316, "PrimitiveBoolean", "value", 0),
    (314, "ComfySwitchNode", "switch", 0),
    (315, "ComfySwitchNode", "switch", 0),
    (318, "ComfySwitchNode", "switch", 0),
]
EXPECTED_TYPES = {
    122: "LoadImage", 322: "Save3DAdvanced", 285: "MeshToFile3D",
    260: "MeshSmoothNormals", 210: "ApplyTextureToMesh", 196: "UnwrapMesh",
    241: "RemeshMesh", 186: "DecimateMesh", 224: "BakeNormalMapFromMesh",
    233: "BakeAmbientOcclusion", 288: "PrimitiveInt", 316: "PrimitiveBoolean",
    119: "VaeDecodeStructureTrellis2", 94: "Trellis2UpsampleStage",
}


def git_blob_sha1(blob: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(blob)).encode() + b"\0" + blob).hexdigest()


def patch_widget(node: dict, key: str, index: int, value):
    values = node.get("widgets_values")
    named = node.get("widgets_values_named")
    if not isinstance(values, list) or index >= len(values):
        raise ValueError(f"Node {node['id']} {node['type']}: invalid widget index {index}")
    if not isinstance(named, dict) or key not in named:
        raise ValueError(f"Node {node['id']} {node['type']}: missing widget key {key}")
    old = {"id": node["id"], "type": node["type"], "widget": key,
           "before": named[key], "after": value}
    values[index] = value
    named[key] = value
    return old


def require_node(nodes: dict, node_id: int, node_type: str) -> dict:
    n = nodes.get(node_id)
    if n is None or n.get("type") != node_type:
        raise ValueError(f"Official graph incompatible: node {node_id} must be {node_type}")
    return n


def assert_link(wf: dict, source_id: int, target_id: int, target_socket: int):
    if not any(int(link[1]) == source_id and int(link[3]) == target_id and
               int(link[4]) == target_socket for link in wf["links"]):
        raise ValueError(f"Official graph topology changed: expected {source_id} -> {target_id}:{target_socket}")


def validate_upstream(wf: dict):
    if not isinstance(wf.get("nodes"), list) or not isinstance(wf.get("links"), list):
        raise ValueError("Not a ComfyUI UI workflow JSON (nodes/links required)")
    nodes = {int(n["id"]): n for n in wf["nodes"]}
    if len(nodes) != len(wf["nodes"]):
        raise ValueError("Duplicate node IDs")
    for k, v in EXPECTED_TYPES.items():
        require_node(nodes, k, v)
    # Decimated mesh is the bake low_poly path; remesh stays high_poly.
    assert_link(wf, 241, 186, 0)
    assert_link(wf, 196, 224, 0)
    assert_link(wf, 241, 224, 1)
    assert_link(wf, 196, 233, 0)
    assert_link(wf, 241, 233, 1)
    # The same texture resolution primitive is wired into UV and texture baking.
    assert_link(wf, 288, 196, 1)
    assert_link(wf, 288, 147, 3)
    # Final model goes to Save3DAdvanced through mesh-to-file conversion.
    assert_link(wf, 285, 322, 0)
    return nodes


def make_variant(original: dict, engine: str, reference: str):
    if engine not in {"pixal3d", "trellis2"}:
        raise ValueError("Unknown engine")
    wf = copy.deepcopy(original)
    wf["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, str(original.get("id")) + "|mallfunction|" + engine))
    wf["revision"] = 0
    nodes = validate_upstream(wf)
    audit = []
    for node_id, node_type, key, ix, val in PILOT_CHANGES:
        audit.append(patch_widget(require_node(nodes, node_id, node_type), key, ix, val))
    switch_value = engine == "trellis2"
    for node_id, node_type, key, ix in ENGINE_CHANGES:
        audit.append(patch_widget(require_node(nodes, node_id, node_type), key, ix, switch_value))
    audit.append(patch_widget(nodes[122], "image", 0, reference))
    prefix = "3d/Mallfunction/PLAYBOX_FoxPlush_" + engine + "_pilot_v01"
    audit.append(patch_widget(nodes[322], "filename_prefix", 0, prefix))
    # As a deliberate safety invariant, patch ONLY widget values + revision/id.
    orig_by_id = {n["id"]: n for n in original["nodes"]}
    changed_keys = {"widgets_values", "widgets_values_named"}
    for node in wf["nodes"]:
        prev = orig_by_id[node["id"]]
        assert {k: v for k, v in node.items() if k not in changed_keys} == {k: v for k, v in prev.items() if k not in changed_keys}
    if original["links"] != wf["links"]:
        raise AssertionError("Graph links must be identical to original")
    return wf, audit


def load_official(path: Path, *, download: bool = False, allow_other_source: bool = False):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file():
        if not download:
            raise FileNotFoundError(f"Missing {path}; rerun with --download or supply --source")
        with urllib.request.urlopen(UPSTREAM_URL, timeout=45) as response:
            data = response.read(6_000_000)
        if len(data) >= 6_000_000:
            raise ValueError("Unexpectedly large upstream JSON")
        blob = git_blob_sha1(data)
        if blob != UPSTREAM_BLOB_SHA1:
            raise ValueError(f"Upstream SHA mismatch. Expected {UPSTREAM_BLOB_SHA1}, actual {blob}. Nothing saved.")
        path.write_bytes(data)
    data = path.read_bytes()
    blob = git_blob_sha1(data)
    if blob != UPSTREAM_BLOB_SHA1 and not allow_other_source:
        raise ValueError("Graph is not the pinned official version. SHA: " + blob
                         + ". For a deliberately changed source, pass --allow-other-source.")
    wf = json.loads(data.decode("utf-8-sig"))
    validate_upstream(wf)
    return wf, blob


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    root = Path(__file__).resolve().parent.parent
    ap.add_argument("--source", type=Path, default=root / "upstream" / "official_pixal3d_trellis2.json")
    ap.add_argument("--out", type=Path, default=root / "workflows")
    ap.add_argument("--reference", default="PLAYBOX_FoxPlush_reference.png",
                    help="Name AFTER uploading reference in ComfyUI LoadImage (not a local path)")
    ap.add_argument("--download", action="store_true", help="Download ONLY the official JSON; NEVER model weights")
    ap.add_argument("--allow-other-source", action="store_true", help="Use locally supplied compatible graph with another SHA")
    args = ap.parse_args(argv)
    if not re.fullmatch(r"[a-zA-Z0-9_().-]+\.(png|jpg|jpeg|webp)", args.reference, re.I):
        ap.error("--reference must be a plain image filename (not path), to be uploaded in LoadImage")
    try:
        official, blob = load_official(args.source, download=args.download,
                                      allow_other_source=args.allow_other_source)
        args.out.mkdir(parents=True, exist_ok=True)
        outputs = []
        for engine in ("pixal3d", "trellis2"):
            wf, changes = make_variant(official, engine, args.reference)
            out_file = args.out / f"Mallfunction_PLAYBOX_FoxPlush_{engine}_pilot_v01.json"
            if out_file.exists():
                raise FileExistsError(f"Pilot output exists, will not overwrite: {out_file}")
            outputs.append((out_file, wf, engine, changes))
        for p, wf, engine, changes in outputs:
            p.write_text(json.dumps(wf, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            manifest = {
                "project": "Mallfunction", "asset_id": "PLAYBOX_FoxPlush", "engine": engine,
                "status": "PREPARED_NOT_EXECUTED", "upstream_commit": UPSTREAM_COMMIT,
                "upstream_blob_sha1": blob, "reference_expected_in_comfy_input": args.reference,
                "workflow_file": p.name, "graph_nodes": len(wf["nodes"]),
                "graph_links": len(wf["links"]), "modifications": changes,
                "first_run": "Upload reference in LoadImage; run only one workflow; log GPU separately",
                "outputs_verified": False,
            }
            (p.with_suffix(".audit.json")).write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            print(f"PREPARED {engine}: {p}")
        print(f"Upstream verified: git blob sha1 = {blob}")
        print("ComfyUI, its model cache, and existing workflows: UNTOUCHED")
        return 0
    except (ValueError, FileNotFoundError, FileExistsError, OSError) as ex:
        print("ERROR:", ex, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
