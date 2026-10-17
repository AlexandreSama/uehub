//! Téléchargement d'un asset Fab : manifest → chunks → fichiers.

use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use egs_api::api::types::chunk::Chunk;
use egs_api::api::types::download_manifest::{DownloadManifest, FileManifestList};
use egs_api::api::types::fab_library::{FabAsset, ProjectVersion};
use futures::stream::{self, StreamExt};
use sha1::{Digest, Sha1};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::{CoreError, Result};

// ---------------------------------------------------------------------------
// Choisir quoi télécharger
// ---------------------------------------------------------------------------

/// Cherche dans la bibliothèque par asset_id exact, titre exact ou morceau de titre.
pub fn find_assets<'a>(library: &'a [FabAsset], query: &str) -> Vec<&'a FabAsset> {
    let q = query.to_lowercase();
    if let Some(asset) = library.iter().find(|a| a.asset_id == query) {
        return vec![asset];
    }
    if let Some(asset) = library.iter().find(|a| a.title.to_lowercase() == q) {
        return vec![asset];
    }
    library
        .iter()
        .filter(|a| a.title.to_lowercase().contains(&q))
        .collect()
}

/// Choisit la version compatible avec `ue` ("5.4"), ou la plus récente si `ue` est absent.
pub fn pick_version<'a>(asset: &'a FabAsset, ue: Option<&str>) -> Option<&'a ProjectVersion> {
    match ue {
        Some(v) => {
            let wanted = format!("UE_{}", v.trim_start_matches("UE_"));
            asset
                .project_versions
                .iter()
                .find(|p| p.engine_versions.contains(&wanted))
        }
        None => asset
            .project_versions
            .iter()
            .max_by_key(|p| newest_engine(p)),
    }
}

/// Nom de dossier tiré du titre : "Megascans: Grass" → "Megascans_Grass".
pub fn folder_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    cleaned
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn newest_engine(version: &ProjectVersion) -> (u32, u32) {
    version
        .engine_versions
        .iter()
        .filter_map(|v| parse_ue(v))
        .max()
        .unwrap_or((0, 0))
}

fn parse_ue(v: &str) -> Option<(u32, u32)> {
    let (major, minor) = v.strip_prefix("UE_")?.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

// ---------------------------------------------------------------------------
// Plan de téléchargement
// ---------------------------------------------------------------------------

/// Avancement, envoyé à l'interface (barre de progression).
#[derive(Debug, Clone, Copy)]
pub enum Progress {
    /// `done` chunks téléchargés sur `total`
    Chunks { done: usize, total: usize },
    /// `done` fichiers écrits sur `total`
    Files { done: usize, total: usize },
}

/// Tout ce qu'il faut savoir pour télécharger un asset, tiré du manifest.
pub struct DownloadPlan {
    files: Vec<FileManifestList>,
    /// guid du chunk → URL sur le CDN (chaque chunk n'est téléchargé qu'une fois)
    chunks: HashMap<String, String>,
}

impl DownloadPlan {
    pub fn from_manifest(manifest: &DownloadManifest) -> Self {
        let mut files: Vec<FileManifestList> = manifest.files().into_values().collect();
        files.sort_by(|a, b| a.filename.cmp(&b.filename));

        let mut chunks = HashMap::new();
        for part in files.iter().flat_map(|f| &f.file_chunk_parts) {
            if let Some(link) = &part.link {
                chunks
                    .entry(part.guid.clone())
                    .or_insert_with(|| link.to_string());
            }
        }
        Self { files, chunks }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Taille finale de l'asset sur le disque, en octets.
    pub fn install_size(&self) -> u64 {
        self.files.iter().map(|f| f.size() as u64).sum()
    }

    /// Télécharge tous les chunks dans `cache`, `jobs` à la fois.
    /// Un chunk déjà présent dans le cache est sauté : c'est ce qui permet la reprise.
    pub async fn download_chunks(
        &self,
        http: &reqwest::Client,
        cache: &Path,
        jobs: usize,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<()> {
        fs::create_dir_all(cache).await?;
        let total = self.chunks.len();
        on_progress(Progress::Chunks { done: 0, total });

        // Chaque tâche possède ses données (des clones) : le futur reste `Send`,
        // ce qu'exige Tauri pour l'exécuter sur son runtime multi-thread.
        let mut tasks = stream::iter(self.chunks.clone())
            .map(|(guid, url)| {
                let http = http.clone();
                let cache = cache.to_path_buf();
                async move { fetch_chunk(&http, &cache, &guid, &url).await }
            })
            .buffer_unordered(jobs.max(1));

        let mut done = 0;
        while let Some(result) = tasks.next().await {
            result?;
            done += 1;
            on_progress(Progress::Chunks { done, total });
        }
        Ok(())
    }

    /// Reconstruit chaque fichier à partir des chunks du cache et vérifie son SHA-1.
    pub async fn assemble_files(
        &self,
        cache: &Path,
        out_dir: &Path,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<()> {
        let total = self.files.len();
        for (i, file) in self.files.iter().enumerate() {
            let target = safe_join(out_dir, &file.filename)?;
            if let Err(e) = write_file(file, cache, &target).await {
                if matches!(e, CoreError::HashMismatch(_)) {
                    // Un chunk du cache est sans doute abîmé : on le jette pour la prochaine fois.
                    for part in &file.file_chunk_parts {
                        let _ = fs::remove_file(chunk_path(cache, &part.guid)).await;
                    }
                }
                return Err(e);
            }
            on_progress(Progress::Files { done: i + 1, total });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Chunks
// ---------------------------------------------------------------------------

fn chunk_path(cache: &Path, guid: &str) -> PathBuf {
    cache.join(format!("{guid}.bin"))
}

/// Télécharge un chunk (3 essais), le décode et enregistre ses données décompressées.
async fn fetch_chunk(http: &reqwest::Client, cache: &Path, guid: &str, url: &str) -> Result<()> {
    let path = chunk_path(cache, guid);
    if fs::try_exists(&path).await? {
        return Ok(());
    }

    let mut last_error = CoreError::Download(format!("chunk {guid}"));
    for attempt in 1..=3 {
        match get_and_decode(http, url).await {
            Ok(data) => {
                // Écriture dans un fichier temporaire puis renommage : jamais de chunk à moitié écrit.
                let tmp = path.with_extension("part");
                fs::write(&tmp, &data).await?;
                fs::rename(&tmp, &path).await?;
                return Ok(());
            }
            Err(e) => {
                log::warn!("chunk {guid}, essai {attempt}/3 : {e}");
                last_error = e;
                tokio::time::sleep(Duration::from_secs(attempt)).await;
            }
        }
    }
    Err(last_error)
}

async fn get_and_decode(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let bytes = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    // Décompression zlib : travail CPU, on le sort du runtime async.
    let chunk = tokio::task::spawn_blocking(move || Chunk::from_vec(bytes))
        .await
        .map_err(|e| CoreError::Download(e.to_string()))?
        .ok_or_else(|| CoreError::Download(format!("chunk illisible : {url}")))?;
    Ok(chunk.data)
}

// ---------------------------------------------------------------------------
// Fichiers
// ---------------------------------------------------------------------------

/// Refuse les chemins qui sortiraient du dossier cible (`../`, chemins absolus).
fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.components().all(|c| matches!(c, Component::Normal(_))) {
        Ok(root.join(rel))
    } else {
        Err(CoreError::Download(format!("chemin refusé : {relative}")))
    }
}

async fn write_file(file: &FileManifestList, cache: &Path, target: &Path) -> Result<()> {
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).await?;
    }
    let mut tmp = target.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    let mut out = fs::File::create(&tmp).await?;
    let mut hasher = Sha1::new();

    for part in &file.file_chunk_parts {
        let mut chunk = fs::File::open(chunk_path(cache, &part.guid)).await?;
        chunk.seek(SeekFrom::Start(part.offset as u64)).await?;
        let mut buf = vec![0u8; part.size as usize];
        chunk.read_exact(&mut buf).await?;
        hasher.update(&buf);
        out.write_all(&buf).await?;
    }
    out.flush().await?;
    drop(out);

    let got: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !got.eq_ignore_ascii_case(&file.file_hash) {
        fs::remove_file(&tmp).await?;
        return Err(CoreError::HashMismatch(file.filename.clone()));
    }
    fs::rename(&tmp, target).await?;
    Ok(())
}