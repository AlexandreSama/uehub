//! La dernière liste des moteurs d'Epic, gardée sur le disque dans
//! ~/.cache/uehub/engines.json : l'écran Moteurs l'affiche tout de suite, le temps de
//! relire la page Linux.
//!
//! Les adresses de téléchargement n'y sont pas. Signées et valables une heure, elles ne
//! servent qu'à l'installation, qui en demande toujours une fraîche à Epic.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use super::EngineDownload;
use crate::{CoreError, Result};

/// Un moteur de la liste : ce que l'écran affiche, sans l'adresse de téléchargement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedEngine {
    /// "5.8.3"
    pub version: String,
    /// Taille du zip en octets
    pub size: u64,
    /// Date de mise en ligne, ex. "2026-09-22T12:45:18.215Z"
    pub created_at: Option<String>,
}

impl From<&EngineDownload> for ListedEngine {
    fn from(download: &EngineDownload) -> Self {
        Self {
            version: download.version.clone(),
            size: download.size,
            created_at: download.created_at.clone(),
        }
    }
}

/// La liste des moteurs, et quand elle a été lue sur unrealengine.com.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineList {
    /// En secondes depuis 1970
    pub checked_at: u64,
    /// Du plus récent au plus ancien, comme sur la page
    pub engines: Vec<ListedEngine>,
}

impl EngineList {
    /// Une liste qu'on vient de lire, datée de maintenant.
    pub fn checked_now(engines: Vec<ListedEngine>) -> Self {
        let checked_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self {
            checked_at,
            engines,
        }
    }

    /// La liste gardée sur le disque. Pas encore de fichier, ou un fichier abîmé : rien.
    pub fn load() -> Option<Self> {
        Self::load_from(&file().ok()?)
    }

    /// Garde la liste pour le prochain affichage. Un échec n'empêche rien : l'écran
    /// attendra Epic, comme avant.
    pub fn save(&self) {
        if let Err(e) = file().and_then(|file| self.save_to(&file)) {
            log::warn!("liste des moteurs non gardée : {e}");
        }
    }

    /// Oublie la liste gardée : le compte n'a pas accepté le CLUF, Epic ne lui propose
    /// rien.
    pub fn forget() {
        if let Ok(file) = file() {
            // Pas de fichier : il n'y avait rien à oublier.
            let _ = std::fs::remove_file(file);
        }
    }

    fn load_from(file: &Path) -> Option<Self> {
        let bytes = std::fs::read(file).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Écrit dans un fichier temporaire puis renomme : jamais de liste à moitié écrite.
    fn save_to(&self, file: &Path) -> Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, file)?;
        Ok(())
    }
}

/// Un cache : l'effacer ne fait rien perdre, la liste revient au prochain affichage.
fn file() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "uehub").ok_or(CoreError::NoDataDir)?;
    Ok(dirs.cache_dir().join("engines.json"))
}