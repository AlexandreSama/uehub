//! Les commandes que l'interface React appelle avec `invoke`.
//! Elles traduisent les types du core en JSON simple pour TypeScript.

use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

use serde::{Deserialize, Serialize, Serializer};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;
use uehub_core::{
    AssetKind, CoreError, DownloadSummary, EpicClient, FabAsset, LOGIN_URL, Progress,
    ProjectVersion, RecentProjects, UnrealProject, folder_name, parse_version, pick_for_project,
    pick_version,
};

use crate::queue::{Job, Queue, Request, Status};

/// Nombre de chunks téléchargés en parallèle.
const JOBS: usize = 8;

/// L'état partagé par toutes les commandes.
pub struct AppState {
    /// Verrou asynchrone : le téléchargement en cours le garde jusqu'à la fin.
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

    /// Comme `asset`, mais recharge la bibliothèque si elle ne contient pas l'asset
    /// (par exemple si tu reprends la file avant qu'elle ait fini de charger).
    async fn asset_or_reload(&self, client: &mut EpicClient, id: &str) -> Result<FabAsset> {
        if let Ok(asset) = self.asset(id) {
            return Ok(asset);
        }
        let assets = client.fab_library().await?;
        *self.library.lock().unwrap() = assets;
        self.asset(id)
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

/// Le bilan d'un job terminé. Il est aussi enregistré dans la file.
#[derive(Clone, Serialize, Deserialize)]
pub struct Summary {
    files: usize,
    bytes: u64,
    dir: String,
    /// Remarque à afficher, par exemple quand la version exacte n'existait pas
    note: Option<String>,
}

#[derive(Serialize)]
pub struct RecentProject {
    /// Chemin du fichier .uproject
    path: String,
    name: String,
    /// "5.4", ou absent si la version du projet n'a pas pu être lue
    engine: Option<String>,
}

/// L'avancement du job en cours, envoyé avec la file : {"phase":"chunks","done":3,"total":40}
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

/// L'image « Thumbnail » de l'asset, sinon la première.
fn thumbnail(asset: &FabAsset) -> Option<String> {
    asset
        .images
        .iter()
        .find(|i| i.type_field == "Thumbnail")
        .or_else(|| asset.images.first())
        .map(|i| i.url.clone())
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
            thumbnail: thumbnail(a),
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
// Choisir la version à installer
// ---------------------------------------------------------------------------

/// La version la plus récente, pour un téléchargement dans un dossier.
fn latest_version(asset: &FabAsset) -> Result<ProjectVersion> {
    pick_version(asset, None)
        .cloned()
        .ok_or_else(|| Error::Msg("aucune version téléchargeable".into()))
}

/// Tout ce qu'il faut pour ajouter un asset à un projet.
struct AddPlan {
    project: UnrealProject,
    version: ProjectVersion,
    /// Remarque à afficher quand la version exacte n'existait pas
    note: Option<String>,
}

/// Lit le projet et choisit la version de l'asset qui correspond à son moteur.
fn plan_add(asset: &FabAsset, uproject: &Path) -> Result<AddPlan> {
    let project = UnrealProject::open(uproject)?;
    let (major, minor) = project.engine.ok_or_else(|| {
        Error::Msg(format!(
            "version d'Unreal du projet {} introuvable",
            project.name
        ))
    })?;
    let (version, exact) = pick_for_project(asset, (major, minor)).ok_or_else(|| {
        Error::Msg(format!(
            "« {} » n'existe pas pour UE {major}.{minor} ni pour une version plus ancienne",
            asset.title
        ))
    })?;
    if !exact && AssetKind::of(asset) == AssetKind::Plugin {
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
    Ok(AddPlan {
        version: version.clone(),
        project,
        note,
    })
}

/// Exécute un job de la file. Seul le worker l'appelle, un job à la fois.
pub async fn run_job(state: &AppState, queue: &Queue, job: &Job) -> Result<Summary> {
    let mut client = state.client.lock().await;
    let asset = state
        .asset_or_reload(&mut client, job.request.asset_id())
        .await?;
    let send = |p: Progress| queue.set_progress(job.id, p.into());

    match &job.request {
        Request::Download { folder, .. } => {
            let version = latest_version(&asset)?;
            queue.set_artifact(job.id, &version.artifact_id);
            let out = folder.join(folder_name(&asset.title));
            let done = client.download(&asset, &version, &out, JOBS, &send).await?;
            Ok(summary(done, None))
        }
        Request::AddToProject {
            uproject,
            overwrite,
            ..
        } => {
            let plan = plan_add(&asset, uproject)?;
            queue.set_artifact(job.id, &plan.version.artifact_id);
            let done = client
                .add_to_project(
                    &asset,
                    &plan.version,
                    &plan.project,
                    *overwrite,
                    JOBS,
                    &send,
                )
                .await?;
            Ok(summary(done, plan.note))
        }
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
pub async fn logout(state: State<'_, AppState>, queue: State<'_, Queue>) -> Result<()> {
    // Le téléchargement en cours garde le client : on met la file en pause pour le libérer.
    queue.pause_all();
    state.library.lock().unwrap().clear();
    state.client.lock().await.logout().await?;
    Ok(())
}

#[tauri::command]
pub async fn library(state: State<'_, AppState>) -> Result<Vec<Asset>> {
    let assets = state.client.lock().await.fab_library().await?;
    let views = assets.iter().map(Asset::from).collect();
    *state.library.lock().unwrap() = assets;
    Ok(views)
}

// ---------------------------------------------------------------------------
// File d'attente
// ---------------------------------------------------------------------------

/// Ajoute une demande à la file. Ce qui ne peut pas marcher (version introuvable…)
/// est refusé tout de suite, pas au moment du téléchargement.
#[tauri::command]
pub fn enqueue(
    state: State<'_, AppState>,
    queue: State<'_, Queue>,
    request: Request,
) -> Result<()> {
    let asset = state.asset(request.asset_id())?;
    let target = match &request {
        Request::Download { folder, .. } => {
            latest_version(&asset)?;
            folder.join(folder_name(&asset.title)).display().to_string()
        }
        Request::AddToProject { uproject, .. } => {
            let plan = plan_add(&asset, uproject)?;
            // Le projet passe en tête des récents dès qu'il entre dans la file.
            if let Ok(mut recent) = RecentProjects::load() {
                let _ = recent.remember(&plan.project.uproject);
            }
            plan.project.name
        }
    };
    // L'id et l'état définitifs sont fixés par la file.
    let job = Job {
        id: 0,
        request,
        title: asset.title.clone(),
        thumbnail: thumbnail(&asset),
        target,
        status: Status::Waiting,
        artifact_id: None,
        finished_at: None,
        progress: None,
    };
    queue.add(job).map_err(Error::Msg)
}

/// La file telle qu'elle est maintenant. Ensuite, l'interface suit l'événement "queue".
#[tauri::command]
pub fn jobs(queue: State<'_, Queue>) -> Vec<Job> {
    queue.snapshot()
}

#[tauri::command]
pub fn pause_job(queue: State<'_, Queue>, id: u64) {
    queue.pause(id);
}

#[tauri::command]
pub fn resume_job(queue: State<'_, Queue>, id: u64) {
    queue.resume(id);
}

#[tauri::command]
pub fn remove_job(queue: State<'_, Queue>, id: u64) {
    queue.remove(id);
}

#[tauri::command]
pub fn resume_all(queue: State<'_, Queue>) {
    queue.resume_all();
}

#[tauri::command]
pub fn clear_finished(queue: State<'_, Queue>) {
    queue.clear_finished();
}

// ---------------------------------------------------------------------------
// Projets récents
// ---------------------------------------------------------------------------

/// Les projets récents encore lisibles, le plus récent en premier.
#[tauri::command]
pub fn recent_projects() -> Result<Vec<RecentProject>> {
    let recent = RecentProjects::load()?;
    let projects = recent
        .paths()
        .iter()
        .filter_map(|path| UnrealProject::open(path).ok())
        .map(|p| RecentProject {
            path: p.uproject.display().to_string(),
            name: p.name,
            engine: p.engine.map(|(major, minor)| format!("{major}.{minor}")),
        })
        .collect();
    Ok(projects)
}

#[tauri::command]
pub fn forget_project(uproject: PathBuf) -> Result<()> {
    RecentProjects::load()?.forget(&uproject)?;
    Ok(())
}