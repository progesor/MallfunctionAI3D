"""MallfunctionAI3D - automated topology-aware UV re-atlas + semantic PBR rebake.

This is an independent, versioned Blender 4.3+/5.x background worker.
It never changes the original GLB or the validated input maps.
- Exact-position seam welding is applied only to the WORKING copy, and
  discarded if triangle count/bounds change.
- Adaptive curvature seams + one meridian cut per closed connected component
  are unwrapped with angle-based Blender unwrap (Smart UV only as fallback).
- Source images sample EXPLICIT OLD UV, while each bake targets NEW UV.
- Metallic, Roughness, AO, BaseColor are baked separately with EMIT.
- Normal is baked as a tangent-space NORMAL pass, not copied across a
  different UV tangent basis.
- Geometry-only GLB and semantic map PNGs are assembled into a PBR GLB by Rust.
- Working .blend and UV SVG are kept for optional manual intervention.
"""
import argparse
import heapq
import json
import math
import sys
from pathlib import Path

import bpy
import bmesh
from mathutils import Vector

MAPS = ("baseColor", "normal", "metallic", "roughness", "occlusion")


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--source-maps", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--resolution", type=int, choices=(512, 1024, 2048), default=1024)
    parser.add_argument("--angle", type=int, default=62)
    parser.add_argument("--margin", type=int, default=24)
    parser.add_argument("--weld", type=int, choices=(0, 1), default=1)
    parser.add_argument("--preset", choices=("preserve", "plush"), default="preserve")
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])


def triangles(mesh):
    mesh.calc_loop_triangles()
    return len(mesh.loop_triangles)


def uv_island_count(mesh, name):
    """Count UV-connected face regions, not just disconnected mesh objects."""
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh)
        layer = bm.loops.layers.uv.get(name)
        if layer is None:
            raise RuntimeError("Missing UV layer: " + name)
        visited = set()
        count = 0
        for face in bm.faces:
            if face in visited:
                continue
            count += 1
            todo = [face]
            visited.add(face)
            while todo:
                current = todo.pop()
                for edge in current.edges:
                    if len(edge.link_faces) != 2:
                        continue
                    adjacent = edge.link_faces[0] if edge.link_faces[1] == current else edge.link_faces[1]
                    if adjacent in visited:
                        continue
                    # UV endpoints on both sides must match, independent of loop winding.
                    def coords(f):
                        return {loop.vert: tuple(round(v, 5) for v in loop[layer].uv)
                                for loop in f.loops if loop.vert in edge.verts}
                    left, right = coords(current), coords(adjacent)
                    if len(left) == 2 and left == right:
                        visited.add(adjacent)
                        todo.append(adjacent)
        return count
    finally:
        bm.free()


def attempt_safe_weld(mesh):
    """Rejoin exact GLB UV-split vertices on a temporary mesh only."""
    original = mesh.copy()
    before_triangles = triangles(mesh)
    before_bounds = [(min(v.co[k] for v in mesh.vertices),
                      max(v.co[k] for v in mesh.vertices)) for k in range(3)]
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh)
        before_verts = len(bm.verts)
        extents = [hi - lo for lo, hi in before_bounds]
        tolerance = max(1e-9, min(max(extents), 1000.0) * 1e-7)
        bmesh.ops.remove_doubles(bm, verts=list(bm.verts), dist=tolerance)
        bm.to_mesh(mesh)
        mesh.update()
    finally:
        bm.free()
    after_triangles = triangles(mesh)
    after_bounds = [(min(v.co[k] for v in mesh.vertices),
                     max(v.co[k] for v in mesh.vertices)) for k in range(3)]
    stable = after_triangles == before_triangles and all(
        abs(a - b) < 1e-6 for x, y in zip(before_bounds, after_bounds) for a, b in zip(x, y))
    if stable:
        bpy.data.meshes.remove(original)
        return before_verts - len(mesh.vertices), True
    # Do not apply a weld that changes triangles/bounds.
    mesh.from_pydata([tuple(v.co) for v in original.vertices], [],
                     [tuple(p.vertices) for p in original.polygons])
    # Restore *all* UV layers and attributes, not just shape, via the caller's copy.
    bpy.data.meshes.remove(original)
    raise RuntimeError("Temporary weld altered triangles/bounds and is unsafe")


def face_components(bm):
    visited = set()
    result = []
    for face in bm.faces:
        if face in visited:
            continue
        stack = [face]
        visited.add(face)
        component = []
        while stack:
            current = stack.pop()
            component.append(current)
            for edge in current.edges:
                for adjacent in edge.link_faces:
                    if adjacent not in visited:
                        visited.add(adjacent)
                        stack.append(adjacent)
        result.append(component)
    return result


def cut_closed_component(bm, component):
    """Open a smooth closed shell with a single rear meridian edge path."""
    verts = list({vert for face in component for vert in face.verts})
    if len(verts) < 12:
        return 0
    coords = [[float(v.co[axis]) for v in verts] for axis in range(3)]
    lo = [min(values) for values in coords]
    hi = [max(values) for values in coords]
    ranges = [hi[i] - lo[i] for i in range(3)]
    major = max(range(3), key=lambda i: ranges[i])
    back = 1 if major != 1 else 0
    center = (lo[back] + hi[back]) * .5
    start = min(verts, key=lambda v: v.co[major])
    goal = max(verts, key=lambda v: v.co[major])
    connected = set(verts)
    # Dijkstra path guided toward hidden/back-facing edges, not random cuts.
    q = [(0.0, start.index)]
    best = {start.index: 0.0}
    parent = {}
    vertex_by_index = {v.index: v for v in verts}
    while q:
        dist, key = heapq.heappop(q)
        if dist > best.get(key, float("inf")):
            continue
        if key == goal.index:
            break
        current = vertex_by_index[key]
        for edge in current.link_edges:
            other = edge.other_vert(current)
            if other not in connected:
                continue
            length = max(1e-8, edge.calc_length())
            # Back side penalty keeps the seam on an unobtrusive surface.
            position = max(0.0, float(other.co[back]) - center)
            weight = length * (1.0 + 6.0 * position / max(ranges[back], 1e-5))
            nxt = dist + weight
            if nxt < best.get(other.index, float("inf")):
                best[other.index] = nxt
                parent[other.index] = (key, edge)
                heapq.heappush(q, (nxt, other.index))
    if goal.index not in parent:
        return 0
    cursor = goal.index
    edges_marked = 0
    while cursor != start.index:
        cursor, edge = parent[cursor]
        if not edge.seam:
            edge.seam = True
            edges_marked += 1
    return edges_marked


def mark_curvature_seams(mesh, threshold_degrees):
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh)
        bm.verts.index_update()
        edges_marked = 0
        threshold = math.radians(threshold_degrees)
        for edge in bm.edges:
            edge.seam = not edge.is_manifold or edge.calc_face_angle(0.0) >= threshold
            if edge.seam:
                edges_marked += 1
        components = face_components(bm)
        for faces in components:
            edges = {edge for face in faces for edge in face.edges}
            if not any(edge.is_boundary or edge.seam for edge in edges):
                edges_marked += cut_closed_component(bm, faces)
        bm.to_mesh(mesh)
        mesh.update()
        return edges_marked, len(components)
    finally:
        bm.free()


def new_uv(target, angle, margin_px, resolution):
    mesh = target.data
    old_name = mesh.uv_layers.active.name
    old_count = uv_island_count(mesh, old_name)
    last_error = None
    # Favor coherent large surfaces. Relax seam threshold if fragments remain
    # excessive. This is geometry-aware, not semantic AI part labeling.
    for threshold in [angle, min(88, angle + 12), min(88, angle + 22)]:
        seams, components = mark_curvature_seams(mesh, threshold)
        layer = mesh.uv_layers.get("AI3D_Clean")
        if layer is None:
            layer = mesh.uv_layers.new(name="AI3D_Clean")
        mesh.uv_layers.active = layer
        layer.active_render = True
        bpy.ops.object.select_all(action="DESELECT")
        target.select_set(True)
        bpy.context.view_layer.objects.active = target
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        try:
            bpy.ops.uv.unwrap(method="ANGLE_BASED", margin_method="FRACTION",
                              margin=float(margin_px) / resolution)
            bpy.ops.uv.average_islands_scale()
            bpy.ops.uv.pack_islands(rotate=True, margin_method="FRACTION",
                                   margin=float(margin_px) / resolution)
        except Exception as e:
            last_error = str(e)
        finally:
            bpy.ops.object.mode_set(mode="OBJECT")
        islands = uv_island_count(mesh, layer.name)
        uv_values = [tuple(loop.uv) for loop in layer.data]
        valid = bool(uv_values) and all(math.isfinite(u) and math.isfinite(v) and
                                         -0.001 <= u <= 1.001 and -0.001 <= v <= 1.001
                                         for u, v in uv_values)
        if valid:
            extent_u = max(u for u, _ in uv_values) - min(u for u, _ in uv_values)
            extent_v = max(v for _, v in uv_values) - min(v for _, v in uv_values)
            valid = extent_u > .2 and extent_v > .2
        if valid and islands <= max(12, math.ceil(old_count * .9)):
            return old_name, layer.name, {
                "strategy": "curvature-seams-angle-unwrap",
                "seamAngleDeg": threshold, "seamEdges": seams,
                "connectedComponents": components, "oldUVIslands": old_count,
                "newUVIslands": islands,
            }
    # Fallback is explicit and reported, never mislabel Smart UV as semantic UV.
    bpy.ops.object.select_all(action="DESELECT")
    target.select_set(True)
    bpy.context.view_layer.objects.active = target
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    result = bpy.ops.uv.smart_project(angle_limit=math.radians(86),
                                      island_margin=float(margin_px) / resolution)
    bpy.ops.object.mode_set(mode="OBJECT")
    if "FINISHED" not in result:
        raise RuntimeError("UV Smart fallback also failed")
    points = [tuple(loop.uv) for loop in layer.data]
    if not points or max(u for u,v in points)-min(u for u,v in points)<.2 or \
       max(v for u,v in points)-min(v for u,v in points)<.2:
        raise RuntimeError("Generated UV atlas is degenerate; not creating repaired asset")
    return old_name, layer.name, {
        "strategy": "smart-project-fallback",
        "reason": last_error or "Adaptive unwrap failed validity/island-count checks",
        "oldUVIslands": old_count, "newUVIslands": uv_island_count(mesh, layer.name),
    }


def emit_uv_layout(mesh, layer_name, output):
    """Readable SVG UV ISLAND OUTLINES (not a dense 8k-triangle wireframe)."""
    bm = bmesh.new()
    pieces = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">',
              '<rect x="0" y="0" width="1024" height="1024" fill="#15212b"/>',
              '<rect x="0" y="0" width="1024" height="1024" fill="none" stroke="#627785" stroke-width="2"/>']
    try:
        bm.from_mesh(mesh)
        layer = bm.loops.layers.uv.get(layer_name)
        if layer is None:
            raise RuntimeError("UV layout missing layer " + layer_name)
        for edge in bm.edges:
            for face in edge.link_faces:
                # Only emit boundary or true UV seam edges, not all interior edges.
                is_boundary = len(edge.link_faces) != 2
                if not is_boundary:
                    other = next(f for f in edge.link_faces if f != face)
                    def verts_uv(f):
                        return {loop.vert: tuple(round(v, 5) for v in loop[layer].uv)
                                for loop in f.loops if loop.vert in edge.verts}
                    is_boundary = verts_uv(face) != verts_uv(other)
                if not is_boundary:
                    continue
                loops = [l for l in face.loops if l.vert in edge.verts]
                if len(loops) != 2:
                    continue
                a, b = (tuple(float(x) for x in loop[layer].uv) for loop in loops)
                pieces.append(
                    f'<line x1="{a[0]*1024:.2f}" y1="{(1-a[1])*1024:.2f}" '
                    f'x2="{b[0]*1024:.2f}" y2="{(1-b[1])*1024:.2f}" '
                    'stroke="#a1d4b5" stroke-width="1.4"/>')
    finally:
        bm.free()
    pieces.append("</svg>")
    output.write_text("\n".join(pieces), encoding="utf-8")


def preview_material(target, maps, uv_name, preset):
    """Keep a readable working Blender file; source images are still preserved."""
    mat = bpy.data.materials.new("AI3D_PBR_Clean_Preview")
    mat.use_nodes = True
    mat.node_tree.nodes.clear()
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (180, 0)
    output = nodes.new("ShaderNodeOutputMaterial")
    output.location = (550, 0)
    links.new(bsdf.outputs["BSDF"], output.inputs["Surface"])
    uv = nodes.new("ShaderNodeUVMap")
    uv.uv_map = uv_name
    uv.location = (-650, 0)
    for idx, role in enumerate(MAPS):
        image = maps.get(role)
        if image is None:
            continue
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = image
        tex.label = role
        tex.location = (-340, -idx * 220)
        links.new(uv.outputs["UV"], tex.inputs["Vector"])
        if role == "baseColor":
            links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
        elif role in ("metallic", "roughness"):
            if preset == "plush" and role == "metallic":
                bsdf.inputs["Metallic"].default_value = 0
            elif preset == "plush" and role == "roughness":
                bsdf.inputs["Roughness"].default_value = .82
            else:
                links.new(tex.outputs["Color"], bsdf.inputs[
                    "Metallic" if role == "metallic" else "Roughness"])
        elif role == "normal":
            normal = nodes.new("ShaderNodeNormalMap")
            normal.uv_map = uv_name
            normal.location = (-70, -idx * 220)
            links.new(tex.outputs["Color"], normal.inputs["Color"])
            links.new(normal.outputs["Normal"], bsdf.inputs["Normal"])
        # Blender lacks a standard Principled AO input. AO is retained as an
        # image and will be packed by Rust in the real exported PBR GLB.
    target.data.materials.clear()
    target.data.materials.append(mat)


def bake_image(target, role, source_image, old_uv, new_uv, output, resolution, margin):
    mesh = target.data
    mesh.uv_layers.active = mesh.uv_layers[new_uv]
    mesh.uv_layers[new_uv].active_render = True
    mat = bpy.data.materials.new(name="AI3D_Rebake_" + role)
    mat.use_nodes = True
    target.data.materials.clear()
    target.data.materials.append(mat)
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    uv = nodes.new("ShaderNodeUVMap")
    uv.uv_map = old_uv
    source = nodes.new("ShaderNodeTexImage")
    source.image = source_image
    source.interpolation = "Linear"
    links.new(uv.outputs["UV"], source.inputs["Vector"])
    emission = nodes.new("ShaderNodeEmission")
    links.new(source.outputs["Color"], emission.inputs["Color"])
    output_node = nodes.new("ShaderNodeOutputMaterial")
    links.new(emission.outputs["Emission"], output_node.inputs["Surface"])
    destination = nodes.new("ShaderNodeTexImage")
    img = bpy.data.images.new("AI3D_" + role, resolution, resolution, alpha=False)
    img.colorspace_settings.name = "sRGB" if role == "baseColor" else "Non-Color"
    destination.image = img
    nodes.active = destination
    destination.select = True
    bpy.context.scene.render.bake.margin = margin
    if role == "normal":
        normal = nodes.new("ShaderNodeNormalMap")
        normal.uv_map = old_uv
        links.new(source.outputs["Color"], normal.inputs["Color"])
        bsdf = nodes.new("ShaderNodeBsdfPrincipled")
        links.new(normal.outputs["Normal"], bsdf.inputs["Normal"])
        links.new(bsdf.outputs["BSDF"], output_node.inputs["Surface"])
        status = bpy.ops.object.bake(type="NORMAL", normal_space="TANGENT",
                                     use_selected_to_active=False)
    else:
        status = bpy.ops.object.bake(type="EMIT", use_selected_to_active=False)
    if "FINISHED" not in status:
        raise RuntimeError("Cycles bake did not finish: " + role)
    img.filepath_raw = str(output)
    img.file_format = "PNG"
    img.save()
    if not output.is_file() or output.stat().st_size < 60:
        raise RuntimeError("Bake did not write valid PNG: " + str(output))
    return img


def main():
    args = parse_args()
    if not 35 <= args.angle <= 88 or not 4 <= args.margin <= 48:
        raise ValueError("Angle 35..88 and margin 4..48 px required")
    source_path = args.input.resolve(strict=True)
    input_dir = args.source_maps.resolve(strict=True)
    output = args.output.resolve()
    if not output.is_dir():
        raise ValueError("Output version directory must already exist")
    if output in source_path.parents or source_path in output.parents:
        raise ValueError("Output cannot be inside a source model file")
    for role in MAPS:
        if not (input_dir / f"m0_{role}.png").is_file():
            raise FileNotFoundError(f"Validated map missing: m0_{role}.png")
    expected = [output / "geometry.glb", output / "UV_Layout.svg",
                output / "Repair_Report.json", output / "Repair_Working.blend"]
    if any(p.exists() for p in expected):
        raise FileExistsError("Existing UV repair version outputs cannot be overwritten")
    map_dir = output / "Textures"
    map_dir.mkdir(exist_ok=False)

    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    bpy.ops.import_scene.gltf(filepath=str(source_path))
    meshes = [ob for ob in bpy.context.scene.objects if ob.type == "MESH"]
    if not meshes:
        raise RuntimeError("No mesh in GLB")
    all_mats = {m for ob in meshes for m in ob.data.materials if m is not None}
    if len(all_mats) != 1:
        raise RuntimeError("UV repair currently supports exactly one PBR material")
    for obj in bpy.context.scene.objects:
        if obj.type in {"ARMATURE"}:
            raise RuntimeError("Skinned/armature models are not supported")
    # Join only a working copy. Original GLB on disk and its source objects remain.
    bpy.ops.object.select_all(action="DESELECT")
    for ob in meshes:
        ob.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.duplicate()
    work_meshes = [o for o in bpy.context.selected_objects if o.type == "MESH"]
    # Originals remain as hidden, unmodified references in the Blender checkpoint.
    for original_obj in meshes:
        original_obj.hide_set(True)
        original_obj.hide_render = True
    if len(work_meshes) > 1:
        bpy.context.view_layer.objects.active = work_meshes[0]
        bpy.ops.object.join()
    target = bpy.context.view_layer.objects.active
    target.name = "AI3D_Repaired_Target"
    target.data = target.data.copy()
    before_triangles = triangles(target.data)
    if before_triangles < 1 or not target.data.uv_layers:
        raise RuntimeError("UV repair requires triangles and existing UV0")
    welded = 0
    # Safe seam welding: keep a backup of the complete editable mesh datablock.
    if args.weld:
        backup = target.data.copy()
        try:
            welded, stable = attempt_safe_weld(target.data)
        except Exception:
            target.data = backup
            welded = 0
        else:
            if stable:
                bpy.data.meshes.remove(backup)
    old_uv, new_uv_name, uv_report = new_uv(target, args.angle, args.margin, args.resolution)
    scene = bpy.context.scene
    previous_engine = scene.render.engine
    previous_samples = getattr(scene.cycles, "samples", 1) if hasattr(scene, "cycles") else 1
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 2
    scene.render.bake.margin = args.margin
    saved_maps = {}
    baked_images = {}
    try:
        bpy.ops.object.select_all(action="DESELECT")
        target.select_set(True)
        bpy.context.view_layer.objects.active = target
        for role in MAPS:
            png = input_dir / f"m0_{role}.png"
            src_img = bpy.data.images.load(str(png), check_existing=False)
            src_img.colorspace_settings.name = "sRGB" if role == "baseColor" else "Non-Color"
            out_png = map_dir / f"{role}.png"
            baked_images[role] = bake_image(target, role, src_img, old_uv, new_uv_name,
                                           out_png, args.resolution, args.margin)
            saved_maps[role] = out_png.name
            print(f"AI3D_BAKE_MAP:{role}", flush=True)
        if triangles(target.data) != before_triangles:
            raise RuntimeError("Repair changed mesh triangle count")
        emit_uv_layout(target.data, new_uv_name, output / "UV_Layout.svg")
        # The new UV is first/active for glTF export. The old UV remains
        # available in the working blend for seam edits and comparisons.
        target.data.uv_layers.active = target.data.uv_layers[new_uv_name]
        target.data.uv_layers[new_uv_name].active_render = True
        # Remove the old UV layer from the export mesh only: glTF UV0 must be
        # our repaired atlas, while source UV stays in the saved .blend.
        preview_material(target, baked_images, new_uv_name, args.preset)
        bpy.ops.wm.save_as_mainfile(filepath=str(output / "Repair_Working.blend"))
        export_copy = target.data.copy()
        if old_uv in export_copy.uv_layers:
            export_copy.uv_layers.remove(export_copy.uv_layers[old_uv])
        export_copy.uv_layers.active = export_copy.uv_layers[new_uv_name]
        target.data = export_copy
        bpy.ops.object.select_all(action="DESELECT")
        target.select_set(True)
        bpy.context.view_layer.objects.active = target
        bpy.ops.export_scene.gltf(filepath=str(output / "geometry.glb"), export_format="GLB",
                                  use_selection=True, export_materials="NONE",
                                  export_apply=False, export_normals=True, export_texcoords=True)
        if not (output / "geometry.glb").is_file():
            raise RuntimeError("Blender did not export geometry.glb")
        report = {
            "schemaVersion": 1,
            "sourceUntouched": True,
            "strategy": uv_report["strategy"],
            "uv": uv_report,
            "trianglesBefore": before_triangles,
            "trianglesAfter": triangles(target.data),
            "verticesAfter": len(target.data.vertices),
            "weldedWorkingVertices": welded,
            "mapRoles": list(saved_maps.keys()),
            "resolution": args.resolution,
            "marginPixels": args.margin,
            "normalRebakedInTangentSpace": True,
            "profile": args.preset,
            "heightBaked": False,
            "manualEditBlend": "Repair_Working.blend",
            "uvLayout": "UV_Layout.svg",
            "warnings": ["Automatic seams use curvature/topology, not AI semantic limb labeling.",
                         "Inspect normal-map direction and tail/body texture seams in the 3D viewer."]
        }
        (output / "Repair_Report.json").write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print("AI3D_REPAIR_SUCCESS:" + str(output), flush=True)
    finally:
        scene.render.engine = previous_engine
        if hasattr(scene, "cycles"):
            scene.cycles.samples = previous_samples


if __name__ == "__main__":
    main()
