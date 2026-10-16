"""Blender-independent UV/PBR fidelity math used by the M2.5 Blender worker.

The same 3D mesh surface is sampled through original UV0 and repaired UV0.
A reduction in island count is not evidence of visual fidelity. This module
measures the actual texture transfer, independent of atlas packing and all
black pixels outside the UV islands.

No bpy import: deterministic offline unit tests can run without Blender.
"""
import math

BARYCENTRIC_SAMPLES = (
    (1/3, 1/3, 1/3),
    (0.60, 0.20, 0.20),
    (0.20, 0.60, 0.20),
    (0.20, 0.20, 0.60),
)

# Conservative pilot gates. A failed threshold never deletes the work or
# silently produces a "clean" library asset; only its quality status changes.
ROLE_LIMITS = {
    "baseColor": {"meanAbsoluteError": 0.065, "p95AbsoluteError": 0.180,
                  "p99AbsoluteError": 0.250, "severeErrorThreshold": 0.30,
                  "maxSevereSampleFraction": 0.010},
    "metallic": {"meanAbsoluteError": 0.075, "p95AbsoluteError": 0.210,
                 "p99AbsoluteError": 0.270, "severeErrorThreshold": 0.35,
                 "maxSevereSampleFraction": 0.015},
    "roughness": {"meanAbsoluteError": 0.075, "p95AbsoluteError": 0.210,
                  "p99AbsoluteError": 0.270, "severeErrorThreshold": 0.35,
                  "maxSevereSampleFraction": 0.015},
    "occlusion": {"meanAbsoluteError": 0.080, "p95AbsoluteError": 0.220,
                  "p99AbsoluteError": 0.280, "severeErrorThreshold": 0.35,
                  "maxSevereSampleFraction": 0.015},
}

# The clean Fox source is p95=1.24353, max=8.62372; rebuilt is p95=3.67474
# and max=88.76794. Four texture means passing MUST NOT override this loss.
STRETCH_LIMITS = {"maxP95Ratio": 1.80, "maxMaximumRatio": 3.0,
                  "minAbsoluteP95": 1.9, "minAbsoluteMaximum": 24.0}


def evaluate_uv_stretch(source, rebuilt):
    """Fail safely if a newly unwrapped atlas distorts geometry far more.

    The ratio uses source UV as its baseline; a poor original UV must not be
    used to claim arbitrary improvements, and a handful of zero-area edges
    cannot be silently ignored. The thresholds are versioned pilot policy.
    """
    required = ("p95Anisotropy", "maximumAnisotropy", "trianglesSampled")
    for item in (source, rebuilt):
        if any(item.get(k) is None for k in required):
            raise ValueError("UV stretch report missing metrics")
        if not (isinstance(item["trianglesSampled"],int) and item["trianglesSampled"]>0):
            raise ValueError("Invalid UV stretch triangle count")
        if any(not math.isfinite(float(item[k])) or float(item[k]) < 1.
               for k in required[:2]):
            raise ValueError("Non-finite or impossible UV anisotropy")
    if source["trianglesSampled"] != rebuilt["trianglesSampled"]:
        raise ValueError("UV stretch did not evaluate identical source geometry")
    p95_ratio = rebuilt["p95Anisotropy"] / source["p95Anisotropy"]
    max_ratio = rebuilt["maximumAnisotropy"] / source["maximumAnisotropy"]
    allowed_p95 = max(STRETCH_LIMITS["minAbsoluteP95"],
                      source["p95Anisotropy"] * STRETCH_LIMITS["maxP95Ratio"])
    allowed_max = max(STRETCH_LIMITS["minAbsoluteMaximum"],
                      source["maximumAnisotropy"] * STRETCH_LIMITS["maxMaximumRatio"])
    passed = (rebuilt["p95Anisotropy"]<=allowed_p95 and
              rebuilt["maximumAnisotropy"]<=allowed_max and
              rebuilt.get("degenerateUvTriangles",0)==0)
    return {
        "passed": passed, "p95Ratio": round(p95_ratio,6),
        "maximumRatio": round(max_ratio,6),
        "allowedP95Anisotropy": round(allowed_p95,5),
        "allowedMaximumAnisotropy": round(allowed_max,5),
        "limits": dict(STRETCH_LIMITS),
        "note": "This is a geometric UV distortion gate, separate from PBR "
                "pixel MAE. Large tail stretch is never rescued by good averages."
    }


def bilinear_rgba(buffer, width, height, u, v, repeat=True):
    """Sample bottom-row-first RGBA image.pixels with glTF-style repeat."""
    if width < 1 or height < 1 or not (math.isfinite(u) and math.isfinite(v)):
        raise ValueError("Invalid UV sample/dimensions")
    if repeat:
        u, v = u % 1.0, v % 1.0
    else:
        u, v = max(0., min(1., u)), max(0., min(1., v))
    x = u * width - 0.5
    y = v * height - 0.5
    ix, iy = math.floor(x), math.floor(y)
    dx, dy = x - ix, y - iy
    def fetch(px, py):
        if repeat:
            px, py = px % width, py % height
        else:
            px = max(0, min(width - 1, px))
            py = max(0, min(height - 1, py))
        offset = (py * width + px) * 4
        return [float(buffer[offset + c]) for c in range(4)]
    a = fetch(ix, iy)
    b = fetch(ix + 1, iy)
    c = fetch(ix, iy + 1)
    d = fetch(ix + 1, iy + 1)
    return tuple(
        (a[i] * (1-dx) + b[i] * dx) * (1-dy)
        + (c[i] * (1-dx) + d[i] * dx) * dy
        for i in range(4)
    )


def surface_uv_pairs(mesh, old_uv_name, new_uv_name):
    """Yield identical triangle surface locations in old and new UV space.

    Blender MeshLoopTriangle.loops stores corner *loop indices*; reading from
    UV layer.data through those loop indices retains per-face UV seams even
    after vertex welding. UVs are never guessed from vertex coordinates.
    """
    old = mesh.uv_layers[old_uv_name].data
    new = mesh.uv_layers[new_uv_name].data
    mesh.calc_loop_triangles()
    for tri in mesh.loop_triangles:
        corners = tuple(int(i) for i in tri.loops)
        if len(corners) != 3:
            raise ValueError("Expected triangulated mesh loop")
        a = [tuple(float(c) for c in old[i].uv) for i in corners]
        b = [tuple(float(c) for c in new[i].uv) for i in corners]
        for bary in BARYCENTRIC_SAMPLES:
            yield (
                (sum(a[k][0] * bary[k] for k in range(3)),
                 sum(a[k][1] * bary[k] for k in range(3))),
                (sum(b[k][0] * bary[k] for k in range(3)),
                 sum(b[k][1] * bary[k] for k in range(3))),
            )


def compare_surface_maps(uv_pairs, source_pixels, source_size,
                         baked_pixels, baked_size, role):
    """Compare source and baked scalar/color maps at the SAME model points."""
    if role not in ROLE_LIMITS:
        raise ValueError("Only comparable PBR data roles may be measured")
    channels = 3 if role == "baseColor" else 1
    errors = []
    for source_uv, baked_uv in uv_pairs:
        source = bilinear_rgba(
            source_pixels, source_size[0], source_size[1], *source_uv, repeat=True)
        baked = bilinear_rgba(
            baked_pixels, baked_size[0], baked_size[1], *baked_uv, repeat=False)
        err = sum(abs(source[c] - baked[c]) for c in range(channels)) / channels
        if not math.isfinite(err):
            raise ValueError("Non-finite fidelity error")
        errors.append(err)
    if not errors:
        raise ValueError("UV mesh has no triangle samples")
    errors.sort()
    count = len(errors)
    mean = sum(errors) / count
    p95 = errors[min(count - 1, math.ceil(count * .95) - 1)]
    p99 = errors[min(count - 1, math.ceil(count * .99) - 1)]
    limit = ROLE_LIMITS[role]
    severe_fraction = sum(e > limit["severeErrorThreshold"] for e in errors) / count
    passed = (mean <= limit["meanAbsoluteError"]
              and p95 <= limit["p95AbsoluteError"]
              and p99 <= limit["p99AbsoluteError"]
              and severe_fraction <= limit["maxSevereSampleFraction"])
    return {
        "role": role,
        "sampleCount": count,
        "meanAbsoluteError": round(mean, 6),
        "p95AbsoluteError": round(p95, 6),
        "p99AbsoluteError": round(p99, 6),
        "severeSampleFraction": round(severe_fraction, 6),
        "maximumError": round(errors[-1], 6),
        "limits": dict(limit),
        "passed": passed,
    }


def triangle_uv_stretch(edge3, edgeuv):
    """Scale-independent per-face triangle edge distortion estimate.

    Source and target have different atlas scales; compare anisotropy (ratio
    of edge UV/world lengths), not absolute texel density. Near-zero geometry
    or degenerate UV edges are flagged, never divided by zero.
    """
    if len(edge3) != 3 or len(edgeuv) != 3:
        raise ValueError("Three triangle edge lengths required")
    ratios = []
    for world, uv in zip(edge3, edgeuv):
        if world <= 1e-12:
            return None   # ignore physically degenerate import triangles
        if uv <= 1e-12:
            return float("inf")
        ratios.append(uv / world)
    return max(ratios) / min(ratios)


def mesh_uv_stretch(mesh, uv_name):
    """Report UV stretch p95; diagnostics only until real-Fox calibration."""
    uv = mesh.uv_layers[uv_name].data
    mesh.calc_loop_triangles()
    vals = []
    degenerate_uv = 0
    for tri in mesh.loop_triangles:
        corners = [int(i) for i in tri.loops]
        verts = [mesh.vertices[mesh.loops[idx].vertex_index].co for idx in corners]
        coords = [uv[idx].uv for idx in corners]
        edge3 = [float((verts[(i+1) % 3] - verts[i]).length) for i in range(3)]
        edgeuv = [math.hypot(float(coords[(i+1) % 3][0] - coords[i][0]),
                             float(coords[(i+1) % 3][1] - coords[i][1]))
                  for i in range(3)]
        score = triangle_uv_stretch(edge3, edgeuv)
        if score is None:
            continue  # degenerate imported WORLD-space triangle
        if not math.isfinite(score):
            degenerate_uv += 1
        else:
            vals.append(score)
    if not vals:
        return {"trianglesSampled": 0, "p95Anisotropy": None,
                "maximumAnisotropy": None,
                "degenerateUvTriangles": degenerate_uv}
    vals.sort()
    return {"trianglesSampled": len(vals) + degenerate_uv,
            "p95Anisotropy": round(vals[min(len(vals)-1,
                                          math.ceil(.95 * len(vals))-1)], 5),
            "maximumAnisotropy": round(vals[-1], 5),
            "degenerateUvTriangles": degenerate_uv}


REFERENCE_FACE_ID = "AI3D_REPAIR_SOURCE_FACE_ID"


def surface_uv_pairs_reference(reference_mesh, target_mesh,
                               original_uv_name, rebuilt_uv_name):
    """Match original per-face UVs to target UVs *across Blender weld*.

    Blender BMesh can merge vertices and reorder polygon indices while
    preserving polygon-corner UVs. We therefore stamp a temporary integer
    face ID BEFORE any mesh repair and map by face ID and 3D corner positions.
    If mapping/UV origin is invalidated, fail closed before publishing.
    This is strictly stronger than comparing two layers on the welded mesh.
    """
    reference_attr = reference_mesh.attributes.get(REFERENCE_FACE_ID)
    target_attr = target_mesh.attributes.get(REFERENCE_FACE_ID)
    if reference_attr is None or target_attr is None:
        raise ValueError("Repair face IDs missing after BMesh operation")
    target_mesh.calc_loop_triangles()
    original_uv = reference_mesh.uv_layers[original_uv_name].data
    working_old = target_mesh.uv_layers[original_uv_name].data
    rebuilt_uv = target_mesh.uv_layers[rebuilt_uv_name].data
    if len(reference_mesh.polygons) != len(target_mesh.polygons):
        raise ValueError("Topology face count changed during UV repair")
    source_faces = {}
    for polygon in reference_mesh.polygons:
        pid=int(reference_attr.data[polygon.index].value)
        if pid in source_faces:
            raise ValueError("Duplicate reference polygon ID")
        loops=list(polygon.loop_indices)
        if len(loops)!=3:
            raise ValueError("M2.5 needs triangle faces for source UV mapping")
        source_faces[pid]=[
            (tuple(float(c) for c in reference_mesh.vertices[
                reference_mesh.loops[idx].vertex_index].co),
             tuple(float(c) for c in original_uv[idx].uv))
            for idx in loops
        ]
    if len(source_faces)!=len(reference_mesh.polygons):
        raise ValueError("Missing unique reference polygon ID")

    world_points=[
        tuple(float(c) for c in vert.co)
        for vert in reference_mesh.vertices
    ]
    if not world_points:
        raise ValueError("Reference mesh has no vertices")
    size=math.sqrt(sum(
        (max(pos[k] for pos in world_points)-min(pos[k] for pos in world_points))**2
        for k in range(3)))
    tolerance=max(1e-9,size*2e-6)
    drift_count=0
    max_drift=0.0
    pairs=[]
    seen=set()
    for triangle in target_mesh.loop_triangles:
        pid=int(target_attr.data[triangle.polygon_index].value)
        if pid not in source_faces or pid in seen:
            raise ValueError("Source face ID lost, repeated or mismatched")
        seen.add(pid)
        ref=source_faces[pid]
        found=set()
        old_uvs=[]
        new_uvs=[]
        for loop_index in triangle.loops:
            loop_index=int(loop_index)
            current_pos=tuple(float(c) for c in target_mesh.vertices[
                target_mesh.loops[loop_index].vertex_index].co)
            # Exactly 3 points, so exhaustive closest unused correspondence
            # is cheap and allows cyclic winding/vertex index permutations.
            distances=[
                (math.sqrt(sum((current_pos[k]-pos[k])**2 for k in range(3))),i)
                for i,(pos,_) in enumerate(ref) if i not in found
            ]
            if not distances:
                raise ValueError("Insufficient reference corners")
            dist,corner=min(distances)
            if dist>tolerance:
                raise ValueError(
                    "Weld moved source surface corner beyond tolerance")
            found.add(corner)
            source_uv=ref[corner][1]
            current_old=tuple(float(c) for c in working_old[loop_index].uv)
            uv_delta=math.hypot(source_uv[0]-current_old[0],
                                source_uv[1]-current_old[1])
            max_drift=max(max_drift,uv_delta)
            if uv_delta>1e-5:
                drift_count+=1
            old_uvs.append(source_uv)
            new_uvs.append(tuple(float(c) for c in rebuilt_uv[loop_index].uv))
        for bary in BARYCENTRIC_SAMPLES:
            pairs.append((
                (sum(old_uvs[k][0]*bary[k] for k in range(3)),
                 sum(old_uvs[k][1]*bary[k] for k in range(3))),
                (sum(new_uvs[k][0]*bary[k] for k in range(3)),
                 sum(new_uvs[k][1]*bary[k] for k in range(3))),
            ))
    if len(seen)!=len(source_faces):
        raise ValueError("Source triangles were lost or duplicated")
    return pairs, {
        "matchedTriangleCount":len(seen),
        "originalCornerUvDriftCount":drift_count,
        "maxOriginalCornerUvDrift":round(max_drift,8),
        "sourceVertexMatchTolerance":tolerance,
        "originalUvProvenanceIntact":drift_count==0,
    }
