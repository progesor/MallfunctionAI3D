"""Optional MIT-licensed xatlas chart generator for Blender 5.1 UV Studio.

The package is NEVER downloaded or installed by a bake. If Blender's
app-owned plugin directory includes compatible xatlas/numpy, this provider
may contribute one quality-scored candidate to the same strict pre-bake gate.
The user source GLB and original face/UV IDs remain unchanged.
"""
import math
import os
from pathlib import Path


XATLAS_PRESETS=("default","joined","broad")


def native_xatlas_chart_options(xatlas,preset):
    """Chart growth tuned for editable UV chunks, not just low distortion.

    max_cost is the upstream chart-growth ceiling: increasing it can join
    more faces. These are *candidates*, never independently QA-accepted.
    """
    if preset not in XATLAS_PRESETS:
        raise ValueError("Unsupported xatlas chart preset")
    if preset=="default":
        return None
    chart=xatlas.ChartOptions()
    if preset=="joined":
        chart.max_cost=5.0
        chart.normal_deviation_weight=1.0
        chart.normal_seam_weight=1.0
        chart.texture_seam_weight=0.3
        chart.straightness_weight=2.0
        chart.max_iterations=2
    else:
        chart.max_cost=10.0
        chart.normal_deviation_weight=0.4
        chart.normal_seam_weight=0.2
        chart.texture_seam_weight=0.1
        chart.straightness_weight=0.5
        chart.max_iterations=3
    return chart


def apply_xatlas_uv(mesh, clean_uv_name, preset="default", resolution=1024):
    plugin_root=os.environ.get("AI3D_XATLAS_PLUGIN_PATH")
    if not plugin_root:
        return {"available":False,"reason":"App-owned xatlas plugin not installed",
                "engine":"xatlas","accepted":False}
    try:
        import xatlas
        import numpy as np
        native=Path(xatlas.__file__).resolve()
        allowed=Path(plugin_root).resolve()
        if not native.is_relative_to(allowed):
            return {"available":False,"engine":"xatlas","accepted":False,
                    "reason":"xatlas native extension not from Studio plugin folder"}
    except (ImportError, OSError, AttributeError, TypeError) as exc:
        return {"available": False, "reason": str(exc),
                "engine": "xatlas", "accepted": False}
    if not mesh.polygons or not mesh.vertices:
        raise ValueError("xatlas requires nonempty static triangle mesh")
    if any(len(face.vertices) != 3 for face in mesh.polygons):
        raise ValueError("xatlas requires pre-triangulated geometry")

    positions = np.asarray(
        [tuple(float(v.co[k]) for k in range(3)) for v in mesh.vertices],
        dtype=np.float32,
    )
    faces = np.asarray(
        [tuple(int(v) for v in poly.vertices) for poly in mesh.polygons],
        dtype=np.uint32,
    )
    if preset not in XATLAS_PRESETS:
        raise ValueError("Invalid xatlas chart preset")
    if preset=="default":
        # Keep previous proven xatlas 0.0.11 baseline as the first candidate.
        vmapping, indices, uv_coords = xatlas.parametrize(positions, faces)
    else:
        chart=native_xatlas_chart_options(xatlas,preset)
        pack=xatlas.PackOptions()
        pack.padding=4
        pack.resolution=int(resolution)
        atlas=xatlas.Atlas()
        atlas.add_mesh(positions,faces)
        atlas.generate(chart_options=chart,pack_options=pack)
        vmapping, indices, uv_coords=atlas[0]
    vmapping = np.asarray(vmapping)
    indices = np.asarray(indices)
    uv_coords = np.asarray(uv_coords)
    if indices.shape != faces.shape or uv_coords.ndim != 2 or \
       uv_coords.shape[1] != 2 or vmapping.ndim != 1:
        raise ValueError("xatlas output indices/uv coordinates shape mismatch")
    if len(vmapping) != len(uv_coords) or len(vmapping) == 0:
        raise ValueError("xatlas output vertex mapping invalid")
    if not np.isfinite(uv_coords).all() or (uv_coords < -1e-5).any() or \
       (uv_coords > 1.00001).any():
        raise ValueError("xatlas returned invalid/out-of-bounds UV coordinates")
    if np.any(indices >= len(vmapping)) or np.any(vmapping >= len(positions)):
        raise ValueError("xatlas mapping references out-of-range vertices")

    # Some UV packers reorder triangle corners. NEVER assume source triangle
    # faces are in the same order: check exact per-face membership, then map
    # each original corner by its unique vertex index. If face ORDER changed,
    # decline the candidate rather than assign texture to the wrong polygon.
    target_uv = mesh.uv_layers.get(clean_uv_name)
    if target_uv is None:
        raise ValueError("xatlas candidate has no clean UV target")
    new_loops = []
    for face_index, polygon in enumerate(mesh.polygons):
        mesh_vertices = tuple(int(v) for v in polygon.vertices)
        assigned_vertices = tuple(int(vmapping[int(idx)])
                                  for idx in indices[face_index])
        if len(set(mesh_vertices)) != 3 or \
           set(mesh_vertices) != set(assigned_vertices):
            raise ValueError(
                "xatlas changed triangle identity/order; cannot safely "
                "transfer source corner UVs"
            )
        new_uvs = {
            int(vmapping[int(new_idx)]):
            (float(uv_coords[int(new_idx)][0]),
             float(uv_coords[int(new_idx)][1]))
            for new_idx in indices[face_index]
        }
        for corner, vertex in enumerate(mesh_vertices):
            new_loops.append((int(polygon.loop_indices[corner]),
                              new_uvs[vertex]))
    if len(new_loops) != len(mesh.loops):
        raise ValueError("xatlas lost some source face corners")
    # Apply only after every face has been proven to match.
    for loop_id, texcoord in new_loops:
        target_uv.data[loop_id].uv = texcoord
    mesh.uv_layers.active = mesh.uv_layers.get(clean_uv_name)
    mesh.uv_layers.get(clean_uv_name).active_render = True
    return {
        "available": True,
        "engine": "xatlas",
        "atlasPreset": preset,
        "version": str(getattr(xatlas, "__version__", "unknown")),
        "triangles": len(faces),
        "mappedVertices": len(vmapping),
        "accepted": False,  # Set only by shared objective quality gate.
    }
