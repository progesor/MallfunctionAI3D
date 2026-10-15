//! Reusable geometric UV seam matching for static glTF 2.0 triangle meshes.
//! No Blender, atlas rebuilding, normals replacement or texture mutation.
//! Candidate seams are matching exact 3D edges with differing UV corners.
use crate::storage::StudioResult;
use crate::unity_export::parse_glb;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone,Copy,Debug)]
pub(crate) struct VertexUV {
    pub position:[f32;3],
    pub uv:[f32;2],
}
#[derive(Clone,Copy,Debug)]
pub(crate) struct SeamSide {
    pub start:[f32;2],
    pub end:[f32;2],
    pub inside:[f32;2],
    pub normal:[f64;3],
}
#[derive(Clone,Debug)]
pub(crate) struct SeamPair { pub left:SeamSide,pub right:SeamSide }

#[derive(Serialize,Clone,Debug)]
#[serde(rename_all="camelCase")]
pub struct SeamGeometryReport {
    pub triangles:usize,
    pub vertices:usize,
    pub geometric_edges:usize,
    pub candidate_uv_seams:usize,
    pub continuous_edges:usize,
    pub intentional_sharp_edges_skipped:usize,
    pub non_manifold_edges_skipped:usize,
    pub open_boundary_edges:usize,
    pub degenerate_faces_skipped:usize,
}
pub(crate) struct SeamGeometry {
    pub seams:Vec<SeamPair>,
    pub report:SeamGeometryReport,
}
type VertexKey=[u32;3];
type EdgeKey=(VertexKey,VertexKey);

fn canonical(v:f32)->u32 { if v==0. {0} else {v.to_bits()} }
fn key(p:[f32;3])->VertexKey {[canonical(p[0]),canonical(p[1]),canonical(p[2])]}
fn face_normal(a:[f32;3],b:[f32;3],c:[f32;3])->Option<[f64;3]>{
    let u=[(b[0]-a[0]) as f64,(b[1]-a[1]) as f64,(b[2]-a[2]) as f64];
    let v=[(c[0]-a[0]) as f64,(c[1]-a[1]) as f64,(c[2]-a[2]) as f64];
    let x=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
    let len=(x[0]*x[0]+x[1]*x[1]+x[2]*x[2]).sqrt();
    if len<1e-16 || !len.is_finite() { None }
    else {Some([x[0]/len,x[1]/len,x[2]/len])}
}
fn approx_uv(a:[f32;2],b:[f32;2])->bool{
    (a[0]-b[0]).abs()<0.00001 && (a[1]-b[1]).abs()<0.00001
}
fn ufield(v:&Value,k:&str)->StudioResult<usize>{
    v.get(k).and_then(Value::as_u64)
        .and_then(|x|usize::try_from(x).ok())
        .ok_or_else(||format!("glTF field missing/invalid: {k}"))
}
struct Accessor<'a> {
    bin:&'a[u8],start:usize,stride:usize,count:usize,width:usize
}
impl Accessor<'_>{
    fn raw(&self,i:usize)->StudioResult<&[u8]>{
        if i>=self.count {return Err("glTF accessor index outside valid range".into());}
        let pos=self.start+i*self.stride;
        Ok(&self.bin[pos..pos+self.width])
    }
    fn floats<const N:usize>(&self,i:usize)->StudioResult<[f32;N]>{
        if self.width!=N*4{return Err("glTF float accessor width mismatch".into());}
        let b=self.raw(i)?;
        let mut out=[0.0;N];
        for (j,x) in out.iter_mut().enumerate(){
            *x=f32::from_le_bytes(b[j*4..j*4+4].try_into().unwrap());
            if !x.is_finite(){return Err("glTF POSITION/UV contains non-finite component".into());}
        }
        Ok(out)
    }
}
fn reader<'a>(root:&Value,bin:&'a[u8],ix:usize,kind:&str,
              ty:u64,width:usize)->StudioResult<Accessor<'a>>{
    let a=root.get("accessors").and_then(Value::as_array)
        .and_then(|arr|arr.get(ix)).ok_or("glTF accessor missing")?;
    if a.get("sparse").is_some() || a.get("normalized")==Some(&Value::Bool(true)) ||
       a.get("type").and_then(Value::as_str)!=Some(kind) ||
       a.get("componentType").and_then(Value::as_u64)!=Some(ty) {
        return Err("Unsupported sparse/normalized/non-float glTF geometry accessor".into());
    }
    let count=ufield(a,"count")?;
    if count>2_000_000{return Err("glTF accessor too large for seam analyzer".into());}
    let vi=ufield(a,"bufferView")?;
    let view=root.get("bufferViews").and_then(Value::as_array)
        .and_then(|v|v.get(vi)).ok_or("glTF bufferView missing")?;
    if view.get("buffer").and_then(Value::as_u64).unwrap_or(0)!=0{
        return Err("External GLB buffer unsupported".into());
    }
    let vstart=view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let vlen=ufield(view,"byteLength")?;
    let vend=vstart.checked_add(vlen).ok_or("GLB bufferView overflow")?;
    let local=a.get("byteOffset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let start=vstart.checked_add(local).ok_or("GLB accessor byte offset overflow")?;
    let stride=view.get("byteStride").and_then(Value::as_u64)
        .unwrap_or(width as u64) as usize;
    let required=if count==0 {0}else{(count-1).checked_mul(stride)
        .and_then(|n|n.checked_add(width)).ok_or("GLB accessor size overflow")?};
    if stride<width || vend>bin.len() || start>vend ||
       required>vend-start {
        return Err("GLB accessor crosses binary view boundary".into());
    }
    Ok(Accessor{bin,start,stride,count,width})
}
pub(crate) fn analyze_seams(bytes:&[u8])->StudioResult<SeamGeometry>{
    let (root,bin)=parse_glb(bytes)?;
    let meshes=root.get("meshes").and_then(Value::as_array)
        .ok_or("GLB has no meshes")?;
    if meshes.len()!=1 {return Err("Seam repair M3 supports one static mesh (multiple meshes need separate material groups)".into());}
    let prims=meshes[0].get("primitives").and_then(Value::as_array)
        .ok_or("GLB mesh primitives absent")?;
    if prims.len()!=1{return Err("Seam repair M3 supports one triangle primitive per asset".into());}
    let prim=&prims[0];
    if prim.get("mode").and_then(Value::as_u64).unwrap_or(4)!=4{
        return Err("Seam repair requires TRIANGLES glTF mesh".into());
    }
    let attrs=prim.get("attributes").ok_or("GLB attributes absent")?;
    let pos=reader(&root,bin,ufield(attrs,"POSITION")?,"VEC3",5126,12)?;
    let tex=reader(&root,bin,ufield(attrs,"TEXCOORD_0")?,"VEC2",5126,8)?;
    if pos.count==0 || pos.count!=tex.count{
        return Err("Seam repair source POSITION and UV0 count mismatch".into());
    }
    let ind_ix=ufield(prim,"indices")?;
    let desc=root.get("accessors").and_then(Value::as_array)
        .and_then(|x|x.get(ind_ix)).ok_or("GLB indices missing")?;
    let kind=desc.get("componentType").and_then(Value::as_u64)
        .ok_or("GLB index type missing")?;
    let width=match kind{5121=>1,5123=>2,5125=>4,
        _=>return Err("Unsupported GLB triangle index type".into())};
    let indices=reader(&root,bin,ind_ix,"SCALAR",kind,width)?;
    if indices.count%3!=0 || indices.count>1_200_000{
        return Err("GLB triangle index count is invalid/too large".into());
    }
    let read_index=|i:usize|->StudioResult<usize>{
        let b=indices.raw(i)?;
        let v=match kind{5121=>b[0] as usize,
            5123=>u16::from_le_bytes(b[..2].try_into().unwrap()) as usize,
            5125=>u32::from_le_bytes(b[..4].try_into().unwrap()) as usize,
            _=>return Err("Index type unsupported".into())};
        if v>=pos.count {return Err("GLB triangle index out of range".into());}
        Ok(v)
    };
    let mut edges=BTreeMap::<EdgeKey,Vec<SeamSide>>::new();
    let mut degenerate=0usize;
    for face in 0..indices.count/3{
        let inds=[read_index(face*3)?,read_index(face*3+1)?,read_index(face*3+2)?];
        let tri=[
            VertexUV{position:pos.floats(inds[0])?,uv:tex.floats(inds[0])?},
            VertexUV{position:pos.floats(inds[1])?,uv:tex.floats(inds[1])?},
            VertexUV{position:pos.floats(inds[2])?,uv:tex.floats(inds[2])?}
        ];
        let normal=match face_normal(tri[0].position,tri[1].position,tri[2].position){
            Some(v)=>v,None=>{degenerate+=1;continue}
        };
        for i in 0..3{
            let left=tri[i];let right=tri[(i+1)%3];let inside=tri[(i+2)%3];
            let a=key(left.position);let b=key(right.position);
            if a==b {continue}
            let (edge,uv_start,uv_end)=if a<b {
                ((a,b),left.uv,right.uv)
            } else {((b,a),right.uv,left.uv)};
            edges.entry(edge).or_default().push(
                SeamSide{start:uv_start,end:uv_end,inside:inside.uv,normal});
        }
    }
    let mut report=SeamGeometryReport{
        triangles:indices.count/3,vertices:pos.count,geometric_edges:edges.len(),
        candidate_uv_seams:0,continuous_edges:0,intentional_sharp_edges_skipped:0,
        non_manifold_edges_skipped:0,open_boundary_edges:0,
        degenerate_faces_skipped:degenerate,
    };
    let mut seams=Vec::new();
    for (_,faces) in edges{
        if faces.len()==1 {report.open_boundary_edges+=1;continue}
        if faces.len()!=2 {report.non_manifold_edges_skipped+=1;continue}
        let a=faces[0];let b=faces[1];
        if approx_uv(a.start,b.start)&&approx_uv(a.end,b.end) {
            report.continuous_edges+=1;continue
        }
        let cos=a.normal.iter().zip(b.normal.iter()).map(|(x,y)|x*y).sum::<f64>();
        // Automatic smoothing must not erase seams across sharp creases
        // or opposite face winding; those need user painting/normal review.
        if cos<0.90 {
            report.intentional_sharp_edges_skipped+=1;continue
        }
        let all=[a.start,a.end,b.start,b.end,a.inside,b.inside];
        if all.iter().any(|v|v.iter().any(|x|!x.is_finite() ||
           *x<0.0 || *x>1.0)) {continue}
        seams.push(SeamPair{left:a,right:b});
        if seams.len()>25_000 {
            return Err("Too many seam candidates; divide/inspect this mesh first".into());
        }
    }
    report.candidate_uv_seams=seams.len();
    Ok(SeamGeometry{seams,report})
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn detects_one_smooth_uv_seam_without_geometry_change(){
        let positions:[[f32;3];6]=[
            [0.,0.,0.],[1.,0.,0.],[0.,1.,0.],
            [0.,0.,0.],[0.,1.,0.],[-1.,0.,0.]
        ];
        let uv:[[f32;2];6]=[
            [.10,.10],[.30,.10],[.10,.90],
            [.65,.10],[.65,.90],[.90,.10]
        ];
        let mut binary=Vec::new();
        for p in positions {for x in p{binary.extend_from_slice(&x.to_le_bytes());}}
        for p in uv {for x in p{binary.extend_from_slice(&x.to_le_bytes());}}
        for i in [0u16,1,2,3,4,5]{binary.extend_from_slice(&i.to_le_bytes());}
        let doc=serde_json::json!({
            "asset":{"version":"2.0"},
            "buffers":[{"byteLength":binary.len()}],
            "bufferViews":[
                {"buffer":0,"byteOffset":0,"byteLength":72},
                {"buffer":0,"byteOffset":72,"byteLength":48},
                {"buffer":0,"byteOffset":120,"byteLength":12}
            ],
            "accessors":[
                {"bufferView":0,"componentType":5126,"count":6,"type":"VEC3"},
                {"bufferView":1,"componentType":5126,"count":6,"type":"VEC2"},
                {"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}
            ],
            "meshes":[{"primitives":[{"attributes":{
                "POSITION":0,"TEXCOORD_0":1},"indices":2}]}]
        });
        let mut text=serde_json::to_vec(&doc).unwrap();
        while text.len()%4!=0{text.push(b' ');}
        let total=12+8+text.len()+8+binary.len();
        let mut glb=Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(text.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&text);
        glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(&binary);
        let source=glb.clone();
        let result=analyze_seams(&glb).unwrap();
        assert_eq!(result.report.triangles,2);
        assert_eq!(result.report.vertices,6);
        assert_eq!(result.report.candidate_uv_seams,1);
        assert_eq!(result.report.open_boundary_edges,4);
        assert_eq!(glb,source);
    }
    #[test]fn triangle_normal_geometry(){
        let n=face_normal([0.,0.,0.],[1.,0.,0.],[0.,1.,0.]).unwrap();
        assert_eq!(n,[0.,0.,1.]);
        assert!(approx_uv([0.1,0.2],[0.1000001,0.2000001]));
        assert!(!approx_uv([0.1,0.2],[0.2,0.2]));
    }
}
