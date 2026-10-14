#!/usr/bin/env python3
"""Read-only GLB v2 manifest/QC report; no Blender, pip or external packages required.
Checks structural glTF data, embedded textures, UVs, material bindings and poly counts.
Does not attempt UV overlap / topology / visual fidelity checks (use Blender for those).
"""
import argparse
import json
import struct
import sys
from pathlib import Path


def glb_manifest(path: Path) -> dict:
    file_bytes = path.stat().st_size
    with path.open('rb') as f:
        header=f.read(12)
        if len(header)!=12:
            raise ValueError('Not enough bytes for GLB header')
        magic,version,total=struct.unpack('<4sII',header)
        if magic!=b'glTF' or version!=2:
            raise ValueError('Not a GLB v2 file')
        if total!=file_bytes:
            raise ValueError(f'GLB header length {total} != file size {file_bytes}')
        obj=None;bin_chunks=[]
        offset=12
        while offset<total:
            c=f.read(8)
            if len(c)!=8: raise ValueError('Truncated chunk header')
            size,typ=struct.unpack('<I4s',c)
            offset+=8+size
            if offset>total: raise ValueError('Chunk exceeds declared GLB length')
            if typ==b'JSON':
                if obj is not None: raise ValueError('Multiple JSON chunks')
                if size>64_000_000: raise ValueError('Refusing giant JSON chunk')
                obj=json.loads(f.read(size).decode('utf-8').rstrip(' \x00'))
            else:
                if typ==b'BIN\x00': bin_chunks.append(size)
                f.seek(size,1)
        if obj is None: raise ValueError('Missing JSON chunk')
    if obj.get('asset',{}).get('version')!='2.0':
        raise ValueError('Not a glTF 2.0 asset')
    accessors=obj.get('accessors',[])
    meshes=obj.get('meshes',[])
    materials=obj.get('materials',[])
    images=obj.get('images',[])
    textures=obj.get('textures',[])
    samplers=obj.get('samplers',[])
    warnings=[]
    tris=0;primitives=0;uv_count=0;normal_count=0;vertex_count=0
    indexed_primitives=0;material_indices=set();non_triangle_modes=[]
    for mesh in meshes:
        for prim in mesh.get('primitives',[]):
            primitives+=1
            attrs=prim.get('attributes',{})
            uv_count+=int('TEXCOORD_0' in attrs)
            normal_count+=int('NORMAL' in attrs)
            pos=attrs.get('POSITION')
            if pos is None or pos<0 or pos>=len(accessors):
                warnings.append('Primitive without valid POSITION accessor'); continue
            verts=accessors[pos].get('count',0)
            vertex_count+=verts
            mode=prim.get('mode',4)
            index=prim.get('indices')
            if index is not None:
                if not 0<=index<len(accessors):
                    raise ValueError('Invalid indices accessor')
                idx_count=accessors[index].get('count',0)
                indexed_primitives+=1
            else: idx_count=verts
            if mode==4: tris+=idx_count//3
            elif mode in (5,6): tris+=max(idx_count-2,0)
            else: non_triangle_modes.append(mode)
            mat=prim.get('material')
            if mat is not None:
                if not 0<=mat<len(materials):raise ValueError('Invalid primitive material')
                material_indices.add(mat)
    texinfo=[]
    for i,image in enumerate(images):
        embedded = 'bufferView' in image
        uri = image.get('uri')
        texinfo.append({'index':i,'name':image.get('name'),
                        'mimeType':image.get('mimeType'),
                        'embedded_in_glb':embedded or (isinstance(uri,str) and uri.startswith('data:')),
                        'uri':uri if uri and not uri.startswith('data:') else None})
        if not (embedded or (isinstance(uri,str) and uri.startswith('data:'))):
            warnings.append(f'Image {i} is external/not packed into GLB')
    material_report=[]
    total_base=total_metalrough=total_normal=total_ao=0
    for i,m in enumerate(materials):
        pbr=m.get('pbrMetallicRoughness',{})
        flags={
            'base_color_map':'baseColorTexture' in pbr,
            'metallic_roughness_map':'metallicRoughnessTexture' in pbr,
            'normal_map':'normalTexture' in m,
            'ao_map':'occlusionTexture' in m,
            'emissive_map':'emissiveTexture' in m,
        }
        if i in material_indices:
            total_base+=int(flags['base_color_map'])
            total_metalrough+=int(flags['metallic_roughness_map'])
            total_normal+=int(flags['normal_map'])
            total_ao+=int(flags['ao_map'])
        material_report.append({'index':i,'name':m.get('name'),'used_by_mesh':i in material_indices,
                                'flags':flags,'metallicFactor':pbr.get('metallicFactor',1.0),
                                'roughnessFactor':pbr.get('roughnessFactor',1.0),
                                'alphaMode':m.get('alphaMode','OPAQUE')})
    if tris==0: warnings.append('No triangle geometry detected')
    if uv_count!=primitives: warnings.append('Some primitives have no TEXCOORD_0 UVs')
    if normal_count!=primitives: warnings.append('Some primitives have no vertex NORMALs')
    if not material_indices: warnings.append('No assigned mesh material')
    if not total_base: warnings.append('No base color texture assigned to used materials')
    if not total_normal: warnings.append('No normal map assigned to used materials')
    if not total_ao: warnings.append('No ambient occlusion map assigned to used materials')
    if non_triangle_modes: warnings.append(f'Non-triangle primitive modes: {non_triangle_modes}')
    if len(material_indices)>2: warnings.append('More than 2 mesh materials; review Unity draw-call budget')
    return {
        'source_path':str(path.resolve()), 'size_bytes':file_bytes,
        'asset_version':obj['asset']['version'],
        'generator':obj['asset'].get('generator'),
        'unique_mesh_count':len(meshes),'scene_node_count':len(obj.get('nodes',[])),
        'primitive_count':primitives,'triangles_unique_meshes':tris,
        'vertex_count_primitives_sum':vertex_count,'indexed_primitive_count':indexed_primitives,
        'uv_primitive_count':uv_count,'normal_primitive_count':normal_count,
        'assigned_material_count':len(material_indices),'declared_material_count':len(materials),
        'image_count':len(images),'texture_count':len(textures),'sampler_count':len(samplers),
        'embedded_bin_chunks':bin_chunks,'pbr_used_materials_with_basecolor_map':total_base,
        'pbr_used_materials_with_metalrough_map':total_metalrough,
        'pbr_used_materials_with_normal_map':total_normal,
        'pbr_used_materials_with_ao_map':total_ao,
        'images':texinfo, 'materials':material_report,
        'extensions_used':obj.get('extensionsUsed',[]), 'warnings':warnings,
        'requires_blender_visual_and_topology_check':True,
        'accepted_for_game':False,
    }


def main(argv=None):
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('glb',type=Path)
    p.add_argument('--out',type=Path,default=None)
    p.add_argument('--target-triangles',type=int,default=8000)
    args=p.parse_args(argv)
    try:
        report=glb_manifest(args.glb)
        if report['triangles_unique_meshes']>args.target_triangles:
            report['warnings'].append(f"Above pilot target of {args.target_triangles:,} triangles")
        path=args.out or args.glb.with_suffix('.qc.json')
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        print(f"GLB STRUCTURE VALID | {report['triangles_unique_meshes']} triangles | "
              f"{report['assigned_material_count']} materials | {report['image_count']} images | "
              f"{report['uv_primitive_count']}/{report['primitive_count']} UV primitives")
        for item in report['warnings']:print('WARNING:',item)
        print('Written:',path)
        return 0
    except (ValueError,OSError,KeyError,IndexError,json.JSONDecodeError) as ex:
        print('ERROR:',ex,file=sys.stderr);return 1

if __name__=='__main__':sys.exit(main())
