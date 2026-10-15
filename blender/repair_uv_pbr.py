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
from array import array
import heapq
import json
import math
import ntpath
import os
import shutil
import struct
import sys
import tempfile
import traceback
from pathlib import Path

import bpy
import bmesh
from mathutils import Vector
# Blender background --python does not always add the worker directory to
# sys.path. Resolve our pinned Blender-free QA module from next to this file.
_worker_root = str(Path(__file__).resolve().parent)
if _worker_root not in sys.path:
    sys.path.insert(0, _worker_root)
from uv_texture_fidelity import (
    ROLE_LIMITS, compare_surface_maps, mesh_uv_stretch,
    evaluate_uv_stretch, triangle_uv_stretch,
    REFERENCE_FACE_ID, surface_uv_pairs_reference,
)
from uv_planner import (
    evaluate_plan, select_bake_candidate, select_diagnostic_candidate,
)
from uv_topology_blender import (
    make_topology_uv_proxy,transfer_clean_uv_from_proxy,
)
from uv_provider_xatlas import apply_xatlas_uv

MAPS = ("baseColor", "normal", "metallic", "roughness", "occlusion")


def stage(label):
    print("AI3D_UV_STAGE:" + label, flush=True)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--source-maps", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--resolution", type=int, choices=(512, 1024, 2048), default=1024)
    parser.add_argument("--angle", type=int, default=62)
    parser.add_argument("--margin", type=int, default=24)
    parser.add_argument("--weld", type=int, choices=(0, 1), default=1)
    parser.add_argument("--safe-cleanup", type=int, choices=(0, 1), default=1)
    parser.add_argument("--preset", choices=("preserve", "plush"), default="preserve")
    parser.add_argument("--uv-engine", choices=("both","blender","xatlas"),default="both")
    parser.add_argument("--atlas-objective", choices=("auto","compact","fidelity"),default="auto")
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
            # Curvature seams can be isolated and may not actually open a
            # smooth closed volume (tail/cone). Add a hidden meridian on
            # *every manifold closed component*, even if it had sharp seams.
            if edges and all(edge.is_manifold for edge in edges):
                edges_marked += cut_closed_component(bm, faces)
        bm.to_mesh(mesh)
        mesh.update()
        return edges_marked, len(components)
    finally:
        bm.free()


def new_uv(target, angle, margin_px, resolution,
           engine_mode="both", atlas_objective="auto"):
    """Try multiple seam plans and choose lowest UV stretch, not island count."""
    mesh = target.data
    original = mesh.uv_layers.active
    if original is None:
        raise RuntimeError("Source mesh has no active UV map")
    old_name = str(original.name)
    old_count = uv_island_count(mesh, old_name)
    source_stretch = mesh_uv_stretch(mesh, old_name)
    new_layer = mesh.uv_layers.new(name="AI3D_Clean")
    clean_name = str(new_layer.name)
    attempted = []
    last_error = None

    def unwrap_threshold(threshold):
        # Blender can invalidate MeshUVLoopLayer references on BMesh writes or
        # Edit/Object mode switches. Always reacquire using a stable name.
        seams, components = mark_curvature_seams(target.data, threshold)
        live = target.data.uv_layers.get(clean_name)
        if live is None:
            raise RuntimeError("Clean UV layer vanished after BMesh update")
        target.data.uv_layers.active = live
        live.active_render = True
        bpy.ops.object.select_all(action="DESELECT")
        target.select_set(True)
        bpy.context.view_layer.objects.active = target
        bpy.ops.object.mode_set(mode="EDIT")
        try:
            bpy.ops.mesh.select_all(action="SELECT")
            result = bpy.ops.uv.unwrap(
                method="ANGLE_BASED", margin_method="FRACTION",
                margin=float(margin_px) / resolution,
            )
            if "FINISHED" not in result:
                raise RuntimeError("Blender angle-based unwrap failed")
            bpy.ops.uv.average_islands_scale()
            bpy.ops.uv.pack_islands(
                rotate=True, margin_method="FRACTION",
                margin=float(margin_px) / resolution,
            )
        finally:
            bpy.ops.object.mode_set(mode="OBJECT")
        live = target.data.uv_layers.get(clean_name)
        if live is None:
            raise RuntimeError("Clean UV layer missing after unwrap")
        uv_values = [(float(loop.uv[0]), float(loop.uv[1]))
                     for loop in live.data]
        count = uv_island_count(target.data, clean_name)
        valid = bool(uv_values) and all(
            math.isfinite(u) and math.isfinite(v) and
            -.001 <= u <= 1.001 and -.001 <= v <= 1.001
            for u, v in uv_values
        )
        if valid:
            valid = (max(u for u, _ in uv_values) - min(u for u, _ in uv_values) > .2
                     and max(v for _, v in uv_values) - min(v for _, v in uv_values) > .2)
        metrics = mesh_uv_stretch(target.data, clean_name)
        candidate = evaluate_plan(old_count, source_stretch, {
            "engine": "blender-angle",
            "seamAngleDeg": threshold,
            "seamEdges": seams,
            "connectedComponents": components,
            "newUVIslands": count,
            "p95Anisotropy": metrics.get("p95Anisotropy"),
            "maximumAnisotropy": metrics.get("maximumAnisotropy"),
            "degenerateUvTriangles": metrics.get("degenerateUvTriangles", 0),
            "rebuiltStretch": metrics,
            "validBounds": bool(valid),
        },objective=atlas_objective)
        return candidate

    angles = list(dict.fromkeys([
        angle, max(20, angle - 12), max(20, angle - 24),
        max(20, angle - 36), max(20, angle - 48),
    ]))
    provider_status = {"engine": "xatlas", "available": False,
                       "reason": "Optional module not installed"}
    xatlas_uv_snapshots = {}
    for threshold in (angles if engine_mode in ("both","blender") else []):
        try:
            candidate = unwrap_threshold(threshold)
        except Exception as exc:
            last_error = str(exc)
            attempted.append({
                "seamAngleDeg": threshold, "valid": False,
                "passesPreflight": False, "reasons": [last_error],
                "score": None,
            })
            continue
        attempted.append(candidate)
        if not candidate["passesPreflight"]:
            last_error = "UV preflight rejected {}°: {}".format(
                threshold, "; ".join(candidate["reasons"]))

    # CPU-only xatlas sweep: default baseline, moderate and broad chart
    # growth. The latter two tune upstream xatlas.ChartOptions.max_cost.
    # All candidates go through the same UV preflight/PBR gate.
    provider_trials=[]
    if engine_mode in ("both","xatlas"):
        for preset in ("default","joined","broad"):
            try:
                status=apply_xatlas_uv(
                    target.data,clean_name,preset=preset,resolution=resolution)
                provider_trials.append(status)
                if not status["available"]:
                    last_error=status.get("reason","xatlas unavailable")
                    break
                live=target.data.uv_layers.get(clean_name)
                snapshot=[(float(item.uv[0]),float(item.uv[1]))
                          for item in live.data]
                metrics=mesh_uv_stretch(target.data,clean_name)
                if snapshot:
                    uu=[u for u,_ in snapshot]
                    vv=[v for _,v in snapshot]
                    bounds=all(
                        math.isfinite(u) and math.isfinite(v) and
                        -.001<=u<=1.001 and -.001<=v<=1.001
                        for u,v in snapshot
                    ) and max(uu)-min(uu)>.2 and max(vv)-min(vv)>.2
                else:
                    bounds=False
                candidate=evaluate_plan(old_count,source_stretch,{
                    "engine":"xatlas",
                    "atlasPreset":preset,
                    "seamAngleDeg":None,
                    "seamEdges":None,
                    "connectedComponents":None,
                    "newUVIslands":uv_island_count(target.data,clean_name),
                    "rebuiltStretch":metrics,
                    "p95Anisotropy":metrics.get("p95Anisotropy"),
                    "maximumAnisotropy":metrics.get("maximumAnisotropy"),
                    "degenerateUvTriangles":metrics.get("degenerateUvTriangles",0),
                    "validBounds":bool(bounds),
                },objective=atlas_objective)
                attempted.append(candidate)
                xatlas_uv_snapshots[preset]=snapshot
            except Exception as exc:
                detail="xatlas {} candidate unavailable: {}".format(
                    preset,str(exc))
                last_error=detail
                attempted.append({
                    "engine":"xatlas","atlasPreset":preset,
                    "valid":False,"passesPreflight":False,
                    "reasons":[detail],"score":None,
                })
                provider_trials.append({
                    "engine":"xatlas","atlasPreset":preset,
                    "available":False,"reason":detail,
                })
    provider_status={
        "engine":"xatlas",
        "available":any(x.get("available",False) for x in provider_trials),
        "trials":provider_trials,
        "reason":last_error if not provider_trials else None,
    }

    # Never pick a candidate that is only 'best of bad' and then spend GPU
    # minutes on five Cycles bakes. Previous Smart UV fallback fragmented
    # the user's 262-island Fox into 1733 UV islands.
    winning = select_bake_candidate(attempted,atlas_objective)
    if winning is not None:
        if winning.get("engine") == "xatlas":
            snapshot=xatlas_uv_snapshots.get(winning["atlasPreset"])
            if snapshot is None:
                raise RuntimeError("xatlas winner UV data lost")
            live = target.data.uv_layers.get(clean_name)
            if len(live.data) != len(snapshot):
                raise RuntimeError("xatlas UV corner mapping changed")
            for i, (u, v) in enumerate(snapshot):
                live.data[i].uv = (u, v)
            target.data.uv_layers.active = live
            live.active_render = True
            winner = dict(winning)
            if uv_island_count(target.data, clean_name) != winner["newUVIslands"]:
                raise RuntimeError("xatlas UV chart count changed on activation")
            engine = "xatlas"
        else:
            # Reapply the chosen Blender threshold to restore selected seams
            # and UVs after trying other candidates.
            winner = unwrap_threshold(winning["seamAngleDeg"])
            if not winner["passesPreflight"]:
                raise RuntimeError("Accepted UV plan changed on reproduction")
            engine = "blender-angle"
        return old_name, clean_name, {
            "strategy": "multi-engine-preflight-accepted",
            "engine": engine,
            "accepted": True,
            "seamAngleDeg": winner.get("seamAngleDeg"),
            "seamEdges": winner.get("seamEdges"),
            "connectedComponents": winner.get("connectedComponents"),
            "oldUVIslands": old_count,
            "newUVIslands": winner["newUVIslands"],
            "sourceStretch": source_stretch,
            "rebuiltStretch": winner["rebuiltStretch"],
            "stretchGate": winner["stretchGate"],
            "providerStatus": provider_status,
            "engineMode":engine_mode,
            "triedPlans": attempted,
            "selectionReason": "compact-first-with-strict-fidelity-fallback",
            "atlasObjective":atlas_objective,
            "selectionTier":winner["selectionTier"],
            "islandPolicy":winner["islandPolicy"],
            "atlasPreset":winner.get("atlasPreset"),
        }

    # No Smart UV fallback: it can easily make editing harder than original.
    # Keep a DIAGNOSTIC atlas and report but don't start any texture bake.
    diagnostic = select_diagnostic_candidate(attempted, old_count)
    if diagnostic is not None:
        try:
            if diagnostic.get("engine") == "xatlas" and xatlas_uv_snapshots.get(diagnostic.get("atlasPreset")) is not None:
                live = target.data.uv_layers.get(clean_name)
                for i, coords in enumerate(xatlas_uv_snapshots[diagnostic["atlasPreset"]]):
                    live.data[i].uv = coords
            else:
                diagnostic = unwrap_threshold(diagnostic["seamAngleDeg"])
        except Exception as exc:
            last_error = str(exc)
    best_count = (diagnostic.get("newUVIslands")
                  if diagnostic is not None else old_count)
    return old_name, clean_name, {
        "strategy": "uv-preflight-rejected",
        "accepted": False,
        "atlasObjective":atlas_objective,
        "selectionTier":"rejected",
        "oldUVIslands": old_count,
        "newUVIslands": best_count,
        "sourceStretch": source_stretch,
        "rebuiltStretch": mesh_uv_stretch(target.data, clean_name),
        "triedPlans": attempted,
        "selectionReason": "no-plan-meets-island-and-distortion-constraints",
        "reason": last_error or
                  "No candidate both reduced islands and preserved UV stretch",
        "diagnosticOnly": True,
        "providerStatus": provider_status,
        "engineMode":engine_mode,
        "warning": "No Cycles bake; no Smart UV fallback; source unchanged",
    }



def emit_uv_stress_layout(mesh, layer_name, output):
    """SVG diagnostic with distorted triangles emphasized red/orange.

    Useful when a global PBR mean passes but the stylized tail is striped.
    This does NOT alter UV coordinates or source texture maps.
    """
    mesh.calc_loop_triangles()
    uv_layer=mesh.uv_layers[layer_name].data
    pieces=[
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">',
        '<rect x="0" y="0" width="1024" height="1024" fill="#14202b"/>',
    ]
    for tri in mesh.loop_triangles:
        loops=[int(i) for i in tri.loops]
        points=[(float(uv_layer[i].uv[0]),float(uv_layer[i].uv[1]))
                for i in loops]
        verts=[mesh.vertices[mesh.loops[i].vertex_index].co
               for i in loops]
        world=[float((verts[(i+1)%3]-verts[i]).length) for i in range(3)]
        tex=[math.hypot(points[(i+1)%3][0]-points[i][0],
                        points[(i+1)%3][1]-points[i][1]) for i in range(3)]
        anisotropy=triangle_uv_stretch(world,tex)
        if anisotropy is None:
            continue
        color=("#ee5268" if anisotropy>=6 else
               "#ff9b4a" if anisotropy>=3 else
               "#e1ca66" if anisotropy>=2 else "#376c72")
        polygon=" ".join(f"{u*1024:.2f},{(1-v)*1024:.2f}" for u,v in points)
        pieces.append(f'<polygon points="{polygon}" fill="{color}" '
                      'fill-opacity=".82" stroke="#182630" stroke-width=".24"/>')
    pieces.append('</svg>')
    output.write_text("\n".join(pieces),encoding="utf-8")


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


def blender_compatible_gltf_path(path, windows=None):
    r"""Reject paths the Blender 5.1 glTF addon cannot safely pass to makedirs.

    The Blender glTF add-on appends a slash to its file directory, breaking
    Windows extended-length paths such as \\?\C:\some\archive\version.
    Only local short native paths may be passed to that add-on. The source
    and archive themselves may still use extended-length paths for Python I/O.
    """
    if windows is None:
        windows = os.name == "nt"
    name = os.fspath(path)
    if windows:
        name = ntpath.normpath(name)
        if name.startswith("\\\\?\\UNC\\"):
            raise ValueError("Blender glTF staging requires a local drive, not UNC")
        if name.startswith("\\\\?\\"):
            name = name[4:]
        drive, _ = ntpath.splitdrive(name)
        if not drive or not ntpath.isabs(name) or name.startswith("\\\\"):
            raise ValueError("Blender glTF staging path must be on a local absolute drive")
        if len(name) >= 240:
            raise ValueError(
                "Blender glTF staging path exceeds safe native Windows path length"
            )
    else:
        name = os.path.abspath(name)
    return name


def stage_geometry_glb(destination, exporter):
    r"""Export in a short private temp folder, then copy to the versioned archive.

    Blender 5.1's glTF exporter fails with WinError 123 when its destination
    is prefixed with \\?\. The archive destination is intentionally unchanged,
    and copy uses Python file operations that support Windows long paths.
    """
    destination = Path(destination)
    if not destination.parent.is_dir():
        raise FileNotFoundError("UV Repair archive folder missing: " + str(destination.parent))
    if destination.exists():
        raise FileExistsError("Cannot overwrite existing repaired geometry: " + str(destination))
    with tempfile.TemporaryDirectory(prefix="ai3d_uv_glb_") as temp_dir:
        staging = blender_compatible_gltf_path(Path(temp_dir) / "geometry.glb")
        print("AI3D_GLTF_STAGING:" + staging, flush=True)
        result = exporter(
            filepath=staging,
            export_format="GLB",
            use_selection=True,
            export_materials="NONE",
            export_apply=False,
            export_normals=True,
            export_texcoords=True,
        )
        if "FINISHED" not in result:
            raise RuntimeError("Blender glTF exporter did not finish")
        staged = Path(staging)
        if not staged.is_file():
            raise RuntimeError("Blender glTF exporter wrote no geometry: " + staging)
        size = staged.stat().st_size
        with staged.open("rb") as source:
            header = source.read(12)
        if size < 28 or len(header) != 12 or header[:4] != b"glTF" or \
           struct.unpack_from("<I", header, 4)[0] != 2 or \
           struct.unpack_from("<I", header, 8)[0] != size:
            raise RuntimeError("Blender staged GLB header/length validation failed")
        # 'xb' prevents silent overwrite even if a file appeared after the
        # initial check. Only the unique, newly generated version is written.
        with staged.open("rb") as source, destination.open("xb") as target:
            shutil.copyfileobj(source, target, length=1024 * 1024)
        if destination.stat().st_size != size:
            raise RuntimeError("Blender GLB archive copy size mismatch")
    print("AI3D_GLTF_ARCHIVED:" + str(destination), flush=True)


def image_float_pixels(image):
    """Read Blender pixels once per image; 2K maps are ~64 MiB each."""
    if image is None or image.size[0] < 1 or image.size[1] < 1:
        raise ValueError("Texture image has no pixels")
    pixels = array("f", [0.0]) * (int(image.size[0]) * int(image.size[1]) * 4)
    image.pixels.foreach_get(pixels)
    return pixels


def mesh_health(mesh):
    """Read-only mesh diagnostics: possible holes and overlapping faces.

    A candidate is only a diagnosis. A seam loop, eye socket or layered
    fabric can be intentional; deleting/closing without review is unsafe.
    """
    bm = bmesh.new()
    try:
        bm.from_mesh(mesh)
        components = face_components(bm)
        boundary = [edge for edge in bm.edges if edge.is_boundary]
        nonmanifold = sum(1 for edge in bm.edges if not edge.is_manifold)
        loose = sum(1 for v in bm.verts if not v.link_faces)
        zero_faces = sum(1 for face in bm.faces if face.calc_area() <= 1e-16)

        def oriented_key(face):
            if len(face.verts) != 3:
                return None
            points = [tuple(float(v.co[i]) for i in range(3))
                      for v in face.verts]
            rotations = [tuple(points[i:] + points[:i]) for i in range(3)]
            return (face.material_index, min(rotations))
        # Even exact positional coincidence is ONLY a candidate. Separate
        # layered meshes can represent meaningful fur or shell geometry.
        seen_faces = set()
        exact_overlap = 0
        for face in bm.faces:
            key = oriented_key(face)
            if key is None:
                continue
            if key in seen_faces:
                exact_overlap += 1
            else:
                seen_faces.add(key)

        referenced = [vert for vert in bm.verts if vert.link_faces]
        if referenced:
            lo = [min(float(v.co[axis]) for v in referenced) for axis in range(3)]
            hi = [max(float(v.co[axis]) for v in referenced) for axis in range(3)]
            world_size = math.sqrt(sum((hi[i] - lo[i]) ** 2 for i in range(3)))
        else:
            world_size = 0.0
        boundary_set = set(boundary)
        visited = set()
        tiny_loops = 0
        for edge in boundary:
            if edge in visited:
                continue
            queue = [edge]
            visited.add(edge)
            part_edges = []
            part_verts = set()
            while queue:
                current = queue.pop()
                part_edges.append(current)
                for vert in current.verts:
                    part_verts.add(vert)
                    for adjacent in vert.link_edges:
                        if adjacent in boundary_set and adjacent not in visited:
                            visited.add(adjacent)
                            queue.append(adjacent)
            if not 3 <= len(part_edges) <= 8 or world_size <= 0:
                continue
            if any(sum(1 for e in vert.link_edges if e in boundary_set) != 2
                   for vert in part_verts):
                continue
            low = [min(float(v.co[i]) for v in part_verts) for i in range(3)]
            high = [max(float(v.co[i]) for v in part_verts) for i in range(3)]
            diameter = math.sqrt(sum((high[i] - low[i]) ** 2 for i in range(3)))
            if diameter <= world_size * 0.02:
                tiny_loops += 1
        return {
            "connectedComponents": len(components),
            "boundaryEdges": len(boundary),
            "nonManifoldEdges": nonmanifold,
            "looseVertices": loose,
            "zeroAreaFaces": zero_faces,
            "exactOverlappingFaceCandidates": exact_overlap,
            "tinyHoleLoopCandidates": tiny_loops,
            "faces": len(bm.faces),
            "vertices": len(bm.verts),
            "note": "Read-only overlap/hole candidates; no visible faces are "
                    "deleted and no holes are automatically filled.",
        }
    finally:
        bm.free()



def remove_orphan_geometry_only(target):
    """Remove loose vertices/wires, NEVER any vertex belonging to a polygon.

    Duplicate visible faces, self-intersections and small holes require
    separate user-driven repair. Rewriting those automatically can destroy
    the modeled fabric seams and break the original texture correspondence.
    """
    source_mesh = target.data
    candidate = source_mesh.copy()
    original_faces = triangles(source_mesh)
    bm = bmesh.new()
    removed = 0
    try:
        bm.from_mesh(candidate)
        orphaned = [vert for vert in bm.verts if not vert.link_faces]
        removed = len(orphaned)
        if orphaned:
            bmesh.ops.delete(bm, geom=orphaned, context="VERTS")
            bm.to_mesh(candidate)
            candidate.update()
    finally:
        bm.free()
    if not removed:
        bpy.data.meshes.remove(candidate)
        return {"applied": True, "removedLooseVertices": 0,
                "visibleTrianglesPreserved": True}
    if triangles(candidate) != original_faces:
        bpy.data.meshes.remove(candidate)
        raise RuntimeError(
            "Safe mesh cleanup changed visible triangle count; refusing")
    target.data = candidate
    bpy.data.meshes.remove(source_mesh)
    return {"applied": True, "removedLooseVertices": removed,
            "visibleTrianglesPreserved": True}


def sample_texture_fidelity(target, source_images, baked_images,
                            reference_mesh, old_uv, new_uv,
                            stable_surface_pairs, uv_provenance):
    """Surface-space texture comparison, independent of atlas island count.

    The original UV coordinates are read per triangle loop. Every map is
    sampled on exactly the same mesh surface points before and after rebake.
    Normal colors cannot be directly compared because tangent frames change;
    normal validation remains a separate visual/editor acceptance gate.
    """
    mesh = target.data
    pairs = stable_surface_pairs
    if not pairs or not uv_provenance["originalUvProvenanceIntact"]:
        raise RuntimeError("Texture fidelity source UV provenance was not retained")
    data = {}
    for role in ROLE_LIMITS:
        src = source_images[role]
        dst = baked_images[role]
        src_pixels = image_float_pixels(src)
        dst_pixels = image_float_pixels(dst)
        item = compare_surface_maps(
            pairs, src_pixels, (int(src.size[0]), int(src.size[1])),
            dst_pixels, (int(dst.size[0]), int(dst.size[1])), role,
        )
        data[role] = item
        # Large float buffers must not be retained for next texture.
        del src_pixels, dst_pixels
        print("AI3D_FIDELITY:{}:MAE={:.5f}:P95={:.5f}:{}".format(
            role, item["meanAbsoluteError"], item["p95AbsoluteError"],
            "PASS" if item["passed"] else "FAIL"), flush=True)
    stretch_old = mesh_uv_stretch(reference_mesh, old_uv)
    stretch_new = mesh_uv_stretch(mesh, new_uv)
    stretch_gate = evaluate_uv_stretch(stretch_old, stretch_new)
    passed = all(item["passed"] for item in data.values()) and stretch_gate["passed"]
    return {
        "schemaVersion": 2,
        "evaluated": True,
        "passed": passed,
        "method": "same-3d-surface-barycentric-UV-reprojection",
        "sampleCount": len(pairs),
        "maps": data,
        "uvStretch": {"source": stretch_old, "rebuilt": stretch_new},
        "uvStretchGate": stretch_gate,
        "sourceUvProvenance": uv_provenance,
        "normalValidation": "tangent-space-rebake; visual-check-required",
        "qualityStatus": "passed" if passed else "needs-review",
        "note": "M2.5.1: per-map MAE/P95/P99/severe hotspots AND "
                "UV P95/maximum distortion gate are mandatory. "
                "Tangents and tail appearance still need visual approval.",
    }


def main():
    args = parse_args()
    if not 35 <= args.angle <= 88 or not 4 <= args.margin <= 48:
        raise ValueError("Angle 35..88 and margin 4..48 px required")
    stage("validate_inputs")
    source_path = args.input.resolve(strict=True)
    input_dir = args.source_maps.resolve(strict=True)
    output = args.output.resolve()
    # App-managed, EXPLICITLY installed optional xatlas Python 3.13 plugin.
    # Blender 5.1 disables user-site packages, so only the app's
    # documented opt-in directory is added. No implicit downloads.
    library_root = output.parents[3] if len(output.parents) >= 4 else None
    os.environ.pop("AI3D_XATLAS_PLUGIN_PATH",None)
    if library_root is not None:
        plugin_dir = library_root / "plugins" / "xatlas"
        if plugin_dir.is_dir():
            plugin_native = str(plugin_dir)
            if plugin_native.startswith("\\\\?\\"):
                plugin_native = plugin_native[4:]
            if plugin_native not in sys.path:
                sys.path.insert(0, plugin_native)
            os.environ["AI3D_XATLAS_PLUGIN_PATH"]=plugin_native
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
    stage("source_import")
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
    stage("working_mesh_duplicate")
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
    # Stable polygon IDs are stamped BEFORE any welding, BMesh edit or UV
    # unwrap. The unmodified baseline mesh keeps original triangle-corner UVs
    # and supports a genuine source-vs-baked physical surface comparison.
    mesh=target.data
    if mesh.attributes.get(REFERENCE_FACE_ID) is not None:
        raise RuntimeError("Source mesh already uses reserved face provenance ID")
    attr=mesh.attributes.new(name=REFERENCE_FACE_ID, type="INT", domain="FACE")
    for face in mesh.polygons:
        attr.data[face.index].value=int(face.index)
    reference_mesh=mesh.copy()
    original_health = mesh_health(target.data)
    if before_triangles < 1 or not target.data.uv_layers:
        raise RuntimeError("UV repair requires triangles and existing UV0")
    cleanup_report = {"applied": False, "removedLooseVertices": 0,
                      "visibleTrianglesPreserved": True}
    if args.safe_cleanup:
        stage("safe_mesh_cleanup")
        cleanup_report = remove_orphan_geometry_only(target)
    welded = 0
    proxy = None
    weld_diagnostics = {
        "requested":bool(args.weld),"applied":False,
        "weldedVertices":0,"reason":"disabled",
        "renderMeshUntouched":True,
    }
    # Source GLB normals can disagree with polygon winding. Never replace
    # the actual PBR render mesh simply to create connected UV charts.
    # The proxy is used ONLY for UV planning, never for final PBR shading.
    if args.weld:
        stage("build_uv_topology_proxy")
        try:
            proxy,weld_diagnostics=make_topology_uv_proxy(target,REFERENCE_FACE_ID)
            welded=int(weld_diagnostics["weldedVertices"])
        except Exception as exc:
            weld_diagnostics["reason"]="uv-proxy-rejected: "+str(exc)
            failure={
                "schemaVersion":3,"sourceUntouched":True,
                "acceptedForBake":False,"weld":weld_diagnostics,
                "sourceTriangleCountInBlender":before_triangles,
                "sourceMeshHealth":original_health,
                "sourceFormat":"static-triangulated",
            }
            (output/"UV_Plan.json").write_text(
                json.dumps(failure,ensure_ascii=False,indent=2)+"\n",
                encoding="utf-8")
            print("AI3D_UV_PREFLIGHT:PROXY_REJECTED",flush=True)
            raise RuntimeError(
                "UV_PREFLIGHT_REJECTED: Connected UV planning proxy "
                "could not preserve source positions/UV face identities: {}. "
                "0/5 PBR maps baked. Source normals remain untouched.".format(
                    output/"UV_Plan.json"))
    uv_object=proxy if proxy is not None else target
    stage("auto_unwrap")
    old_uv,proxy_uv_name,uv_report=new_uv(
        uv_object,args.angle,args.margin,args.resolution,
        args.uv_engine,args.atlas_objective)
    uv_transfer={
        "sourceRenderNormalsKept":True,
        "renderMeshUntouched":True,
        "proxyNotUsedForPbr":True,
    }
    if proxy is not None and uv_report["accepted"]:
        stage("transfer_proxy_uv_to_render_mesh")
        uv_transfer=transfer_clean_uv_from_proxy(
            proxy,target,REFERENCE_FACE_ID,proxy_uv_name)
        new_uv_name=uv_transfer["renderUVLayer"]
        proxy.hide_set(True)
    else:
        new_uv_name=proxy_uv_name
    preflight = {
        "schemaVersion": 1, "sourceUntouched": True,
        "acceptedForBake": bool(uv_report["accepted"]),
        "uv": uv_report,
        "weld": weld_diagnostics,
        "engineMode": args.uv_engine,
        "atlasObjective":args.atlas_objective,
        "sourceTriangleCountInBlender": before_triangles,
        "sourceMeshHealth": original_health,
        "workingMeshHealth":mesh_health(uv_object.data),
        "renderMeshHealth":mesh_health(target.data),
        "safeCleanup":cleanup_report,
        "uvTransfer":uv_transfer,
    }
    (output / "UV_Plan.json").write_text(
        json.dumps(preflight, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8")
    print("AI3D_UV_PREFLIGHT:{}:{}->{}".format(
        "PASS" if uv_report["accepted"] else "REJECTED",
        uv_report["oldUVIslands"], uv_report["newUVIslands"]), flush=True)
    if not uv_report["accepted"]:
        stage("uv_preflight_rejected")
        emit_uv_layout(uv_object.data,proxy_uv_name,output/"UV_Layout.svg")
        emit_uv_stress_layout(uv_object.data,proxy_uv_name,
                              output/"UV_Stress.svg")
        bpy.ops.wm.save_as_mainfile(filepath=str(output / "UV_Plan_Working.blend"))
        raise RuntimeError(
            "UV_PREFLIGHT_REJECTED: No candidate reduces UV islands AND "
            "keeps source-relative UV stretch. Smart UV fallback disabled; "
            "0/5 PBR maps baked. Analysis: {}. Weld status: {}. "
            "Original GLB unchanged.".format(
                output / "UV_Plan.json", weld_diagnostics["reason"]))
    # Critical: checking the old UV layer on the welded mesh against itself
    # would miss UV corruption introduced by remove_doubles. Match every
    # original face/corner to the current UV by stamped identity+position.
    surface_pairs, uv_provenance = surface_uv_pairs_reference(
        reference_mesh,target.data,old_uv,new_uv_name)
    if not uv_provenance["originalUvProvenanceIntact"]:
        raise RuntimeError(
            "Weld/unwrap changed ORIGINAL triangle-corner UV mapping: "
            +str(uv_provenance)+". Disable weld and retry; source unchanged.")
    repaired_health = mesh_health(target.data)
    scene = bpy.context.scene
    previous_engine = scene.render.engine
    previous_samples = getattr(scene.cycles, "samples", 1) if hasattr(scene, "cycles") else 1
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 2
    scene.render.bake.margin = args.margin
    saved_maps = {}
    baked_images = {}
    saved_quality_images = {}
    source_images = {}
    try:
        bpy.ops.object.select_all(action="DESELECT")
        target.select_set(True)
        bpy.context.view_layer.objects.active = target
        for role in MAPS:
            stage("bake_" + role)
            png = input_dir / f"m0_{role}.png"
            src_img = bpy.data.images.load(str(png), check_existing=False)
            src_img.colorspace_settings.name = "sRGB" if role == "baseColor" else "Non-Color"
            source_images[role] = src_img
            out_png = map_dir / f"{role}.png"
            baked_images[role] = bake_image(target, role, src_img, old_uv, new_uv_name,
                                           out_png, args.resolution, args.margin)
            # Quality checks MUST sample the SAVED PNG, not the transient
            # in-memory Cycles bake buffer: encoding/color-space conversion
            # could otherwise pass a texture later exported differently.
            disk_image = bpy.data.images.load(str(out_png), check_existing=False)
            disk_image.colorspace_settings.name = (
                "sRGB" if role == "baseColor" else "Non-Color")
            saved_quality_images[role] = disk_image
            saved_maps[role] = out_png.name
            print(f"AI3D_BAKE_MAP:{role}", flush=True)
        if triangles(target.data) != before_triangles:
            raise RuntimeError("Repair changed mesh triangle count")
        stage("texture_fidelity")
        quality = sample_texture_fidelity(
            target, source_images, saved_quality_images,
            reference_mesh, old_uv, new_uv_name, surface_pairs, uv_provenance)
        islands_improved = (uv_report["newUVIslands"] < uv_report["oldUVIslands"])
        quality["uvAtlasSimplified"] = islands_improved
        atlas_policy=uv_report["islandPolicy"]
        quality["atlasPolicyPassed"] = bool(atlas_policy["passed"])
        quality["atlasSelectionTier"] = uv_report["selectionTier"]
        quality["atlasObjective"] = uv_report["atlasObjective"]
        quality["atlasPolicy"] = atlas_policy
        quality["passed"] = bool(quality["passed"] and quality["atlasPolicyPassed"])
        quality["qualityStatus"] = "passed" if quality["passed"] else "needs-review"
        (output / "Texture_Quality.json").write_text(
            json.dumps(quality, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8")
        emit_uv_stress_layout(uv_object.data,proxy_uv_name,
                              output/"UV_Stress.svg")
        emit_uv_layout(uv_object.data,proxy_uv_name,output/"UV_Layout.svg")
        # The new UV is first/active for glTF export. The old UV remains
        # available in the working blend for seam edits and comparisons.
        target.data.uv_layers.active = target.data.uv_layers[new_uv_name]
        target.data.uv_layers[new_uv_name].active_render = True
        # Remove the old UV layer from the export mesh only: glTF UV0 must be
        # our repaired atlas, while source UV stays in the saved .blend.
        stage("save_editable_blend")
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
        stage("export_uv_geometry")
        stage_geometry_glb(output / "geometry.glb", bpy.ops.export_scene.gltf)
        report = {
            "schemaVersion": 1,
            "sourceUntouched": True,
            "strategy": uv_report["strategy"],
            "uv": uv_report,
            "trianglesBefore": before_triangles,
            "trianglesAfter": triangles(target.data),
            "verticesAfter": len(target.data.vertices),
            "weldedWorkingVertices": welded,
            "weldDiagnostics": weld_diagnostics,
            "uvPreflight": preflight,
            "mapRoles": list(saved_maps.keys()),
            "resolution": args.resolution,
            "marginPixels": args.margin,
            "normalRebakedInTangentSpace": True,
            "textureFidelity": quality,
            "meshHealth": {"source": original_health,
                           "repairedWorking": repaired_health,
                           "topologyPlanningProxy":mesh_health(uv_object.data),
                           "safeCleanup": cleanup_report},
            "uvTransfer":uv_transfer,
            "profile": args.preset,
            "heightBaked": False,
            "manualEditBlend": "Repair_Working.blend",
            "uvLayout": "UV_Layout.svg",
            "uvStressLayout": "UV_Stress.svg",
            "warnings": ["Automatic seams use curvature/topology, not AI semantic limb labeling.",
                         "Inspect normal-map direction and tail/body texture seams in the 3D viewer."]
        }
        (output / "Repair_Report.json").write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        stage("complete")
        print("AI3D_REPAIR_SUCCESS:" + str(output), flush=True)
    finally:
        scene.render.engine = previous_engine
        if hasattr(scene, "cycles"):
            scene.cycles.samples = previous_samples


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        traceback.print_exc()
        print(f"AI3D_REPAIR_FAILED:{type(exc).__name__}: {exc}", flush=True)
        # Blender's --python-exit-code also catches unhandled failures;
        # sys.exit here covers launcher configurations without that flag.
        sys.exit(23)
