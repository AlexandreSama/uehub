//! Cœur de l'application : tout ce qui parle à Epic et touche au disque.
//! Aucune dépendance d'interface ici : la CLI et l'app Tauri partagent ce code.

mod download;
mod error;
mod project;
mod session;
mod thumbs;
mod recent;

use std::path::{Path, PathBuf};

pub use download::{
    DownloadPlan, Progress, find_assets, folder_name, pick_for_project, pick_version,
};
pub use egs_api::api::types::fab_library::{FabAsset, ProjectVersion};
pub use error::{CoreError, Result};
pub use project::{AssetKind, UnrealProject, parse_version};
pub use thumbs::ThumbCache;
pub use recent::RecentProjects;

use directories::ProjectDirs;
use egs_api::EpicGames;
use egs_api::api::types::download_manifest::DownloadManifest;
use session::SessionStore;

/// Page de connexion Epic. Après connexion, elle affiche un JSON
/// dont il faut copier la valeur `authorizationCode`.
pub const LOGIN_URL: &str = "https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode";

/// Bilan d'un téléchargement terminé.
pub struct DownloadSummary {
    pub files: usize,
    pub bytes: u64,
    /// Dossier où les fichiers ont été écrits
    pub dir: PathBuf,
}

/// Point d'entrée unique vers les services Epic.
pub struct EpicClient {
    egs: EpicGames,
    store: SessionStore,
    http: reqwest::Client,
}

impl EpicClient {
    /// Crée le client et recharge la session enregistrée si elle existe.
    pub fn new() -> Result<Self> {
        let store = SessionStore::new()?;
        let mut egs = EpicGames::new();
        if let Some(saved) = store.load()? {
            egs.set_user_details(saved);
        }
        Ok(Self {
            egs,
            store,
            http: reqwest::Client::new(),
        })
    }

    /// Première connexion avec le code copié depuis `LOGIN_URL`.
    pub async fn login_with_code(&mut self, code: &str) -> Result<()> {
        let code = code.trim().trim_matches('"').to_string();
        if !self.egs.auth_code(None, Some(code)).await {
            return Err(CoreError::AuthFailed);
        }
        self.store.save(&self.egs.user_details())
    }

    /// Reprend la session enregistrée (rafraîchit le token si besoin).
    pub async fn resume(&mut self) -> Result<()> {
        if self.egs.is_logged_in() {
            return Ok(());
        }
        if !self.egs.login().await {
            return Err(CoreError::NotLoggedIn);
        }
        self.store.save(&self.egs.user_details())
    }

    /// Où la session est rangée (trousseau ou fichier).
    pub fn session_storage(&self) -> &'static str {
        self.store.backend()
    }

    /// Nom affiché du compte connecté.
    pub fn display_name(&self) -> Option<String> {
        self.egs.user_details().display_name
    }

    /// Toute la bibliothèque Fab du compte (pagination gérée par egs-api).
    pub async fn fab_library(&mut self) -> Result<Vec<FabAsset>> {
        self.resume().await?;
        let account_id = self
            .egs
            .user_details()
            .account_id
            .ok_or(CoreError::NotLoggedIn)?;
        let library = self
            .egs
            .try_fab_library_items(account_id)
            .await
            .map_err(|e| CoreError::Api(format!("{e:?}")))?;
        Ok(library.results)
    }

    /// Récupère le manifest de téléchargement d'une version d'un asset.
    pub async fn fab_manifest(
        &mut self,
        asset: &FabAsset,
        version: &ProjectVersion,
    ) -> Result<DownloadManifest> {
        self.resume().await?;
        let platform = version
            .build_versions
            .first()
            .map(|b| b.platform.as_str())
            .unwrap_or("Windows");

        let infos = self
            .egs
            .fab_asset_manifest(
                &version.artifact_id,
                &asset.asset_namespace,
                &asset.asset_id,
                Some(platform),
            )
            .await
            .map_err(|e| CoreError::Api(format!("manifest d'asset : {e:?}")))?;
        let info = infos
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Api("aucune info de téléchargement".into()))?;

        // Plusieurs CDN possibles : on prend le premier qui répond.
        for base_url in &info.distribution_point_base_urls {
            match self.egs.fab_download_manifest(info.clone(), base_url).await {
                Ok(manifest) => return Ok(manifest),
                Err(e) => log::warn!("CDN {base_url} : {e:?}"),
            }
        }
        Err(CoreError::Api("aucun CDN n'a fourni le manifest".into()))
    }

    /// Télécharge une version d'un asset dans `out_dir`.
    pub async fn download(
        &mut self,
        asset: &FabAsset,
        version: &ProjectVersion,
        out_dir: &Path,
        jobs: usize,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<DownloadSummary> {
        let manifest = self.fab_manifest(asset, version).await?;
        let plan = DownloadPlan::from_manifest(&manifest);
        self.install(&plan, version, out_dir, jobs, on_progress)
            .await
    }

    /// Ajoute une version d'un asset dans un projet Unreal, au bon endroit selon son type.
    /// Sans `overwrite`, refuse d'écraser des fichiers déjà présents.
    pub async fn add_to_project(
        &mut self,
        asset: &FabAsset,
        version: &ProjectVersion,
        project: &UnrealProject,
        overwrite: bool,
        jobs: usize,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<DownloadSummary> {
        let kind = AssetKind::of(asset);
        if kind == AssetKind::CompleteProject {
            return Err(CoreError::Project(format!(
                "« {} » est un projet complet : télécharge-le dans un nouveau dossier",
                asset.title
            )));
        }

        let manifest = self.fab_manifest(asset, version).await?;
        let plan = DownloadPlan::from_manifest(&manifest);
        let target = match kind {
            AssetKind::Plugin => project::plugin_target(&project.dir, &plan)?,
            _ => project.dir.clone(),
        };

        if !overwrite {
            let existing = plan.existing_files(&target);
            if let Some(first) = existing.first() {
                return Err(CoreError::Conflict(existing.len(), first.to_string()));
            }
        }
        self.install(&plan, version, &target, jobs, on_progress)
            .await
    }

    /// Télécharge les chunks puis écrit les fichiers dans `out_dir`.
    async fn install(
        &self,
        plan: &DownloadPlan,
        version: &ProjectVersion,
        out_dir: &Path,
        jobs: usize,
        on_progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<DownloadSummary> {
        let cache = cache_dir()?.join(&version.artifact_id);
        plan.download_chunks(&self.http, &cache, jobs, on_progress)
            .await?;
        plan.assemble_files(&cache, out_dir, on_progress).await?;
        tokio::fs::remove_dir_all(&cache).await?;

        Ok(DownloadSummary {
            files: plan.file_count(),
            bytes: plan.install_size(),
            dir: out_dir.to_path_buf(),
        })
    }

    /// Déconnexion côté Epic + suppression de la session locale.
    pub async fn logout(&mut self) -> Result<()> {
        let _ = self.egs.logout().await;
        self.store.clear()
    }
}

/// ~/.cache/uehub/chunks : les chunks en cours, gardés tant que l'asset n'est pas complet.
fn cache_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "uehub").ok_or(CoreError::NoDataDir)?;
    Ok(dirs.cache_dir().join("chunks"))
}