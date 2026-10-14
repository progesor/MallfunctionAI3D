"""Blender background pilot: import one GLB, inspect, export LOD0/1/2 as separate GLBs.

Run: blender -b --python blender/prepare_lods.py -- --input model.glb --outdir some_folder
Does not alter the input file. No re-baking; validate tangent-space normal appearance in Unity.
Requires actual Blender installed; not tested against user's Blender yet.
"""
import argparse
import json
import math
import sys
from pathlib import Path
import bpy
import bmesh


def triangle_count(ob):
    ob.data.calc_loop_triangles()
    return len(ob.data.loop_triangles)


def quality(ob):
    me=ob.data
    bm=bmesh.new()
    try:
        bm.from_mesh(me)
        nonmanifold=sum(1 for e in bm.edges if not e.is_manifold)
    finally: bm.free()
    texture_records=[]
    for m in me.materials:
        if m and m.use_nodes:
            for n in m.node_tree.nodes:
                if n.type=='TEX_IMAGE' and n.image:
                    img=n.image
                    texture_records.append({'material':m.name,'image':img.name,'size_px':list(img.size),
                                            'packed': bool(img.packed_file), 'source':img.source})
    return {'triangles':triangle_count(ob),'vertices':len(me.vertices),
            'mesh_objects':1,'uv_layers':len(me.uv_layers),
            'nonmanifold_edges':nonmanifold,'material_slots':len(me.materials),
            'dimensions_blender_units':list(ob.dimensions),
            'textures':texture_records}


def args_from_cmd():
    ap=argparse.ArgumentParser()
    ap.add_argument('--input',type=Path,required=True)
    ap.add_argument('--name',default='PLAYBOX_FoxPlush',help='Safe asset basename for LOD outputs')
    ap.add_argument('--outdir',type=Path,required=True)
    ap.add_argument('--triangles',type=int,nargs=3,default=[8000,5000,3000])
    ap.add_argument('--height-m',type=float,default=0.0,
                    help='Optional explicitly known real-world height; no auto-scaling when 0')
    return ap.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])


def main():
    a=args_from_cmd()
    if not a.input.is_file(): raise FileNotFoundError(a.input)
    import re
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,63}',a.name):
        raise ValueError('Asset name must be 1-64 ASCII letters, digits, underscore, dash')
    if any(n<100 for n in a.triangles) or list(a.triangles)!=sorted(a.triangles,reverse=True):
        raise ValueError('Provide three descending triangle targets >=100')
    a.outdir.mkdir(parents=True,exist_ok=True)
    bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
    bpy.ops.import_scene.gltf(filepath=str(a.input.resolve()))
    meshes=[o for o in bpy.context.scene.objects if o.type=='MESH']
    if not meshes:raise RuntimeError('Imported GLB has no mesh objects')
    for obj in bpy.context.selected_objects:obj.select_set(False)
    for src in meshes:
        src.select_set(True)
    bpy.context.view_layer.objects.active=meshes[0]
    if len(meshes)>1:
        bpy.ops.object.join()
    master=bpy.context.view_layer.objects.active
    master.name=f'{a.name}_Master'
    if a.height_m>0:
        if master.dimensions.z<=0:raise RuntimeError('Cannot set height on zero-height asset')
        factor=a.height_m/master.dimensions.z
        master.scale=(master.scale.x*factor,master.scale.y*factor,master.scale.z*factor)
        bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    result={'source_glb':str(a.input.resolve()),'source_untouched':True,
            'manual_height_m':a.height_m if a.height_m>0 else None,
            'imported_mesh_count':len(meshes),'joined_master':quality(master),
            'lods':[], 'normal_map_rebaked':False,
            'warning':'Check normal map shading on decimated UVs and bake manually if artifacts appear.'}
    for ix,limit in enumerate(a.triangles):
        clone=master.copy();clone.data=master.data.copy()
        bpy.context.collection.objects.link(clone)
        clone.name=f'{a.name}_LOD{ix}'
        bpy.ops.object.select_all(action='DESELECT')
        clone.select_set(True);bpy.context.view_layer.objects.active=clone
        initial=triangle_count(clone)
        if initial>limit:
            mod=clone.modifiers.new('GameLOD_Decimate','DECIMATE')
            mod.decimate_type='COLLAPSE'
            mod.ratio=min(1.0, limit/initial)
            bpy.ops.object.modifier_apply(modifier=mod.name)
        filename=a.outdir/f'{a.name}_LOD{ix}.glb'
        bpy.ops.export_scene.gltf(filepath=str(filename.resolve()), export_format='GLB',use_selection=True)
        row=quality(clone);row['target_triangles']=limit;row['file']=filename.name
        row['file_bytes']=filename.stat().st_size
        result['lods'].append(row)
        bpy.data.objects.remove(clone,do_unlink=True)
    (a.outdir/'Blender_LOD_Report.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('MALLFUNCTION_LOD_REPORT:',a.outdir/'Blender_LOD_Report.json')

if __name__=='__main__':main()
