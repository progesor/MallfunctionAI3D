"""Read-only topology audit for arbitrary static .glb models, no Blender/pip.

Usage: python tools/inspect_glb_topology.py path/to/source.glb
Does not decompress embedded PNGs or modify the source model.
"""
import argparse
import json
from pathlib import Path
import struct
import sys

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/"blender"))
from uv_topology import exact_topology_plan

TYPES={5121:("B",1),5123:("H",2),5125:("I",4),5126:("f",4)}
SHAPES={"VEC3":3,"SCALAR":1}


def read_glb_binary(path):
    size=path.stat().st_size
    if size<28 or size>250_000_000:
        raise ValueError("GLB size outside 28 bytes..250 MB")
    with path.open("rb") as f:
        magic,version,declared=struct.unpack("<4sII",f.read(12))
        if magic!=b"glTF" or version!=2 or declared!=size:
            raise ValueError("GLB header invalid")
        jl,tag=struct.unpack("<I4s",f.read(8))
        if tag!=b"JSON" or jl>5_000_000:
            raise ValueError("JSON chunk missing or too large")
        root=json.loads(f.read(jl))
        bl,tag=struct.unpack("<I4s",f.read(8))
        if tag!=b"BIN\x00" or bl>size:
            raise ValueError("Binary chunk missing")
        data=f.read(bl)
    if len(data)!=bl:
        raise ValueError("GLB binary chunk truncated")
    return root,data


def get_accessor(root,blob,index):
    accessor=root["accessors"][index]
    if "sparse" in accessor or accessor.get("normalized",False):
        raise ValueError("Sparse/normalized geometry accessor not supported")
    fmt,width=TYPES[accessor["componentType"]]
    columns=SHAPES[accessor["type"]]
    view=root["bufferViews"][accessor["bufferView"]]
    if view.get("buffer",0)!=0:
        raise ValueError("Only embedded GLB buffer 0 supported")
    stride=view.get("byteStride",width*columns)
    count=accessor["count"]
    start=view.get("byteOffset",0)+accessor.get("byteOffset",0)
    end=view.get("byteOffset",0)+view["byteLength"]
    if stride<width*columns or start<0 or end>len(blob) or \
       (count and start+(count-1)*stride+width*columns>end):
        raise ValueError("Geometry accessor exceeds BIN data")
    return [struct.unpack_from("<"+fmt*columns,blob,start+i*stride)
            for i in range(count)]


def analyze_glb(path):
    root,blob=read_glb_binary(path)
    results=[]
    for mi,mesh in enumerate(root.get("meshes",[])):
        for pi,prim in enumerate(mesh.get("primitives",[])):
            record={"meshIndex":mi,"primitiveIndex":pi}
            try:
                if prim.get("mode",4)!=4:
                    raise ValueError("Triangle mode 4 only")
                pos=get_accessor(root,blob,prim["attributes"]["POSITION"])
                if "indices" in prim:
                    ix=[int(x[0]) for x in get_accessor(root,blob,prim["indices"])]
                else:
                    ix=list(range(len(pos)))
                if len(ix)%3:
                    raise ValueError("Index count must be multiple of three")
                faces=[tuple(ix[k:k+3]) for k in range(0,len(ix),3)]
                record.update({
                    k:v for k,v in exact_topology_plan(pos,faces).items()
                    if k not in ("vertexPositions","triangleIndices","oldToUnique")
                })
                record["status"]="AUDIT_OK"
            except (ValueError,KeyError,IndexError,struct.error,TypeError) as exc:
                record["status"]="UNSUPPORTED_OR_INVALID"
                record["error"]=str(exc)
            results.append(record)
    return {"file":str(path),"assetVersion":root.get("asset",{}).get("version"),
            "meshes":len(root.get("meshes",[])),
            "primitives":results}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("glb",type=Path)
    args=p.parse_args()
    if not args.glb.is_file():
        p.error("GLB file does not exist")
    result=analyze_glb(args.glb)
    print(json.dumps(result,indent=2,ensure_ascii=False))
    if not result["primitives"] or any(x["status"]!="AUDIT_OK"
                                      for x in result["primitives"]):
        return 2
    return 0


if __name__=="__main__":
    sys.exit(main())
