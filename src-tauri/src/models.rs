use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::{fs, io::AsyncWriteExt};

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub file: String,
    pub installed: bool,
    pub size_bytes: Option<u64>,
    pub min_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
    model: String,
    percent: f64,
    bytes: u64,
    total: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct ModelDef {
    id: &'static str,
    name: &'static str,
    file: &'static str,
    min_bytes: u64,
}

const MODELS: &[ModelDef] = &[
    ModelDef {
        id: "tiny",
        name: "Tiny",
        file: "ggml-tiny.bin",
        min_bytes: 75_000_000,
    },
    ModelDef {
        id: "base",
        name: "Base",
        file: "ggml-base.bin",
        min_bytes: 145_000_000,
    },
    ModelDef {
        id: "small",
        name: "Small",
        file: "ggml-small.bin",
        min_bytes: 480_000_000,
    },
];

pub fn model_path(app_dir: &Path, id: &str) -> anyhow::Result<PathBuf> {
    let def = model_def(id)?;
    Ok(app_dir.join("models").join(def.file))
}

pub fn statuses(app_dir: &Path) -> anyhow::Result<Vec<ModelStatus>> {
    MODELS
        .iter()
        .map(|def| {
            let path = app_dir.join("models").join(def.file);
            let size_bytes = std::fs::metadata(&path).ok().map(|meta| meta.len());
            Ok(ModelStatus {
                id: def.id.to_string(),
                name: def.name.to_string(),
                file: def.file.to_string(),
                installed: size_bytes.is_some_and(|size| size >= def.min_bytes),
                size_bytes,
                min_bytes: def.min_bytes,
            })
        })
        .collect()
}

pub fn is_installed(app_dir: &Path, id: &str) -> anyhow::Result<bool> {
    let def = model_def(id)?;
    let status = status_for(app_dir, def)?;
    Ok(status.installed)
}

pub async fn download(app: AppHandle, app_dir: PathBuf, id: String) -> anyhow::Result<ModelStatus> {
    let def = model_def(&id)?;
    let models_dir = app_dir.join("models");
    fs::create_dir_all(&models_dir).await?;

    let final_path = models_dir.join(def.file);
    if let Ok(meta) = fs::metadata(&final_path).await {
        if meta.len() >= def.min_bytes {
            return status_for(&app_dir, def);
        }
    }

    let part_path = final_path.with_extension("bin.part");
    let _ = fs::remove_file(&part_path).await;

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        def.file
    );
    let response = reqwest::get(url).await?.error_for_status()?;
    let total = response.content_length();
    let mut stream = response.bytes_stream();
    let mut file = fs::File::create(&part_path).await?;
    let mut written = 0_u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;
        let percent = total
            .map(|total| (written as f64 / total as f64) * 100.0)
            .unwrap_or(0.0);
        let _ = app.emit(
            "model-download-progress",
            DownloadProgress {
                model: id.clone(),
                percent,
                bytes: written,
                total,
            },
        );
    }

    file.flush().await?;
    drop(file);

    if let Some(total) = total {
        if written != total {
            let _ = fs::remove_file(&part_path).await;
            return Err(anyhow::anyhow!(
                "Downloaded {written} bytes, expected {total} bytes"
            ));
        }
    }
    if written < def.min_bytes {
        let _ = fs::remove_file(&part_path).await;
        return Err(anyhow::anyhow!("Downloaded model is smaller than expected"));
    }

    fs::rename(&part_path, &final_path).await?;
    status_for(&app_dir, def)
}

fn status_for(app_dir: &Path, def: ModelDef) -> anyhow::Result<ModelStatus> {
    let path = app_dir.join("models").join(def.file);
    let size_bytes = std::fs::metadata(&path).ok().map(|meta| meta.len());
    Ok(ModelStatus {
        id: def.id.to_string(),
        name: def.name.to_string(),
        file: def.file.to_string(),
        installed: size_bytes.is_some_and(|size| size >= def.min_bytes),
        size_bytes,
        min_bytes: def.min_bytes,
    })
}

fn model_def(id: &str) -> anyhow::Result<ModelDef> {
    MODELS
        .iter()
        .copied()
        .find(|model| model.id == id)
        .ok_or_else(|| anyhow::anyhow!("Unknown model: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_accepts_known_model_ids_only() {
        let root = Path::new("/tmp/mim-dictate-test");
        assert_eq!(
            model_path(root, "tiny").unwrap(),
            root.join("models").join("ggml-tiny.bin")
        );
        assert!(model_path(root, "../tiny").is_err());
    }

    #[test]
    fn model_definitions_have_size_thresholds() {
        for model in MODELS {
            assert!(model.min_bytes > 1_000_000);
            assert!(model.file.ends_with(".bin"));
        }
    }
}
