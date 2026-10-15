"""Blender 5.1 UV proxy adapter: exact topology for planning, original normals for PBR.

The source/render mesh is NEVER replaced. Some AI GLBs supply custom corner
normals that Blender cannot reconstruct on a welded mesh (especially when
their direction disagrees with imported face winding). Such normals do not
need to be transferred to a UV PLANNING proxy. The original GLB-derived
render mesh keeps its vertices, loop normals, old UV, material and winding.
Only the chosen NEW UV coordinates are copied BACK per stable face corner.
"""
import math
import bpy
from uv_topology import exact_topology_plan


def _vec3(value):
    return tuple(float(value[k]) for k in range(3))


def _normal_snapshot(mesh):
    return [_vec3(entry.vector) for entry in mesh.corner_normals]


def _normal_drift(before, after):
    if len(before) != len(after):
        raise RuntimeError("Source render corner normal count changed")
    return max((math.sqrt(sum((a[k]-b[k])**2 for k in range(3)))
                for a,b in zip(before,after)),default=0.0)


def make_topology_uv_proxy(render_obj, face_id_name):
    """Build a separate, UV-only mesh with connected XYZ topology.

    Geometry and original UV0 are validated. The proxy's Blender normals
    may be recomputed: they are deliberately NEVER exported or used to bake
    the PBR final model; source shading stays with render_obj.
    """
    source=render_obj.data
    if render_obj.type!="MESH" or source.shape_keys is not None or render_obj.vertex_groups:
        raise RuntimeError("UV proxy requires an unrigged static triangular mesh")
    if len(source.color_attributes)>0:
        raise RuntimeError("Vertex colors require a dedicated non-destructive adapter")
    if not source.uv_layers or not source.uv_layers.active:
        raise RuntimeError("Source mesh has no active UV channel")
    if any(len(poly.vertices)!=3 for poly in source.polygons):
        raise RuntimeError("UV proxy requires a triangulated static mesh")
    original_ids=source.attributes.get(face_id_name)
    if original_ids is None or original_ids.domain!="FACE" or \
       original_ids.data_type!="INT":
        raise RuntimeError("Stable face provenance ID missing")
    original_uv_names=[str(layer.name) for layer in source.uv_layers]
    original_active=str(source.uv_layers.active.name)
    original_render=next((str(layer.name) for layer in source.uv_layers
                          if layer.active_render), original_active)
    positions=[_vec3(vert.co) for vert in source.vertices]
    faces=[tuple(int(v) for v in face.vertices) for face in source.polygons]
    plan=exact_topology_plan(positions,faces)
    if not plan["sourceCornerPositionsPreserved"]:
        raise RuntimeError("UV proxy plan moved geometry")
    proxy_mesh=bpy.data.meshes.new(source.name+"_UV_Connected")
    proxy_obj=None
    try:
        proxy_mesh.from_pydata(plan["vertexPositions"],[],plan["triangleIndices"])
        proxy_mesh.update(calc_edges=True)
        if len(proxy_mesh.polygons)!=len(source.polygons):
            raise RuntimeError("UV proxy polygon count mismatch")
        for material in source.materials:
            proxy_mesh.materials.append(material)
        id_layer=proxy_mesh.attributes.new(
            name=face_id_name,type="INT",domain="FACE")
        ids=set()
        for i,(a,b) in enumerate(zip(source.polygons,proxy_mesh.polygons)):
            fid=int(original_ids.data[i].value)
            if fid in ids:
                raise RuntimeError("Original face ID repeated")
            ids.add(fid)
            id_layer.data[i].value=fid
            b.material_index=int(a.material_index)
            # Proxy shading isn't used for final PBR, but preserve the
            # original face flag for the .blend manual diagnostic.
            b.use_smooth=bool(a.use_smooth)
            if len(a.loop_indices)!=len(b.loop_indices):
                raise RuntimeError("UV proxy face corner count mismatch")
        for name in original_uv_names:
            source_uv=source.uv_layers.get(name)
            result_uv=proxy_mesh.uv_layers.new(name=name)
            for src_poly,dst_poly in zip(source.polygons,proxy_mesh.polygons):
                for source_loop,dst_loop in zip(src_poly.loop_indices,
                                                 dst_poly.loop_indices):
                    p=source_uv.data[source_loop].uv
                    result_uv.data[dst_loop].uv=(float(p[0]),float(p[1]))
        proxy_mesh.uv_layers.active=proxy_mesh.uv_layers.get(original_active)
        proxy_mesh.uv_layers.get(original_render).active_render=True

        # Compare original per-face/corner POSITION and UV0 on the two
        # independent meshes. These invariants are necessary for a valid
        # source-surface PBR reprojection.
        for i,face in enumerate(source.polygons):
            dest=proxy_mesh.polygons[i]
            for src_loop,dst_loop in zip(face.loop_indices,dest.loop_indices):
                source_vert=source.loops[src_loop].vertex_index
                proxy_vert=proxy_mesh.loops[dst_loop].vertex_index
                if positions[source_vert]!=_vec3(proxy_mesh.vertices[proxy_vert].co):
                    raise RuntimeError("UV proxy changed a render-mesh position")
                for name in original_uv_names:
                    a=source.uv_layers[name].data[src_loop].uv
                    b=proxy_mesh.uv_layers[name].data[dst_loop].uv
                    if any(abs(float(a[k])-float(b[k]))>1e-6 for k in (0,1)):
                        raise RuntimeError("UV proxy changed a source UV corner")
        proxy_obj=bpy.data.objects.new(render_obj.name+"_UV_Proxy",proxy_mesh)
        bpy.context.scene.collection.objects.link(proxy_obj)
        proxy_obj.matrix_world=render_obj.matrix_world.copy()
        proxy_obj.hide_render=True
    except Exception:
        if proxy_obj is not None:
            bpy.data.objects.remove(proxy_obj,do_unlink=True)
        if proxy_mesh.users==0:
            bpy.data.meshes.remove(proxy_mesh)
        raise
    topology={k:v for k,v in plan.items()
              if k not in ("vertexPositions","triangleIndices","oldToUnique")}
    return proxy_obj,{
        "requested":True,"applied":True,
        "weldedVertices":plan["weldedVertices"],
        "renderMeshUntouched":True,
        "originalCornerNormalsKeptOnRenderMesh":True,
        "proxyCornerNormalsUsedForPBR":False,
        "reason":"Exact-position connected UV proxy; source normal transfer intentionally unnecessary",
        "topology":topology,
    }


def transfer_clean_uv_from_proxy(proxy,render_obj,face_id_name,proxy_uv_name):
    """Transfer new UV atlas by face-ID and exact 3D corners ONLY.

    The actual PBR render object retains source mesh geometry and imported
    custom normals. UV is copied after all preflight checks, atomically as a
    separate layer. The original UV layer and all normal data are checked.
    """
    ps=proxy.data
    rs=render_obj.data
    pr=ps.attributes.get(face_id_name)
    rr=rs.attributes.get(face_id_name)
    if pr is None or rr is None:
        raise RuntimeError("Cannot copy atlas without stable source face IDs")
    if len(ps.polygons)!=len(rs.polygons):
        raise RuntimeError("UV proxy and render polygon counts differ")
    dst=ps.uv_layers.get(proxy_uv_name)
    if dst is None:
        raise RuntimeError("Accepted UV proxy layer missing")
    src_old={name:[
        (float(loop.uv[0]),float(loop.uv[1]))
        for loop in rs.uv_layers[name].data
    ] for name in rs.uv_layers.keys()}
    source_normals=_normal_snapshot(rs)
    source_loop_count=len(rs.loops)
    target_positions=[_vec3(v.co) for v in rs.vertices]
    atlas_by_loop={}
    face_lookup={}
    for poly in rs.polygons:
        key=int(rr.data[poly.index].value)
        if key in face_lookup:
            raise RuntimeError("Repeated render face provenance")
        face_lookup[key]=poly
    for poly in ps.polygons:
        pid=int(pr.data[poly.index].value)
        render=face_lookup.get(pid)
        if render is None:
            raise RuntimeError("Source face missing from UV proxy")
        mapped=set()
        for lp in poly.loop_indices:
            xyz=_vec3(ps.vertices[ps.loops[lp].vertex_index].co)
            matches=[
                index for index in render.loop_indices
                if index not in mapped and
                target_positions[rs.loops[index].vertex_index]==xyz
            ]
            if len(matches)!=1:
                raise RuntimeError("UV proxy corner-to-render mapping ambiguous")
            li=matches[0]
            mapped.add(li)
            atlas_by_loop[li]=(
                float(dst.data[lp].uv[0]),float(dst.data[lp].uv[1]))
    if len(atlas_by_loop)!=source_loop_count:
        raise RuntimeError("UV proxy lost render triangle corners")
    if any(not math.isfinite(x) for uv in atlas_by_loop.values() for x in uv):
        raise RuntimeError("UV proxy has invalid finite UVs")

    # Create only after geometry/source checks. No original layer replaced.
    layer=rs.uv_layers.new(name=proxy_uv_name)
    actual_name=str(layer.name)
    try:
        for loop,uv in atlas_by_loop.items():
            rs.uv_layers[actual_name].data[loop].uv=uv
        rs.uv_layers.active=rs.uv_layers.get(actual_name)
        rs.uv_layers[actual_name].active_render=True
        for name,baseline in src_old.items():
            now=rs.uv_layers.get(name)
            if now is None or any(
                abs(now.data[i].uv[k]-baseline[i][k])>1e-6
                for i in range(len(baseline)) for k in (0,1)):
                raise RuntimeError("Original render-mesh UV changed during atlas transfer")
        normal_change=_normal_drift(source_normals,_normal_snapshot(rs))
        if normal_change>0.002:
            raise RuntimeError(
                "Render normals changed during UV-only transfer: {:.5f}".format(
                    normal_change))
    except Exception:
        candidate=rs.uv_layers.get(actual_name)
        if candidate is not None:
            rs.uv_layers.remove(candidate)
        raise
    return {
        "renderUVLayer":actual_name,
        "transferredCorners":len(atlas_by_loop),
        "renderTrianglesPreserved":len(rs.polygons),
        "maximumCornerNormalDelta":round(normal_change,7),
        "originalRenderNormalsPreserved":True,
        "originalRenderUVsPreserved":True,
    }
