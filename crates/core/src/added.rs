//! Ce que uehub a ajouté à un projet, et en quelle version : une fiche par asset dans
//! Saved/uehub/assets.json, à l'intérieur du projet. En la comparant à la bibliothèque
//! Fab, on sait si une version plus récente est sortie.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use egs_api::api::types::fab_library::{FabAsset, ProjectVersion};
use serde::{Deserialize, Serialize};

use crate::download::{build_of, newest_engine, pick_for_project};
use crate::{Result, parse_version};

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

/// Les fiches d'un projet.
pub struct ProjectAssets {
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
        Self { file, assets }
    }

    pub fn list(&self) -> &[AddedAsset] {
        &self.assets
    }

    /// Range la fiche (elle remplace celle du même asset), puis enregistre.
    pub fn record(&mut self, added: AddedAsset) -> Result<()> {
        self.assets.retain(|a| a.asset_id != added.asset_id);
        self.assets.push(added);
        self.save()
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