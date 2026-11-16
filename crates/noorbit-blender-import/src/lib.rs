use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum ImportError { #[error("glb no existeix: {0}")] NotFound(PathBuf), #[error("path invàlid: {0}")] BadPath(String), #[error("E/S: {0}")] Io(#[from] std::io::Error) }
pub type Result<T> = std::result::Result<T, ImportError>;

pub struct BlenderImporter { project_path: PathBuf, scenes_dir: String, assets_dir: String }
impl BlenderImporter {
    pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into(), scenes_dir: "res://scenes".into(), assets_dir: "res://assets".into() } }
    pub fn with_scenes_dir(mut self, dir: impl Into<String>) -> Self { self.scenes_dir = dir.into(); self }
    pub fn with_assets_dir(mut self, dir: impl Into<String>) -> Self { self.assets_dir = dir.into(); self }
    pub async fn import(&self, task_id: &str) -> Result<String> {
        if task_id.is_empty() || task_id.contains('/') || task_id.contains('\\') || task_id.contains("..") { return Err(ImportError::BadPath(format!("task_id invàlid: {task_id}"))); }
        let glb_rel = format!("{}/{}.glb", self.assets_dir, task_id);
        let glb_fs = self.resolve_res(&glb_rel)?;
        if !glb_fs.exists() { return Err(ImportError::NotFound(glb_fs)); }
        let tscn_rel = format!("{}/{}.tscn", self.scenes_dir, task_id);
        let tscn_fs = self.resolve_res(&tscn_rel)?;
        let tscn = format!("[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"PackedScene\" path=\"{glb_rel}\" id=\"1_glb\"]\n\n[node name=\"{task_id}\" type=\"Node3D\"]\n\n[node name=\"Model\" parent=\".\" instance=ExtResource(\"1_glb\")]\n");
        if let Some(parent) = tscn_fs.parent() { tokio::fs::create_dir_all(parent).await?; }
        tokio::fs::write(&tscn_fs, tscn.as_bytes()).await?;
        info!(task = task_id, "glb importat");
        Ok(tscn_rel)
    }
    fn resolve_res(&self, res_path: &str) -> Result<PathBuf> {
        let rel = res_path.strip_prefix("res://").ok_or_else(|| ImportError::BadPath(format!("no comença amb res://: {res_path}")))?;
        let p = Path::new(rel);
        if p.is_absolute() { return Err(ImportError::BadPath(format!("path absolut: {res_path}"))); }
        for comp in p.components() { if matches!(comp, Component::ParentDir) { return Err(ImportError::BadPath(format!("path amb ..: {res_path}"))); } }
        Ok(self.project_path.join(p))
    }
}
pub struct BlenderImportVerifier { #[allow(dead_code)] project_path: PathBuf, scenes_dir: String }
impl BlenderImportVerifier {
    pub fn new(project_path: impl Into<PathBuf>) -> Self { Self { project_path: project_path.into(), scenes_dir: "res://scenes".into() } }
    pub fn with_scenes_dir(mut self, dir: impl Into<String>) -> Self { self.scenes_dir = dir.into(); self }
}
#[allow(unused)]
fn _warn() { let _ = warn!(""); }

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn importa_requereix_glb() {
        let dir = TempDir::new().unwrap();
        let imp = BlenderImporter::new(dir.path());
        let r = imp.import("t01").await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn rebutja_task_id_amb_dot_dot() {
        let dir = TempDir::new().unwrap();
        let imp = BlenderImporter::new(dir.path());
        let r = imp.import("../evil").await;
        assert!(matches!(r, Err(ImportError::BadPath(_))));
    }

    #[test]
    fn resolve_res_rebutja_path_absolut() {
        let imp = BlenderImporter::new("/tmp/proj");
        let r = imp.resolve_res("/etc/passwd");
        assert!(r.is_err());
    }

    #[test]
    fn resolve_res_rebutja_dot_dot() {
        let imp = BlenderImporter::new("/tmp/proj");
        let r = imp.resolve_res("res://../../etc/passwd");
        assert!(r.is_err());
    }
}
