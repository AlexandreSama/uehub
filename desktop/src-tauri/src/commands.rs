//! Les commandes que l'interface React appelle avec `invoke`.
//! Elles traduisent les types du core en JSON simple pour TypeScript.

use std::path::PathBuf;
use std::sync::Mutex as StdMutex;

use serde::{Serialize, Serializer};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;
use uehub_core::{
    AssetKind, CoreError, DownloadSummary, EpicClient, FabAsset, LOGIN_URL, Progress,
    UnrealProject, folder_name, parse_version, pick_for_project, pick_version,
};

/// Nombre de chunks téléchargés en parallèle.
const JOBS: usize = 8;

/// L'état partagé par toutes les commandes.
pub struct AppState {
    /// Verrou asynchrone : un téléchargement le garde jusqu'à la fin.
    client: Mutex<EpicClient>,
    /// La dernière bibliothèque chargée, pour retrouver un asset par son id.
    library: StdMutex<Vec<FabAsset>>,
}

impl AppState {
    pub fn new(client: EpicClient) -> Self {
        Self {
            client: Mutex::new(client),
            library: StdMutex::new(Vec::new()),
        }
    }

    fn asset(&self, id: &str) -> Result<FabAsset> {
        self.library
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.asset_id == id)
            .cloned()
            .ok_or_else(|| Error::Msg("asset introuvable : recharge la bibliothèque".into()))
    }
}

// ---------------------------------------------------------------------------
// Erreurs : elles arrivent en JavaScript sous forme de texte
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("{0}")]
    Msg(String),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Ce qu'on envoie à l'interface
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Account {
    name: String,
    storage: String,
}

#[derive(Serialize)]
pub struct Asset {
    id: String,
    title: String,
    /// "pack", "plugin" ou "project"
    kind: &'static str,
    category: Option<String>,
    thumbnail: Option<String>,
    /// Versions d'Unreal compatibles, triées : ["4.27", "5.0", …]
    engines: Vec<String>,
}

#[derive(Serialize)]
pub struct Summary {
    files: usize,
    bytes: u64,
    dir: String,
    /// Remarque à afficher, par exemple quand la version exacte n'existait pas
    note: Option<String>,
}

/// Une étape de progression, envoyée sur un `Channel` : {"phase":"chunks","done":3,"total":40}
#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum ProgressEvent {
    Chunks { done: usize, total: usize },
    Files { done: usize, total: usize },
}

impl From<Progress> for ProgressEvent {
    fn from(p: Progress) -> Self {
        match p {
            Progress::Chunks { done, total } => Self::Chunks { done, total },
            Progress::Files { done, total } => Self::Files { done, total },
        }
    }
}

fn account(client: &EpicClient) -> Account {
    Account {
        name: client.display_name().unwrap_or_default(),
        storage: client.session_storage().into(),
    }
}

impl From<&FabAsset> for Asset {
    fn from(a: &FabAsset) -> Self {
        let mut engines: Vec<(u32, u32)> = a
            .project_versions
            .iter()
            .flat_map(|v| &v.engine_versions)
            .filter_map(|v| parse_version(v.trim_start_matches("UE_")))
            .collect();
        engines.sort();
        engines.dedup();

        Self {
            id: a.asset_id.clone(),
            title: a.title.clone(),
            kind: match AssetKind::of(a) {
                AssetKind::AssetPack => "pack",
                AssetKind::Plugin => "plugin",
                AssetKind::CompleteProject => "project",
            },
            category: a.categories.first().and_then(|c| c.name.clone()),
            thumbnail: a
                .images
                .iter()
                .find(|i| i.type_field == "Thumbnail")
                .or_else(|| a.images.first())
                .map(|i| i.url.clone()),
            engines: engines
                .iter()
                .map(|(major, minor)| format!("{major}.{minor}"))
                .collect(),
        }
    }
}

fn summary(done: DownloadSummary, note: Option<String>) -> Summary {
    Summary {
        files: done.files,
        bytes: done.bytes,
        dir: done.dir.display().to_string(),
        note,
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// Reprend la session enregistrée. `None` : il faut se connecter.
#[tauri::command]
pub async fn session(state: State<'_, AppState>) -> Result<Option<Account>> {
    let mut client = state.client.lock().await;
    match client.resume().await {
        Ok(()) => Ok(Some(account(&client))),
        Err(CoreError::NotLoggedIn) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Ouvre la page de connexion Epic dans le navigateur par défaut.
#[tauri::command]
pub fn open_login_page(app: AppHandle) -> Result<()> {
    app.opener()
        .open_url(LOGIN_URL, None::<&str>)
        .map_err(|e| Error::Msg(format!("impossible d'ouvrir le navigateur : {e}")))
}

#[tauri::command]
pub async fn login(state: State<'_, AppState>, code: String) -> Result<Account> {
    let mut client = state.client.lock().await;
    client.login_with_code(&code).await?;
    Ok(account(&client))
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<()> {
    state.library.lock().unwrap().clear();
    state.client.lock().await.logout().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Bibliothèque et téléchargements
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn library(state: State<'_, AppState>) -> Result<Vec<Asset>> {
    let assets = state.client.lock().await.fab_library().await?;
    let views = assets.iter().map(Asset::from).collect();
    *state.library.lock().unwrap() = assets;
    Ok(views)
}

/// Télécharge la version la plus récente dans `folder/<titre de l'asset>`.
#[tauri::command]
pub async fn download(
    state: State<'_, AppState>,
    asset_id: String,
    folder: PathBuf,
    on_progress: Channel<ProgressEvent>,
) -> Result<Summary> {
    let asset = state.asset(&asset_id)?;
    let version = pick_version(&asset, None)
        .cloned()
        .ok_or_else(|| Error::Msg("aucune version téléchargeable".into()))?;
    let out = folder.join(folder_name(&asset.title));
    let send = |p: Progress| {
        let _ = on_progress.send(p.into());
    };

    let mut client = state.client.lock().await;
    let done = client.download(&asset, &version, &out, JOBS, &send).await?;
    Ok(summary(done, None))
}

/// Ajoute l'asset au projet, avec la version qui correspond à son moteur.
#[tauri::command]
pub async fn add_to_project(
    state: State<'_, AppState>,
    asset_id: String,
    uproject: PathBuf,
    overwrite: bool,
    on_progress: Channel<ProgressEvent>,
) -> Result<Summary> {
    let asset = state.asset(&asset_id)?;
    let project = UnrealProject::open(&uproject)?;
    let (major, minor) = project.engine.ok_or_else(|| {
        Error::Msg(format!(
            "version d'Unreal du projet {} introuvable",
            project.name
        ))
    })?;
    let (version, exact) = pick_for_project(&asset, (major, minor)).ok_or_else(|| {
        Error::Msg(format!(
            "« {} » n'existe pas pour UE {major}.{minor} ni pour une version plus ancienne",
            asset.title
        ))
    })?;
    if !exact && AssetKind::of(&asset) == AssetKind::Plugin {
        return Err(Error::Msg(format!(
            "ce plugin n'existe pas pour UE {major}.{minor}"
        )));
    }
    let note = (!exact).then(|| {
        let used = version.engine_versions.last().map_or("?", String::as_str);
        format!(
            "Pas de version pour UE {major}.{minor} : celle pour {} a été installée, Unreal la convertira.",
            used.replace('_', " ")
        )
    });
    let version = version.clone();
    let send = |p: Progress| {
        let _ = on_progress.send(p.into());
    };

    let mut client = state.client.lock().await;
    let done = client
        .add_to_project(&asset, &version, &project, overwrite, JOBS, &send)
        .await?;
    Ok(summary(done, note))
}