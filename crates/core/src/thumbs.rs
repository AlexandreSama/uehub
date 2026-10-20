//! Cache disque des vignettes Fab : ~/.cache/uehub/thumbs/<SHA-1 de l'URL>.
//! Une vignette est téléchargée une seule fois, ensuite elle est lue sur le disque.

use std::path::PathBuf;

use directories::ProjectDirs;
use sha1::{Digest, Sha1};

use crate::{CoreError, Result, t};

pub struct ThumbCache {
    dir: PathBuf,
    http: reqwest::Client,
    /// Seules les images de Fab passent : le cache ne télécharge pas n'importe quoi.
    allowed: &'static str,
}

impl ThumbCache {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "uehub").ok_or(CoreError::NoDataDir)?;
        Ok(Self {
            dir: dirs.cache_dir().join("thumbs"),
            http: crate::http_client()?,
            allowed: "https://media.fab.com/",
        })
    }

    /// Les octets de l'image : depuis le disque si elle y est, sinon depuis Fab.
    pub async fn get(&self, url: &str) -> Result<Vec<u8>> {
        if !url.starts_with(self.allowed) {
            return Err(CoreError::Download(t!("errors.thumb_refused", url = url)));
        }
        let path = self.dir.join(file_name(url));
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(bytes);
        }

        let bytes = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        // Écriture dans un fichier temporaire puis renommage : jamais d'image à moitié écrite.
        tokio::fs::create_dir_all(&self.dir).await?;
        let part = path.with_extension("part");
        tokio::fs::write(&part, &bytes).await?;
        tokio::fs::rename(&part, &path).await?;
        Ok(bytes.to_vec())
    }
}

/// Un nom de fichier stable et sans caractères spéciaux, tiré de l'URL.
fn file_name(url: &str) -> String {
    Sha1::digest(url.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
