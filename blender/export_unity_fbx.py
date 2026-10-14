"""Export three immutable Studio LOD GLBs to one Unity-importable FBX.

Blender 4.3+/5.x background worker, called only with absolute, validated paths.
Does not generate or modify LODs, nor touches the source GLBs.
Example:
 blender -b --python blender/export_unity_fbx.py -- \
  --lod0 a.glb --lod1 b.glb --lod2 c.glb --out Asset_LODs.fbx --report report.json
"""
import argparse
import json
import sys
from pathlib import Path

import bpy


def arguments():
    parser = argparse.ArgumentParser()
    for i in range(3):
        parser.add_argument(f"--lod{i}", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])


def main():
    args = arguments()
    sources = [getattr(args, f"lod{i}").resolve(strict=True) for i in range(3)]
    if any(path.suffix.lower() != ".glb" for path in sources):
        raise ValueError("LOD input must be a GLB")
    output = args.out.resolve()
    report = args.report.resolve()
    if output.suffix.lower() != ".fbx" or report.suffix.lower() != ".json":
        raise ValueError("Output and report extensions must be FBX and JSON")
    if output in sources or report in sources:
        raise ValueError("Worker must not overwrite source files")
    if output.exists() or report.exists():
        raise FileExistsError("Unity FBX outputs are immutable; choose a new version")
    if not output.parent.is_dir() or not report.parent.is_dir():
        raise ValueError("Output directories must already be created by Studio")

    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.context.scene.unit_settings.system = "NONE"
    lods = []
    export_objects = []
    for i, path in enumerate(sources):
        before = set(bpy.data.objects[:])
        bpy.ops.import_scene.gltf(filepath=str(path))
        objects = [ob for ob in bpy.context.scene.objects if ob not in before and ob.type == "MESH"]
        if not objects:
            raise ValueError(f"LOD{i} has no mesh")
        bpy.ops.object.select_all(action="DESELECT")
        for ob in objects:
            ob.select_set(True)
        bpy.context.view_layer.objects.active = objects[0]
        if len(objects) > 1:
            bpy.ops.object.join()
        mesh = bpy.context.view_layer.objects.active
        mesh.data.calc_loop_triangles()
        tris = len(mesh.data.loop_triangles)
        if tris == 0 or not mesh.data.uv_layers or not mesh.data.loops:
            raise ValueError(f"LOD{i} missing triangle geometry, UV0 or normals")
        if len(mesh.data.materials) > 1:
            raise ValueError(f"LOD{i} contains multiple materials")
        # Materials are built once in Unity, not serialized three times in FBX.
        mesh.data.materials.clear()
        parent = bpy.data.objects.new(f"LOD{i}", None)
        bpy.context.collection.objects.link(parent)
        original_world = mesh.matrix_world.copy()
        mesh.parent = parent
        mesh.matrix_world = original_world
        mesh.name = f"LOD{i}_Mesh"
        export_objects.extend([parent, mesh])
        lods.append({
            "name": f"LOD{i}", "triangles": tris, "vertices": len(mesh.data.vertices),
            "uvLayers": len(mesh.data.uv_layers),
            "dimensions": [round(float(d), 8) for d in mesh.dimensions],
            "worldMatrix": [[round(float(c), 8) for c in row] for row in mesh.matrix_world],
        })
    if not (lods[0]["triangles"] >= lods[1]["triangles"] >= lods[2]["triangles"] > 0):
        raise ValueError("LOD triangle counts must be descending")
    bpy.ops.object.select_all(action="DESELECT")
    for ob in export_objects:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = export_objects[0]
    # FBX native Unity import: -Z forward and Y up, no resampling or texture embedding.
    bpy.ops.export_scene.fbx(
        filepath=str(output), use_selection=True, object_types={"EMPTY", "MESH"},
        axis_forward="-Z", axis_up="Y", global_scale=1.0,
        apply_unit_scale=True, bake_space_transform=False,
        use_mesh_modifiers=False, use_tspace=True,
        add_leaf_bones=False, bake_anim=False,
        path_mode="AUTO", embed_textures=False,
    )
    if not output.is_file() or output.stat().st_size < 100:
        raise RuntimeError("Blender produced no valid FBX")
    report.write_text(json.dumps({
        "schemaVersion": 1, "format": "FBX", "axisUp": "Y", "axisForward": "-Z",
        "unit": "source GLB units (no guessed real height)",
        "materialSlots": 0, "lods": lods,
        "fbxBytes": output.stat().st_size, "sourceUntouched": True,
    }, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("MALLFUNCTION_UNITY_FBX:", output)


if __name__ == "__main__":
    main()
