"""Blender-independent exact-position indexed triangle topology reconstruction.

Reusable for *any* static model. All source triangle IDs and per-face-corner
attributes (UV, normals, material) must be transferred by the Blender adapter.
No artist-specific/head-tail labels and NO epsilon/tolerance weld by default.
"""
from collections import Counter
from math import isfinite


def _component_count(vertex_count, faces):
    parent = list(range(vertex_count))
    def find(v):
        while parent[v] != v:
            parent[v] = parent[parent[v]]
            v = parent[v]
        return v
    used = set()
    for a, b, c in faces:
        ra, rb, rc = find(a), find(b), find(c)
        parent[rb] = ra
        parent[rc] = ra
        used.update((a, b, c))
    return len({find(v) for v in used})


def _edge_profile(faces):
    edges = Counter()
    for a, b, c in faces:
        for x, y in ((a, b), (b, c), (c, a)):
            edges[tuple(sorted((x, y)))] += 1
    return {'boundaryEdges': sum(n == 1 for n in edges.values()),
            'nonManifoldEdges': sum(n > 2 for n in edges.values())}


def exact_topology_plan(vertices, faces):
    """Return an immutable reconstruction PLAN or raise before changing mesh.

    Duplicate XYZ positions share one geometric vertex after reconstruction.
    Every original face retains its order, orientation, ID and 3D corners.
    UVs and normals may differ at an original duplicated vertex, so they MUST
    be restored per Blender LOOP by the adapter; never derive UV by XYZ alone.
    """
    if len(vertices) == 0 or len(faces) == 0:
        raise ValueError('No triangle mesh to reconstruct')
    if len(vertices) > 2_000_000 or len(faces) > 4_000_000:
        raise ValueError('Mesh size above safe pilot limits')
    mapping = {}
    unique = []
    old_to_unique = []
    for idx, value in enumerate(vertices):
        if len(value) != 3 or any(not isfinite(float(x)) for x in value):
            raise ValueError(f'Invalid 3D vertex position: {idx}')
        xyz = tuple(float(x) for x in value)
        if xyz not in mapping:
            mapping[xyz] = len(unique)
            unique.append(xyz)
        old_to_unique.append(mapping[xyz])
    tris = []
    for face_index, face in enumerate(faces):
        if len(face) != 3 or any(not isinstance(v, int) or not 0 <= v < len(vertices)
                                 for v in face):
            raise ValueError(f'Invalid triangle index: {face_index}')
        triangle = tuple(old_to_unique[v] for v in face)
        if len(set(triangle)) != 3:
            raise ValueError(f'Exact weld would collapse a triangle: {face_index}')
        a, b, c = (unique[k] for k in triangle)
        v = tuple(b[k] - a[k] for k in range(3))
        w = tuple(c[k] - a[k] for k in range(3))
        cross = (v[1] * w[2] - v[2] * w[1],
                 v[2] * w[0] - v[0] * w[2],
                 v[0] * w[1] - v[1] * w[0])
        if all(x == 0.0 for x in cross):
            raise ValueError(f'Exact weld would create zero area face: {face_index}')
        tris.append(triangle)
    before = _edge_profile(faces)
    after = _edge_profile(tris)
    return {
        'algorithm': 'exact-position-topology-v1',
        'vertexPositions': unique,
        'triangleIndices': tris,
        'oldToUnique': old_to_unique,
        'inputVertices': len(vertices),
        'uniqueVertices': len(unique),
        'weldedVertices': len(vertices) - len(unique),
        'triangleCount': len(tris),
        'sourceConnectedComponents': _component_count(len(vertices), faces),
        'weldedConnectedComponents': _component_count(len(unique), tris),
        'sourceBoundaryEdges': before['boundaryEdges'],
        'sourceNonManifoldEdges': before['nonManifoldEdges'],
        'weldedBoundaryEdges': after['boundaryEdges'],
        'weldedNonManifoldEdges': after['nonManifoldEdges'],
        'sourceCornerPositionsPreserved': all(
            tuple(float(x) for x in vertices[face[k]]) == unique[tris[i][k]]
            for i, face in enumerate(faces) for k in range(3)),
    }
