//! Read-only glTF indexed-triangle audit for Blender import differences.
//! Unlike inspect_glb's accessor.count / 3, this decodes actual POSITION and
//! triangle indices to identify provably zero-area source triangles.
//! Only such triangles may explain a Blender import count difference; a
//! one-triangle tolerance without evidence would hide a real topology loss.
use crate::storage::StudioResult;
use crate::unity_export::parse_glb;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTriangles {
    pub declared: u64,
    pub zero_area: u64,
    pub duplicate_faces: u64,
    pub effective: u64,
}

struct Attribute<'a> {
    binary: &'a [u8],
    start: usize,
    stride: usize,
    width: usize,
    count: usize,
}

fn usize_field(value:&Value, field:&str, default:Option<usize>) -> StudioResult<usize> {
    match value.get(field) {
        Some(v) => v.as_u64()
            .and_then(|n|usize::try_from(n).ok())
            .ok_or_else(||format!("glTF {field} is invalid")),
        None => default.ok_or_else(||format!("glTF {field} is required")),
    }
}
fn read_attr<'a>(
    root:&Value, binary:&'a [u8], index:usize,
    kind:&str, component_type:u64, width:usize,
) -> StudioResult<Attribute<'a>> {
    let accessor=root.get("accessors").and_then(Value::as_array)
        .and_then(|list|list.get(index))
        .ok_or("glTF accessor index outside source")?;
    if accessor.get("sparse").is_some() {
        return Err("Sparse source accessor cannot be audited safely".into());
    }
    if accessor.get("type").and_then(Value::as_str)!=Some(kind) ||
       accessor.get("componentType").and_then(Value::as_u64)!=Some(component_type) {
        return Err(format!("glTF index/position accessor format unsupported: {kind} {component_type}"));
    }
    let count=usize_field(accessor,"count",None)?;
    let view_index=usize_field(accessor,"bufferView",None)?;
    let view=root.get("bufferViews").and_then(Value::as_array)
        .and_then(|a|a.get(view_index)).ok_or("glTF bufferView index out of range")?;
    if usize_field(view,"buffer",Some(0))?!=0 {
        return Err("glTF source buffer is not embedded buffer 0".into());
    }
    let view_begin=usize_field(view,"byteOffset",Some(0))?;
    let view_length=usize_field(view,"byteLength",None)?;
    let view_end=view_begin.checked_add(view_length).ok_or("glTF bufferView length overflow")?;
    if view_end>binary.len(){return Err("glTF bufferView exceeds BIN payload".into());}
    let local=usize_field(accessor,"byteOffset",Some(0))?;
    let start=view_begin.checked_add(local).ok_or("glTF accessor byte offset overflow")?;
    let stride=usize_field(view,"byteStride",Some(width))?;
    if width==0 || stride<width || start>view_end {
        return Err("glTF accessor stride is invalid".into());
    }
    let total=if count==0 {0} else {(count-1).checked_mul(stride)
        .and_then(|v|v.checked_add(width)).ok_or("glTF accessor count overflow")?};
    if total>view_end-start {
        return Err("glTF accessor exceeds bufferView limits".into());
    }
    Ok(Attribute{binary,start,stride,width,count})
}
impl Attribute<'_> {
    fn element(&self, index:usize)->StudioResult<&[u8]> {
        if index>=self.count {return Err("glTF triangle index exceeds POSITION accessor count".into());}
        let offset=self.start+index*self.stride;
        Ok(&self.binary[offset..offset+self.width])
    }
}
fn position(a:&Attribute<'_>, index:usize) -> StudioResult<[f64;3]> {
    let data=a.element(index)?;
    let p=[
        f32::from_le_bytes(data[0..4].try_into().unwrap()) as f64,
        f32::from_le_bytes(data[4..8].try_into().unwrap()) as f64,
        f32::from_le_bytes(data[8..12].try_into().unwrap()) as f64,
    ];
    if p.iter().any(|c|!c.is_finite()) {
        return Err("glTF source has non-finite POSITION data".into());
    }
    Ok(p)
}
fn index(a:&Attribute<'_>, i:usize, ty:u64) -> StudioResult<usize> {
    let data=a.element(i)?;
    let v=match ty {
        5121=>data[0] as u64,
        5123=>u16::from_le_bytes(data[..2].try_into().unwrap()) as u64,
        5125=>u32::from_le_bytes(data[..4].try_into().unwrap()) as u64,
        _=>return Err("glTF source index component type not supported".into())
    };
    usize::try_from(v).map_err(|_|"glTF index too large".to_string())
}
fn exact_zero_area(a:[f64;3], b:[f64;3], c:[f64;3]) -> bool {
    // Not an arbitrary epsilon: near-zero *valid* faces must NOT be discarded.
    // Explicit zero index or exactly collinear source vertex positions only.
    let v=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];
    let w=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];
    let cross=[
        v[1]*w[2]-v[2]*w[1],
        v[2]*w[0]-v[0]*w[2],
        v[0]*w[1]-v[1]*w[0],
    ];
    cross.iter().all(|x|*x==0.0)
}

/// Same three indices in cyclic winding order are a provably redundant face
/// within ONE glTF primitive: same material, all vertex attributes, and front
/// orientation. Reverse winding is intentionally NOT considered redundant.
fn oriented_key(a:usize,b:usize,c:usize) -> [usize;3] {
    [[a,b,c],[b,c,a],[c,a,b]].into_iter().min().unwrap()
}

pub fn audit_source_triangles(src:&[u8]) -> StudioResult<SourceTriangles> {
    let (root,bin)=parse_glb(src)?;
    let mut declared=0u64;
    let mut zero_area=0u64;
    let mut duplicate_faces=0u64;
    let meshes=root.get("meshes").and_then(Value::as_array).ok_or("glTF source meshes absent")?;
    for mesh in meshes {
        let prims=mesh.get("primitives").and_then(Value::as_array).ok_or("glTF primitives absent")?;
        for primitive in prims {
            if primitive.get("mode").and_then(Value::as_u64).unwrap_or(4)!=4 {
                return Err("Cannot audit a non-TRIANGLES glTF primitive".into());
            }
            let pos_ix=usize_field(&primitive["attributes"],"POSITION",None)?;
            let positions=read_attr(&root,bin,pos_ix,"VEC3",5126,12)?;
            let ind_ix=usize_field(primitive,"indices",None)?;
            let accessor=root.get("accessors").and_then(Value::as_array)
                .and_then(|a|a.get(ind_ix)).ok_or("Index accessor outside glTF")?;
            let ty=accessor.get("componentType").and_then(Value::as_u64)
                .ok_or("glTF index componentType missing")?;
            let elem=match ty {5121=>1,5123=>2,5125=>4,_=>return Err(
                format!("Unsupported glTF index component type: {ty}"))};
            let indices=read_attr(&root,bin,ind_ix,"SCALAR",ty,elem)?;
            if indices.count%3!=0{return Err("glTF index count not divisible by three".into());}
            declared=declared.checked_add((indices.count/3) as u64)
                .ok_or("glTF triangle count overflow")?;
            // Reset per primitive: across primitives, materials may differ.
            let mut seen=HashSet::<[usize;3]>::new();
            for i in (0..indices.count).step_by(3) {
                let ia=index(&indices,i,ty)?;
                let ib=index(&indices,i+1,ty)?;
                let ic=index(&indices,i+2,ty)?;
                let a=position(&positions,ia)?;
                let b=position(&positions,ib)?;
                let c=position(&positions,ic)?;
                if ia==ib || ib==ic || ia==ic || exact_zero_area(a,b,c) {
                    zero_area+=1;
                } else if !seen.insert(oriented_key(ia,ib,ic)) {
                    duplicate_faces+=1;
                }
            }
        }
    }
    if declared==0 {return Err("glTF source has zero triangles".into());}
    Ok(SourceTriangles{
        declared,zero_area,duplicate_faces,
        effective:declared-zero_area-duplicate_faces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn triangle_glb(second_degenerate:bool) -> Vec<u8> {
        let xyz:[[f32;3];4]=[
            [0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]
        ];
        let mut buffer=Vec::new();
        for p in xyz {for x in p {buffer.extend_from_slice(&x.to_le_bytes());}}
        for index in if second_degenerate {
            [0u16,1,2,0,0,3]
        } else {
            [0u16,1,2,0,2,3]
        } {buffer.extend_from_slice(&index.to_le_bytes());}
        let doc=json!({
          "asset":{"version":"2.0"},
          "buffers":[{"byteLength":buffer.len()}],
          "bufferViews":[
              {"buffer":0,"byteOffset":0,"byteLength":48},
              {"buffer":0,"byteOffset":48,"byteLength":12}
          ],
          "accessors":[
              {"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"},
              {"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}
          ],
          "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
          "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0
        });
        let mut json_bytes=serde_json::to_vec(&doc).unwrap();
        while json_bytes.len()%4!=0 {json_bytes.push(b' ');}
        let total=12+8+json_bytes.len()+8+buffer.len();
        let mut out=Vec::new();
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&json_bytes);
        out.extend_from_slice(&(buffer.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&buffer);
        assert_eq!(out.len(),total);
        out
    }
    #[test] fn documents_one_provably_degenerate_triangle() {
        let count=audit_source_triangles(&triangle_glb(true)).unwrap();
        assert_eq!(count,SourceTriangles{declared:2,zero_area:1,duplicate_faces:0,effective:1});
        let clean=audit_source_triangles(&triangle_glb(false)).unwrap();
        assert_eq!(clean,SourceTriangles{declared:2,zero_area:0,duplicate_faces:0,effective:2});
    }
    #[test] fn redundant_faces_same_winding_count_once() {
        assert_eq!(oriented_key(0,1,2), oriented_key(1,2,0));
        assert_eq!(oriented_key(0,1,2), oriented_key(2,0,1));
        assert_ne!(oriented_key(0,1,2), oriented_key(0,2,1));
        let mut glb=triangle_glb(false);
        // This fixture has a two-triangle buffer. The checker above verifies
        // oriented-key behavior without inventing a duplicate binary blob.
        assert_eq!(audit_source_triangles(&glb).unwrap().duplicate_faces,0);
        // Mutate second triangle indices to the *same* oriented face as the
        // first one (0,1,2). Blender can remove such duplicate faces on import.
        let second_start=glb.len()-6;
        glb[second_start..].copy_from_slice(&[
            1,0,2,0,0,0, // u16 indices 1,2,0
        ]);
        let audit=audit_source_triangles(&glb).unwrap();
        assert_eq!(audit,SourceTriangles{
            declared:2,zero_area:0,duplicate_faces:1,effective:1,
        });
    }
    #[test] fn refuses_out_of_bounds_source_triangle_indices() {
        let mut glb=triangle_glb(false);
        // Last u16 index in buffer, after headers and JSON.
        let last=glb.len()-2;
        glb[last..].copy_from_slice(&99u16.to_le_bytes());
        assert!(audit_source_triangles(&glb).is_err());
    }
}
