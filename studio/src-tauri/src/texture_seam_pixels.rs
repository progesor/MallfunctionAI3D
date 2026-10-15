//! Conservative PBR texture seam blending, driven by SAME 3D mesh surface.
//! Never repacks UV, edits vertices, changes source normals or normal PNG.
//! All candidate pixels are sampled from immutable source images (not from
//! intermediate modifications). Texture updates are review-only initially.
use crate::storage::StudioResult;
use crate::texture_seam_geometry::{SeamPair,SeamSide};
use image::{Rgba,RgbaImage};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize,Debug,Clone)]
#[serde(rename_all="camelCase")]
pub(crate) struct SeamMapReport {
    pub role:String,
    pub source_width:u32,
    pub source_height:u32,
    pub eligible_samples:usize,
    pub large_difference_samples_skipped:usize,
    pub unchanged_samples:usize,
    pub changed_pixels:usize,
    pub changed_fraction:f64,
    pub mean_pixel_delta:f64,
    pub maximum_pixel_delta:f64,
    pub scope:String,
}

struct Accumulator {rgb:[f64;3],weight:f64}
type Contributions=BTreeMap<usize,Accumulator>;

fn uv_pixel(uv:[f32;2],w:u32,h:u32)->[f64;2]{
    [uv[0] as f64*(w-1) as f64,
     (1.0-uv[1] as f64)*(h-1) as f64]
}
fn inward(side:&SeamSide,t:f64,w:u32,h:u32)->Option<([f64;2],[f64;2])>{
    let a=uv_pixel(side.start,w,h);
    let b=uv_pixel(side.end,w,h);
    let c=uv_pixel(side.inside,w,h);
    let p=[a[0]*(1.-t)+b[0]*t,a[1]*(1.-t)+b[1]*t];
    let e=[b[0]-a[0],b[1]-a[1]];
    let len=(e[0]*e[0]+e[1]*e[1]).sqrt();
    if len<1e-8 {return None}
    let sign=if e[0]*(c[1]-p[1])-e[1]*(c[0]-p[0])>0.0 {1.0}else{-1.0};
    let norm=[-e[1]/len*sign,e[0]/len*sign];
    Some((p,norm))
}
fn pixel_at(p:[f64;2],w:u32,h:u32)->Option<(u32,u32,usize)>{
    let x=p[0].round() as i64;
    let y=p[1].round() as i64;
    if x<0 || y<0 || x>=w as i64 || y>=h as i64 {return None}
    Some((x as u32,y as u32,y as usize*w as usize+x as usize))
}
fn channel_distance(a:&Rgba<u8>,b:&Rgba<u8>,color:bool)->f64{
    if color{
        ((a[0] as f64-b[0] as f64).abs()+
         (a[1] as f64-b[1] as f64).abs()+
         (a[2] as f64-b[2] as f64).abs())/(3.*255.)
    }else{(a[0] as f64-b[0] as f64).abs()/255.}
}
fn contribute(all:&mut Contributions,side:&SeamSide,t:f64,
              rgba:&RgbaImage,target:[f64;3],radius:u32,
              color:bool){
    let w=rgba.width();let h=rgba.height();
    let Some((p,n))=inward(side,t,w,h) else{return};
    for dist in 0..radius {
        // Never paint outside the triangle's UV chart intentionally.
        let px_dist=0.75+dist as f64;
        let q=[p[0]+n[0]*px_dist,p[1]+n[1]*px_dist];
        let Some((x,y,key))=pixel_at(q,w,h) else{continue};
        let existing=rgba.get_pixel(x,y);
        if existing[3]<255 {continue}
        let weight=1.0-dist as f64/(radius as f64+0.5);
        let item=all.entry(key).or_insert(Accumulator{rgb:[0.;3],weight:0.});
        for (j,v) in item.rgb.iter_mut().enumerate(){
            *v+=target[if color{j}else{0}]*weight;
        }
        item.weight+=weight;
    }
}
/// Works for source PBR RGBA maps. Normals always remain unchanged.
pub(crate) fn blend_seams(src:&RgbaImage, pairs:&[SeamPair],
    role:&str, radius:u32, mode:&str
) -> StudioResult<(RgbaImage,SeamMapReport,RgbaImage)> {
    if !matches!(role,"baseColor"|"metallic"|"roughness"|"occlusion") {
        return Err("Normal maps cannot be auto-blended across UV seams".into());
    }
    if !matches!(mode,"gentle"|"balanced") || !(1..=3).contains(&radius) {
        return Err("Seam mode/radius outside safe bounds".into());
    }
    let (w,h)=src.dimensions();
    if w<4 || h<4 || w>4096 || h>4096 || w as u64*h as u64>16_000_000 {
        return Err("Seam repair PNG dimensions outside safe range".into());
    }
    let color=role=="baseColor";
    let max_mismatch=if mode=="gentle" {
        if color{0.11}else{0.15}
    }else if color{0.18}else{0.24};
    let mut acc=Contributions::new();
    let mut considered=0usize;
    let mut skipped=0usize;
    let mut unchanged=0usize;
    for pair in pairs {
        let p0=uv_pixel(pair.left.start,w,h);
        let p1=uv_pixel(pair.left.end,w,h);
        let pixel_length=((p1[0]-p0[0]).powi(2)+(p1[1]-p0[1]).powi(2)).sqrt();
        let count=((pixel_length.ceil() as usize).max(2)).min(512);
        for i in 0..count {
            let t=(i as f64+0.5)/(count as f64);
            let Some((a,na))=inward(&pair.left,t,w,h) else{continue};
            let Some((b,nb))=inward(&pair.right,t,w,h) else{continue};
            let aa=[a[0]+na[0]*1.15,a[1]+na[1]*1.15];
            let bb=[b[0]+nb[0]*1.15,b[1]+nb[1]*1.15];
            let (Some((ax,ay,_)),Some((bx,by,_)))=
                (pixel_at(aa,w,h),pixel_at(bb,w,h)) else{continue};
            let ca=src.get_pixel(ax,ay);
            let cb=src.get_pixel(bx,by);
            if ca[3]!=255 || cb[3]!=255 {continue}
            let mismatch=channel_distance(ca,cb,color);
            if mismatch<0.015 {unchanged+=1;continue}
            if mismatch>max_mismatch {skipped+=1;continue}
            let target=[
                (ca[0] as f64+cb[0] as f64)/2.,
                (ca[1] as f64+cb[1] as f64)/2.,
                (ca[2] as f64+cb[2] as f64)/2.,
            ];
            considered+=1;
            contribute(&mut acc,&pair.left,t,src,target,radius,color);
            contribute(&mut acc,&pair.right,t,src,target,radius,color);
        }
    }
    let mut result=src.clone();
    let mut overlay=RgbaImage::new(w,h);
    let total=(w as usize)*(h as usize);
    let blend_strength=if mode=="gentle" {0.35}else{0.55};
    let mut modified=0usize;
    let mut delta_sum=0.0f64;
    let mut max_delta=0.0f64;
    for (idx,item) in acc {
        if item.weight<1e-8{continue}
        let x=idx%w as usize;
        let y=idx/w as usize;
        let prior=src.get_pixel(x as u32,y as u32);
        let mut after=*prior;
        // Blend only a narrow seam band; a pixel may receive multiple
        // adjacent samples but contributions are averaged, not accumulated
        // into runaway brightness/roughness/metallic changes.
        let amount=blend_strength*(item.weight/3.0).min(1.0);
        let n=if color{3}else{1};
        for k in 0..n {
            let desired=item.rgb[k]/item.weight;
            let mixed=prior[k] as f64*(1.0-amount)+desired*amount;
            after[k]=mixed.round().clamp(0.,255.) as u8;
        }
        if !color {
            after[1]=after[0];
            after[2]=after[0];
        }
        if after==*prior {continue}
        let diff=channel_distance(prior,&after,color);
        modified+=1;
        delta_sum+=diff;
        max_delta=max_delta.max(diff);
        result.put_pixel(x as u32,y as u32,after);
        overlay.put_pixel(x as u32,y as u32,Rgba([
            252,140,42,(diff*1024.).round().clamp(40.,240.) as u8
        ]));
    }
    let fraction=modified as f64/total as f64;
    let mean=if modified>0 {delta_sum/modified as f64}else{0.0};
    // Fail closed on broad unexpected edits or unstable high-contrast changes.
    if fraction>0.080001 || mean>0.085001 || max_delta>0.220001 {
        return Err(format!(
            "Seam smoothing exceeded safety gate: {role} pixels={modified}/{total}, \
             fraction={fraction:.4}, meanDiff={mean:.4}, maxDiff={max_delta:.4}"
        ));
    }
    Ok((result,SeamMapReport{
        role:role.into(),source_width:w,source_height:h,
        eligible_samples:considered,
        large_difference_samples_skipped:skipped,
        unchanged_samples:unchanged,
        changed_pixels:modified,changed_fraction:(fraction*100000.0).round()/100000.0,
        mean_pixel_delta:(mean*100000.0).round()/100000.0,
        maximum_pixel_delta:(max_delta*100000.0).round()/100000.0,
        scope:"3D-matched-UV-seams-only; original UV/mesh/normal preserved".into(),
    },overlay))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn matches_similar_seam_and_rejects_hard_mismatch(){
        let side_a=SeamSide{start:[0.15,0.20],end:[0.15,0.80],inside:[0.40,0.50],normal:[0.,0.,1.]};
        let side_b=SeamSide{start:[0.65,0.20],end:[0.65,0.80],inside:[0.90,0.50],normal:[0.,0.,1.]};
        let pair=SeamPair{left:side_a,right:side_b};
        let mut image=RgbaImage::from_pixel(64,64,Rgba([100,100,100,255]));
        for y in 0..64 {for x in 32..64{
            image.put_pixel(x,y,Rgba([120,120,120,255]));
        }}
        let (out,report,_)=blend_seams(&image,&[pair.clone()],"baseColor",2,"balanced").unwrap();
        assert!(report.changed_pixels>0);
        assert_eq!(out.dimensions(),image.dimensions());
        let (far,blocked,_)=blend_seams(&{
            let mut high=image.clone();
            for y in 0..64{for x in 32..64{
                high.put_pixel(x,y,Rgba([250,250,250,255]));
            }}
            high
        },&[pair],"baseColor",2,"gentle").unwrap();
        assert_eq!(far.width(),64);
        assert_eq!(blocked.eligible_samples,0);
    }
    #[test]fn normal_role_always_rejected(){
        assert!(blend_seams(&RgbaImage::new(32,32),&[],"normal",2,"balanced").is_err());
    }
}
