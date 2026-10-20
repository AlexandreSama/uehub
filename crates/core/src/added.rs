//! Ce que uehub a ajouté à un projet, et en quelle version : une fiche par asset dans
//! Saved/uehub/assets.json, à l'intérieur du projet. En la comparant à la bibliothèque
//! Fab, on sait si une version plus récente est sortie.
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use egs_api::api::types::fab_library::{FabAsset, ProjectVersion};
use serde::{Deserialize, Serialize};

use crate::download::{build_of, newest_engine, pick_for_project};
use crate::{CoreError, Result, parse_version};

/// Le fichier, relatif au dossier du projet. Saved/ est le dossier des données locales
/// d'un projet Unreal : les modèles de .gitignore d'Unreal l'excluent déjà.
const FILE: &str = "Saved/uehub/assets.json";

/// Un asset ajouté à un projet par uehub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedAsset {
    pub asset_id: String,
    pub title: String,
    /// La version de l'asset installée : Fab en a une par groupe de versions d'Unreal
    pub artifact_id: String,
    /// Le numéro de build de cette version au moment de l'ajout ; il change quand
    /// le vendeur publie une nouvelle build
    pub build_version: String,
    /// La version d'Unreal la plus récente que vise cette version de l'asset, ex. "5.3"
    pub engine: String,
    /// Date de l'ajout, en secondes depuis 1970
    pub added_at: u64,
    /// Les fichiers écrits, relatifs au dossier du projet
    pub files: Vec<String>,
}

impl AddedAsset {
    /// La fiche d'une version d'asset qu'on s'apprête à ajouter. `files` est rempli
    /// une fois les fichiers écrits.
    pub fn new(asset: &FabAsset, version: &ProjectVersion) -> Self {
        let (major, minor) = newest_engine(version);
        Self {
            asset_id: asset.asset_id.clone(),
            title: asset.title.clone(),
            artifact_id: version.artifact_id.clone(),
            build_version: build_of(version),
            engine: format!("{major}.{minor}"),
            added_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            files: Vec::new(),
        }
    }
}

/// Pourquoi un asset déjà ajouté peut être mis à jour.
#[derive(Debug, Clone, PartialEq)]
pub enum AssetUpdate {
    /// Le vendeur a publié une nouvelle build de la version installée
    NewBuild,
    /// Une version plus proche du moteur du projet existe, ex. "5.4" au lieu de "5.3"
    CloserEngine(String),
}

/// Compare une fiche à l'asset tel que Fab le décrit aujourd'hui. On choisit la version
/// comme pour un ajout (`pick_for_project`) : si ce n'est plus celle qui est installée,
/// ou si sa build a changé, il y a une mise à jour.
pub fn update_for(
    added: &AddedAsset,
    asset: &FabAsset,
    project_engine: (u32, u32),
) -> Option<AssetUpdate> {
    let (version, _) = pick_for_project(asset, project_engine)?;
    if version.artifact_id != added.artifact_id {
        let had = parse_version(&added.engine)?;
        let now = newest_engine(version);
        return match now.cmp(&had) {
            std::cmp::Ordering::Greater => {
                Some(AssetUpdate::CloserEngine(format!("{}.{}", now.0, now.1)))
            }
            // Même version d'Unreal sous un autre identifiant : une nouvelle publication.
            std::cmp::Ordering::Equal => Some(AssetUpdate::NewBuild),
            // Le vendeur a retiré la version installée : rien de mieux à proposer.
            std::cmp::Ordering::Less => None,
        };
    }
    let build = build_of(version);
    (!added.build_version.is_empty() && build != added.build_version)
        .then_some(AssetUpdate::NewBuild)
}

/// Ce qu'un retrait a fait sur le disque.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Removal {
    /// Fichiers effacés
    pub deleted: usize,
    /// Fichiers gardés : un autre asset du projet les a aussi installés
    pub shared: usize,
    /// Fichiers déjà absents (effacés à la main, par exemple)
    pub missing: usize,
}

/// Les fiches d'un projet.
pub struct ProjectAssets {
    /// Le dossier du projet : les chemins des fiches partent de lui
    dir: PathBuf,
    file: PathBuf,
    assets: Vec<AddedAsset>,
}

impl ProjectAssets {
    /// Lit les fiches du projet. Pas de fichier (rien n'a encore été ajouté), ou un
    /// fichier abîmé : la liste est vide.
    pub fn load(project_dir: &Path) -> Self {
        let file = project_dir.join(FILE);
        let assets = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            dir: project_dir.to_path_buf(),
            file,
            assets,
        }
    }

    pub fn list(&self) -> &[AddedAsset] {
        &self.assets
    }

    /// Range la fiche (elle remplace celle du même asset), puis enregistre. Pour une mise
    /// à jour, les fichiers de l'ancienne version que la nouvelle n'a plus sont effacés.
    pub fn record(&mut self, added: AddedAsset) -> Result<Removal> {
        let stale: Vec<String> = match self.take(&added.asset_id) {
            Some(old) => {
                let kept: HashSet<&str> = added.files.iter().map(String::as_str).collect();
                old.files
                    .into_iter()
                    .filter(|f| !kept.contains(f.as_str()))
                    .collect()
            }
            None => Vec::new(),
        };
        let removal = self.delete(&stale)?;
        self.assets.push(added);
        self.save()?;
        Ok(removal)
    }

    /// Retire un asset du projet : efface ses fichiers, sauf ceux qu'un autre asset a
    /// aussi installés, puis sa fiche. Les dossiers qu'il laisse vides disparaissent avec.
    pub fn remove(&mut self, asset_id: &str) -> Result<Removal> {
        let added = self.take(asset_id).ok_or_else(|| {
            CoreError::Project("cet asset n'a pas été ajouté à ce projet par uehub".into())
        })?;
        let removal = self.delete(&added.files)?;
        self.save()?;
        Ok(removal)
    }

    /// Sort la fiche d'un asset de la liste.
    fn take(&mut self, asset_id: &str) -> Option<AddedAsset> {
        let i = self.assets.iter().position(|a| a.asset_id == asset_id)?;
        Some(self.assets.remove(i))
    }

    /// Efface ces fichiers du projet. Ceux qu'une autre fiche cite restent : deux packs
    /// d'un même vendeur installent souvent les mêmes textures.
    fn delete(&self, files: &[String]) -> Result<Removal> {
        let others: HashSet<&str> = self
            .assets
            .iter()
            .flat_map(|a| a.files.iter().map(String::as_str))
            .collect();
        let mut removal = Removal::default();
        for file in files {
            if others.contains(file.as_str()) {
                removal.shared += 1;
                continue;
            }
            // Un chemin qui sortirait du projet n'est jamais effacé : compté comme absent.
            let Some(path) = self.inside(file) else {
                removal.missing += 1;
                continue;
            };
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    removal.deleted += 1;
                    self.prune(&path);
                }
                Err(e) if e.kind() == ErrorKind::NotFound => removal.missing += 1,
                Err(e) => {
                    return Err(CoreError::Project(format!(
                        "impossible d'effacer {file} : {e}"
                    )));
                }
            }
        }
        Ok(removal)
    }

    /// Le chemin d'un fichier de la fiche, dans le projet. Une fiche abîmée ou modifiée à
    /// la main ne doit rien effacer ailleurs : seuls les chemins relatifs simples passent,
    /// sans `..` ni `/` au début.
    fn inside(&self, file: &str) -> Option<PathBuf> {
        let relative = Path::new(file);
        let simple = relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        (simple && !file.is_empty()).then(|| self.dir.join(relative))
    }

    /// Supprime les dossiers que l'effacement d'un fichier a laissés vides, en remontant.
    /// On s'arrête avant les dossiers du projet lui-même (Content, Plugins…).
    fn prune(&self, file: &Path) {
        let mut dir = file.parent();
        while let Some(d) = dir {
            if d == self.dir || d.parent() == Some(self.dir.as_path()) {
                break;
            }
            // Un dossier qui n'est pas vide refuse : c'est là qu'on s'arrête.
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }

    /// Écrit dans un fichier temporaire puis renomme : une coupure au mauvais moment
    /// ne laisse jamais un fichier à moitié écrit.
    fn save(&self) -> Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.assets)?)?;
        std::fs::rename(&tmp, &self.file)?;
        Ok(())
    }
}
