"""Blender adapter for generalized exact-position weld without BMesh mutation.

Creates a BRAND NEW mesh rather than calling bmesh.ops.remove_doubles on
a GLB-imported mesh (previously failed with 'internal error setting the array'
in Blender 5.1.2). Transfers all original per-corner UVs, custom normals,
material indices and stable face IDs. No visible triangles are removed.
"""
import math

import bpy

from uv_topology import exact_topology_plan


def rebuild_exact_topology_work_copy(target, face_id_name):
    source = target.data
    if target.type != "MESH":
        raise ValueError("Only static mesh objects may be rebuilt")
    if source.shape_keys is not None or target.vertex_groups:
        raise ValueError("Skinned/shapekey mesh cannot be exact-welded safely")
    if len(source.color_attributes) > 0:
        raise ValueError("Vertex-color assets need a color-aware weld adapter")
    if not source.uv_layers:
        raise ValueError("Source mesh has no UV layers")
    if any(len(poly.vertices) != 3 for poly in source.polygons):
        raise ValueError("Exact weld currently requires pre-triangulated meshes")
    origin_attribute = source.attributes.get(face_id_name)
    if origin_attribute is None or origin_attribute.domain != "FACE" or \
       origin_attribute.data_type != "INT":
        raise ValueError("Source face identity attribute not available")
    original_uv_names = [str(layer.name) for layer in source.uv_layers]
    original_active = str(source.uv_layers.active.name)
    original_render = next(
        (str(layer.name) for layer in source.uv_layers if layer.active_render),
        original_active,
    )
    original_vertices = [tuple(float(v.co[k]) for k in range(3))
                         for v in source.vertices]
    original_faces = [tuple(int(i) for i in poly.vertices)
                      for poly in source.polygons]
    proposal = exact_topology_plan(original_vertices, original_faces)
    if not proposal["sourceCornerPositionsPreserved"]:
        raise RuntimeError("Exact-weld altered source face corner positions")
    if proposal["weldedVertices"] == 0:
        return {
            "requested": True, "applied": False, "weldedVertices": 0,
            "reason": "No exact duplicated XYZ vertices",
            "topology": {
                k: v for k, v in proposal.items()
                if k not in ("vertexPositions", "triangleIndices", "oldToUnique")
            },
        }

    # Build on an EMPTY mesh, so Blender never tries to set a resized data
    # array on a GLTF-owned or mutated BMesh with custom attributes.
    rebuilt = bpy.data.meshes.new(source.name + "_TopologyConnected")
    try:
        rebuilt.from_pydata(
            proposal["vertexPositions"], [],
            proposal["triangleIndices"],
        )
        rebuilt.update(calc_edges=True)
        if len(rebuilt.polygons) != len(source.polygons):
            raise RuntimeError("Exact topology rebuild changed polygon count")

        for material in source.materials:
            rebuilt.materials.append(material)
        source_face_id = [int(origin_attribute.data[i].value)
                          for i in range(len(source.polygons))]
        source_mat_index = [int(poly.material_index)
                            for poly in source.polygons]
        source_smooth = [bool(poly.use_smooth) for poly in source.polygons]
        for i, poly in enumerate(rebuilt.polygons):
            poly.material_index = source_mat_index[i]
            poly.use_smooth = source_smooth[i]

        # Copy EVERY UV layer per FACE CORNER (loop) to protect texture seams:
        # deduplicated 3D point positions alone are NOT sufficient for PBR.
        for old_name in original_uv_names:
            src_uv = source.uv_layers.get(old_name)
            dst_uv = rebuilt.uv_layers.new(name=old_name)
            if src_uv is None or dst_uv is None:
                raise RuntimeError("UV channel lost during topology rebuild")
            for poly_index, poly in enumerate(source.polygons):
                dst_poly = rebuilt.polygons[poly_index]
                for old_loop, new_loop in zip(poly.loop_indices,
                                               dst_poly.loop_indices):
                    src_data = src_uv.data[old_loop].uv
                    dst_uv.data[new_loop].uv = (
                        float(src_data[0]), float(src_data[1])
                    )
        rebuilt.uv_layers.active = rebuilt.uv_layers.get(original_active)
        rebuilt.uv_layers.get(original_render).active_render = True

        face_ids = rebuilt.attributes.new(name=face_id_name,
                                          type="INT", domain="FACE")
        for i, fid in enumerate(source_face_id):
            face_ids.data[i].value = fid

        # Preserve source NORMALS per loop. A single welded vertex can have
        # different split normals at hard edges or between authored pieces.
        # Blender 5.1 exposes Mesh.normals_split_custom_set.
        split = [tuple(float(v) for v in item.vector)
                 for item in source.corner_normals]
        if len(split) != len(rebuilt.loops):
            raise RuntimeError("Source per-corner normal count changed")
        if not hasattr(rebuilt, "normals_split_custom_set"):
            raise RuntimeError(
                "This Blender version lacks custom corner normal restoration"
            )
        rebuilt.normals_split_custom_set(split)
        rebuilt.update()
        restored = [tuple(float(v) for v in item.vector)
                    for item in rebuilt.corner_normals]
        max_normal_delta = max(
            math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
            for a,b in zip(split,restored)
        )
        if max_normal_delta > 0.02:
            raise RuntimeError(
                "Custom source corner normals could not be preserved"
            )

        # Independent verification of exact face-corner UV and positions,
        # source face identity, shading and triangle counts.
        for i, src_poly in enumerate(source.polygons):
            dst_poly = rebuilt.polygons[i]
            if source_face_id[i] != int(face_ids.data[i].value):
                raise RuntimeError("Stable source face ID changed")
            for old_loop, new_loop in zip(src_poly.loop_indices,
                                           dst_poly.loop_indices):
                old_vert = source.loops[old_loop].vertex_index
                new_vert = rebuilt.loops[new_loop].vertex_index
                if original_vertices[old_vert] != tuple(
                    float(c) for c in rebuilt.vertices[new_vert].co
                ):
                    raise RuntimeError("Original surface vertex moved")
                for name in original_uv_names:
                    old_uv = source.uv_layers[name].data[old_loop].uv
                    new_uv = rebuilt.uv_layers[name].data[new_loop].uv
                    if any(abs(float(old_uv[k])-float(new_uv[k])) > 1e-6
                           for k in (0,1)):
                        raise RuntimeError("Original UV face corner changed")
        if len(rebuilt.loop_triangles) != proposal["triangleCount"]:
            rebuilt.calc_loop_triangles()
        if len(rebuilt.loop_triangles) != proposal["triangleCount"]:
            raise RuntimeError("Triangle count changed during exact weld")

    except Exception:
        bpy.data.meshes.remove(rebuilt)
        raise

    target.data = rebuilt  # One atomic publish only AFTER all checks pass.
    if source.users == 0:
        bpy.data.meshes.remove(source)
    health = {
        k: v for k, v in proposal.items()
        if k not in ("vertexPositions", "triangleIndices", "oldToUnique")
    }
    return {
        "requested": True, "applied": True,
        "weldedVertices": proposal["weldedVertices"],
        "reason": "Exact position topology reconstructed with original UV + normals",
        "maximumCornerNormalDelta": round(max_normal_delta, 6),
        "topology": health,
    }
