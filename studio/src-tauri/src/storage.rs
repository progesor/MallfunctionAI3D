use crate::models::{Asset, StudioSettings};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::{Path, PathBuf}, sync::Mutex};
use tauri::Manager;
use uuid::Uuid;

pub struct StudioState {
    pub app_dir: PathBuf,
    pub db: Mutex<Connection>,
    pub settings: Mutex<StudioSettings>,
    pub client: reqwest::Client,
}

pub type StudioResult<T> = Result<T, String>;
fn ioerr(e: impl ToString) -> String { e.to_string() }

impl StudioState {
    pub fn setup(app: &tauri::AppHandle) -> StudioResult<Self> {
        let app_dir = app.path().app_data_dir().map_err(ioerr)?;
        fs::create_dir_all(&app_dir).map_err(ioerr)?;
        let settings_path = app_dir.join("settings.json");
        let settings = if settings_path.is_file() {
            serde_json::from_slice(&fs::read(&settings_path).map_err(ioerr)?).map_err(ioerr)?
        } else { StudioSettings::default() };
        let conn = Connection::open(app_dir.join("catalog.sqlite3")).map_err(ioerr)?;
        conn.execute_batch("PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS assets (
              id TEXT PRIMARY KEY, title TEXT NOT NULL, engine TEXT NOT NULL,
              station TEXT NOT NULL, created_at TEXT NOT NULL,
              file_size INTEGER NOT NULL, triangle_count INTEGER NOT NULL,
              vertex_count INTEGER NOT NULL, material_count INTEGER NOT NULL,
              texture_count INTEGER NOT NULL, sha256 TEXT NOT NULL, file_path TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS jobs (
              prompt_id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL,
              status TEXT NOT NULL, started_at TEXT NOT NULL, asset_id TEXT,
              message TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_assets_created ON assets(created_at DESC);")
            .map_err(ioerr)?;
        Ok(Self { app_dir, db:Mutex::new(conn), settings:Mutex::new(settings),
            client:reqwest::Client::builder().timeout(std::time::Duration::from_secs(40))
                .build().map_err(ioerr)? })
    }
    pub fn current_settings(&self) -> StudioResult<StudioSettings> {
        self.settings.lock().map(|g|g.clone()).map_err(ioerr)
    }
    pub fn library_dir(&self) -> StudioResult<PathBuf> {
        let value = self.current_settings()?.library_root;
        let path = if value.trim().is_empty() { self.app_dir.join("library") } else { PathBuf::from(value) };
        fs::create_dir_all(&path).map_err(ioerr)?;
        Ok(path)
    }
    pub fn save_settings(&self, mut next: StudioSettings) -> StudioResult<StudioSettings> {
        let url = url::Url::parse(&next.comfy_url).map_err(|e|format!("Geçersiz ComfyUI adresi: {e}"))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() || !url.username().is_empty()
           || url.password().is_some() || url.fragment().is_some() || url.query().is_some() {
            return Err("ComfyUI adresinde yalnızca http(s) host/port ve isteğe bağlı path kullanılabilir.".into());
        }
        next.comfy_url = next.comfy_url.trim_end_matches('/').to_owned();
        if next.station.is_empty() || next.station.len() > 48 ||
           !next.station.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("İstasyon etiketi yalnızca harf, rakam, - veya _ içermeli.".into());
        }
        if !next.blender_path.is_empty() && !Path::new(&next.blender_path).is_file() {
            return Err("Blender.exe bulunamadı; tam yolu kontrol et.".into());
        }
        if !next.library_root.is_empty() {
            let p = Path::new(&next.library_root);
            if !p.is_absolute() { return Err("Arşiv yolu tam (absolute) bir klasör olmalı.".into()); }
            fs::create_dir_all(p).map_err(ioerr)?;
        }
        let data = serde_json::to_vec_pretty(&next).map_err(ioerr)?;
        fs::write(self.app_dir.join("settings.json"), data).map_err(ioerr)?;
        *self.settings.lock().map_err(ioerr)? = next.clone();
        Ok(next)
    }
    pub fn job_insert(&self, prompt_id: &str, workflow_id: &str) -> StudioResult<()> {
        self.db.lock().map_err(ioerr)?.execute(
            "INSERT INTO jobs(prompt_id,workflow_id,status,started_at,message) VALUES(?1,?2,'queued',?3,'')",
            params![prompt_id,workflow_id,Utc::now().to_rfc3339()]).map_err(ioerr)?;
        Ok(())
    }
    pub fn job_update(&self, prompt_id: &str, status: &str, asset_id: Option<&str>, message: &str) -> StudioResult<()> {
        self.db.lock().map_err(ioerr)?.execute(
            "UPDATE jobs SET status=?2,asset_id=COALESCE(?3,asset_id),message=?4 WHERE prompt_id=?1",
            params![prompt_id,status,asset_id,message]).map_err(ioerr)?;
        Ok(())
    }
    pub fn job_asset(&self, prompt_id: &str) -> StudioResult<Option<Asset>> {
        let db=self.db.lock().map_err(ioerr)?;
        let mut stmt=db.prepare("SELECT a.id,a.title,a.engine,a.station,a.created_at,a.file_size,a.triangle_count,a.vertex_count,a.material_count,a.texture_count,a.sha256,a.file_path FROM assets a JOIN jobs j ON j.asset_id=a.id WHERE j.prompt_id=?1").map_err(ioerr)?;
        let mut rows=stmt.query(params![prompt_id]).map_err(ioerr)?;
        match rows.next().map_err(ioerr)? {
            Some(row)=>Ok(Some(row_to_asset(row).map_err(ioerr)?)), None=>Ok(None),
        }
    }
    pub fn asset_by_id(&self, id: &str) -> StudioResult<Asset> {
        let db=self.db.lock().map_err(ioerr)?;
        db.query_row("SELECT id,title,engine,station,created_at,file_size,triangle_count,vertex_count,material_count,texture_count,sha256,file_path FROM assets WHERE id=?1",
            params![id],row_to_asset).map_err(ioerr)
    }
    pub fn list_assets(&self) -> StudioResult<Vec<Asset>> {
        let db=self.db.lock().map_err(ioerr)?;
        let mut stmt=db.prepare("SELECT id,title,engine,station,created_at,file_size,triangle_count,vertex_count,material_count,texture_count,sha256,file_path FROM assets ORDER BY created_at DESC LIMIT 400").map_err(ioerr)?;
        let rows=stmt.query_map([],row_to_asset).map_err(ioerr)?;
        rows.map(|row|row.map_err(ioerr)).collect()
    }

    /// Soft-delete: move only a Studio-owned asset directory to our trash.
    /// This never deletes external input GLBs and allows manual recovery.
    pub fn trash_asset(&self, id: &str) -> StudioResult<PathBuf> {
        Uuid::parse_str(id).map_err(|_|"Geçersiz asset kimliği".to_owned())?;
        let asset=self.asset_by_id(id)?;
        let source=Path::new(&asset.file_path);
        let folder=source.parent().ok_or("Arşiv yolu geçersiz")?;
        // Validate structural ownership, including the stored original filename.
        if source.file_name().and_then(|x|x.to_str())!=Some("original.glb")
            || folder.file_name().and_then(|x|x.to_str())!=Some(id)
            || folder.parent().and_then(|p|p.file_name()).and_then(|x|x.to_str())!=Some("assets") {
            return Err("Yalnızca Studio arşivinin kendi GLB kopyaları kaldırılabilir.".into());
        }
        let canonical=folder.canonicalize().map_err(|e|e.to_string())?;
        let assets_parent=folder.parent().ok_or("Arşiv kökü geçersiz")?.canonicalize().map_err(|e|e.to_string())?;
        if canonical.parent()!=Some(assets_parent.as_path()) || !canonical.join("original.glb").is_file() {
            return Err("Arşiv kaynak dosyası doğrulanamadı.".into());
        }
        let root=assets_parent.parent().ok_or("Arşiv üst klasörü geçersiz")?;
        let trash=root.join("trash");
        fs::create_dir_all(&trash).map_err(|e|e.to_string())?;
        let destination=trash.join(id);
        if destination.exists() {return Err("Çöp arşivinde aynı asset mevcut; üzerine yazılmıyor.".into());}
        fs::rename(&canonical,&destination).map_err(|e|format!("Çöp arşivine taşınamadı: {e}"))?;
        let result=(|| -> StudioResult<()> {
            let mut db=self.db.lock().map_err(ioerr)?;
            let tx=db.transaction().map_err(ioerr)?;
            tx.execute("UPDATE jobs SET asset_id=NULL WHERE asset_id=?1",params![id]).map_err(ioerr)?;
            if tx.execute("DELETE FROM assets WHERE id=?1",params![id]).map_err(ioerr)?!=1 {
                return Err("Asset veritabanı kaydı bulunamadı.".into());
            }
            tx.commit().map_err(ioerr)?;
            Ok(())
        })();
        if let Err(error)=result {
            if let Err(restore_error)=fs::rename(&destination,&canonical) {
                return Err(format!("Veritabanı işlemi başarısız: {error}. Çöp klasöründen geri taşıma da başarısız: {restore_error}. Dosya: {}",destination.display()));
            }
            return Err(error);
        }
        Ok(destination)
    }

    pub fn store_asset(&self, content: &[u8], title: &str, engine: &str, provenance: Value) -> StudioResult<Asset> {
        if content.len() > 250_000_000 { return Err("GLB 250 MB üst sınırını aşıyor.".into()); }
        let (tris,verts,mats,tex)=inspect_glb(content)?;
        let settings=self.current_settings()?;
        let asset_id=Uuid::new_v4().to_string();
        let dir=self.library_dir()?.join("assets").join(&asset_id);
        fs::create_dir_all(&dir).map_err(ioerr)?;
        let output=dir.join("original.glb");
        // Write only to a newly allocated immutable asset folder. Never alter source.
        fs::write(&output,content).map_err(ioerr)?;
        let sha=format!("{:x}",Sha256::digest(content));
        let asset=Asset { id:asset_id, title:title.to_owned(), engine:engine.to_owned(),
            station:settings.station, created_at:Utc::now().to_rfc3339(),
            file_size:content.len() as i64, triangle_count:tris,
            vertex_count:verts, material_count:mats, texture_count:tex,
            sha256:sha,file_path:output.to_string_lossy().into_owned() };
        let manifest=json!({"asset":asset,"provenance":provenance,"immutable_source":true});
        fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest).map_err(ioerr)?).map_err(ioerr)?;
        let db=self.db.lock().map_err(ioerr)?;
        db.execute("INSERT INTO assets VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![
            asset.id,asset.title,asset.engine,asset.station,asset.created_at,asset.file_size,
            asset.triangle_count,asset.vertex_count,asset.material_count,asset.texture_count,asset.sha256,asset.file_path
        ]).map_err(ioerr)?;
        Ok(asset)
    }
}

fn row_to_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset { id:row.get(0)?, title:row.get(1)?, engine:row.get(2)?, station:row.get(3)?,
        created_at:row.get(4)?, file_size:row.get(5)?, triangle_count:row.get(6)?,
        vertex_count:row.get(7)?, material_count:row.get(8)?, texture_count:row.get(9)?,
        sha256:row.get(10)?,file_path:row.get(11)? })
}

pub fn inspect_glb(bytes: &[u8]) -> StudioResult<(u64,u64,u64,u64)> {
    if bytes.len()<20 || &bytes[0..4]!=b"glTF" || u32::from_le_bytes(bytes[4..8].try_into().unwrap())!=2 {
        return Err("GLB başlığı geçersiz (glTF 2.0 bekleniyor).".into());
    }
    if u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize!=bytes.len() {
        return Err("GLB byte boyutu başlıkla eşleşmiyor.".into());
    }
    let len=u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if &bytes[16..20]!=b"JSON" || len>bytes.len()-20 {
        return Err("GLB JSON chunk geçersiz.".into());
    }
    let data:Value=serde_json::from_slice(&bytes[20..20+len]).map_err(ioerr)?;
    let accessors=data.get("accessors").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut tris=0;
    let mut verts=0;
    for mesh in data.get("meshes").and_then(Value::as_array).into_iter().flatten() {
        for primitive in mesh.get("primitives").and_then(Value::as_array).into_iter().flatten() {
            let mode=primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
            if mode!=4 { continue; }
            let positions=primitive.get("attributes").and_then(|v|v.get("POSITION"))
                .and_then(Value::as_u64).and_then(|i| accessors.get(i as usize))
                .and_then(|v|v.get("count")).and_then(Value::as_u64).unwrap_or(0);
            verts+=positions;
            let indices=primitive.get("indices").and_then(Value::as_u64)
                .and_then(|i|accessors.get(i as usize))
                .and_then(|v|v.get("count")).and_then(Value::as_u64)
                .unwrap_or(positions);
            tris+=indices/3;
        }
    }
    if tris==0 { return Err("GLB içinde üçgen mesh bulunamadı.".into()); }
    Ok((tris,verts,data.get("materials").and_then(Value::as_array).map(|x|x.len() as u64).unwrap_or(0),
        data.get("textures").and_then(Value::as_array).map(|x|x.len() as u64).unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn reject_bad_glb() { assert!(inspect_glb(b"garbage").is_err()); }
    #[test] fn reject_short_glb() { assert!(inspect_glb(&[0;3]).is_err()); }
}
