//! Les projets Unreal utilisés récemment, partagés par la CLI et l'app.
//! Rangés dans ~/.local/share/uehub/recent_projects.json.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::{CoreError, Result};

/// Nombre de projets gardés dans la liste.
const MAX: usize = 8;

pub struct RecentProjects {
    file: PathBuf,
    /// Chemins des fichiers .uproject, le plus récent en premier
    paths: Vec<PathBuf>,
}

impl RecentProjects {
    /// Charge la liste. Les projets qui n'existent plus sont ignorés,
    /// et un fichier abîmé donne simplement une liste vide.
    pub fn load() -> Result<Self> {
        let dirs = ProjectDirs::from("", "", "uehub").ok_or(CoreError::NoDataDir)?;
        let file = dirs.data_dir().join("recent_projects.json");
        Ok(Self::load_from(file))
    }

    fn load_from(file: PathBuf) -> Self {
        let paths: Vec<PathBuf> = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let paths = paths.into_iter().filter(|p| p.is_file()).collect();
        Self { file, paths }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Met le projet en tête de liste, puis enregistre.
    pub fn remember(&mut self, uproject: &Path) -> Result<()> {
        // Chemin absolu et sans `..` : le même projet n'apparaît qu'une fois.
        let path = uproject.canonicalize()?;
        self.paths.retain(|p| *p != path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX);
        self.save()
    }

    /// Retire le projet de la liste, puis enregistre.
    pub fn forget(&mut self, uproject: &Path) -> Result<()> {
        self.paths.retain(|p| p != uproject);
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_vec_pretty(&self.paths)?)?;
        Ok(())
    }
}