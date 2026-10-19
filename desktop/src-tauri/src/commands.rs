//! Les commandes que l'interface React appelle avec `invoke`.
//! Elles traduisent les types du core en JSON simple pour TypeScript.

use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use serde::{Deserialize, Serialize, Serializer};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;
use uehub_core::{
    AssetKind, AssetUpdate, CoreError, DownloadSummary, Downloaded, EULA_URL, EngineDownload,
    EngineInstall, EngineList, EpicClient, FabAsset, InstallProgress, InstalledEngine, LOGIN_URL,
    PreparedDownload, Progress, ProjectAssets, ProjectVersion, RecentProjects, UnrealProject,
    UnrealSession, engine_for, engines_dir, extracted_size, folder_name, installed_engines,
    launch_editor, parse_version, pick_for_project, pick_version, update_for,
};

use crate::queue::{Job, Queue, Request, Status};
use crate::settings::Settings;

/// Nombre de chunks téléchargés en parallèle.
const JOBS: usize = 8;

/// L'état partagé par toutes les commandes.
pub struct AppState {
    /// Verrou asynchrone : un job ne le garde que le temps de préparer son téléchargement.
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
    #[error(transparent)]
    Updater(#[from] tauri_plugin_updater::Error),
    #[error("{0}")]
    Msg(String),
}


impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

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
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    /// Chemin du fichier .uproject
    path: String,
    name: String,
    /// "5.4", ou absent si la version du projet n'a pas pu être lue
    engine: Option<String>,
    /// La version du moteur installé qui l'ouvrira ("5.4.4"), s'il y en a un
    opens_with: Option<String>,
}

/// Un asset que uehub a ajouté à un projet récent, et sa mise à jour s'il y en a une.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedView {
    /// Le .uproject du projet
    project: String,
    asset_id: String,
    /// La version d'Unreal visée par la version installée, ex. "5.3"
    engine: String,
    /// Date de l'ajout, en secondes depuis 1970
    added_at: u64,
    files: usize,
    update: Option<UpdateView>,
}

/// Pourquoi mettre à jour : {"kind":"newBuild"} ou {"kind":"closerEngine","engine":"5.4"}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateView {
    NewBuild,
    CloserEngine { engine: String },
}

impl From<AssetUpdate> for UpdateView {
    fn from(update: AssetUpdate) -> Self {
        match update {
            AssetUpdate::NewBuild => Self::NewBuild,
            AssetUpdate::CloserEngine(engine) => Self::CloserEngine { engine },
        }
    }
}

/// L'avancement du job en cours, envoyé avec la file :
/// {"phase":"chunks","done":3,"total":40,"bytes":3145728}
#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum ProgressEvent {
    Chunks {
        done: usize,
        total: usize,
        /// Octets reçus du réseau, pour la vitesse
        bytes: u64,
    },
    Files {
        done: usize,
        total: usize,
    },
    /// Octets du zip d'un moteur
    Bytes {
        done: u64,
        total: u64,
    },
}

impl From<Progress> for ProgressEvent {
    fn from(p: Progress) -> Self {
        match p {
            Progress::Chunks { done, total, bytes } => Self::Chunks { done, total, bytes },
            Progress::Files { done, total } => Self::Files { done, total },
        }
    }
}

impl From<InstallProgress> for ProgressEvent {
    fn from(p: InstallProgress) -> Self {
        match p {
            InstallProgress::Download { done, total } => Self::Bytes { done, total },
            InstallProgress::Extract { done, total } => Self::Files { done, total },
        }
    }
}

/// Un moteur proposé par Epic. L'adresse de téléchargement reste dans Rust.
#[derive(Serialize)]
pub struct AvailableEngine {
    /// "5.8.3"
    version: String,
    /// Taille du zip en octets
    size: u64,
    /// Place estimée une fois extrait, en octets
    extracted: u64,
    /// Date de mise en ligne : "2026-09-22"
    date: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCatalog {
    /// Sans CLUF accepté, Epic ne propose rien : `engines` est vide.
    eula_accepted: bool,
    /// Quand la liste a été lue sur unrealengine.com, en secondes depuis 1970
    checked_at: u64,
    engines: Vec<AvailableEngine>,
}

/// Un moteur déjà enregistré dans Install.ini.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngine {
    version: Option<String>,
    dir: String,
    /// Installé par uehub : l'écran propose de le désinstaller
    by_uehub: bool,
}

/// La liste telle que l'écran l'affiche : le jour de mise en ligne, et la place
/// qu'il faudra une fois le zip extrait.
fn catalog(list: EngineList, eula_accepted: bool) -> EngineCatalog {
    let engines = list
        .engines
        .into_iter()
        .map(|e| AvailableEngine {
            date: e
                .created_at
                .as_deref()
                .and_then(|d| d.get(..10))
                .map(String::from),
            version: e.version,
            extracted: extracted_size(e.size),
            size: e.size,
        })
        .collect();
    EngineCatalog {
        eula_accepted,
        checked_at: list.checked_at,
        engines,
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

impl Summary {
    /// "61 fichiers, 1,2 Go", comme summaryText côté React.
    pub fn text(&self) -> String {
        format!("{} fichiers, {}", self.files, format_bytes(self.bytes))
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

/// 1 234 567 890 → "1,2 Go", comme formatBytes côté React.
fn format_bytes(bytes: u64) -> String {
    let (value, unit) = match bytes {
        b if b >= 1_000_000_000 => (b as f64 / 1e9, "Go"),
        b if b >= 1_000_000 => (b as f64 / 1e6, "Mo"),
        b => (b as f64 / 1e3, "Ko"),
    };
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{} {unit}", text.replace('.', ","))
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
    if let Request::InstallEngine { version, folder } = &job.request {
        return install_engine(state, queue, job.id, version, folder).await;
    }
    let (prepared, note) = prepare(state, queue, job).await?;
    // Le client est libre : pendant les chunks, la bibliothèque peut se recharger.
    let send = |p: Progress| queue.set_progress(job.id, p.into());
    let done = prepared.run(JOBS, &send).await?;
    Ok(summary(done, note))
}

/// La partie qui parle à Epic. Le verrou du client est relâché en sortant d'ici,
/// et `PreparedDownload` ne peut pas le garder : il ne contient aucune référence.
async fn prepare(
    state: &AppState,
    queue: &Queue,
    job: &Job,
) -> Result<(PreparedDownload, Option<String>)> {
    let mut client = state.client.lock().await;
    match &job.request {
        Request::Download { asset_id, folder } => {
            let asset = state.asset_or_reload(&mut client, asset_id).await?;
            let version = latest_version(&asset)?;
            queue.set_artifact(job.id, &version.artifact_id);
            let out = folder.join(folder_name(&asset.title));
            let prepared = client.prepare_download(&asset, &version, &out).await?;
            Ok((prepared, None))
        }
        Request::AddToProject {
            asset_id,
            uproject,
            overwrite,
        } => {
            let asset = state.asset_or_reload(&mut client, asset_id).await?;
            let plan = plan_add(&asset, uproject)?;
            queue.set_artifact(job.id, &plan.version.artifact_id);
            let prepared = client
                .prepare_add(&asset, &plan.version, &plan.project, *overwrite)
                .await?;
            Ok((prepared, plan.note))
        }
        Request::InstallEngine { .. } => unreachable!("run_job installe les moteurs à part"),
    }
}

// ---------------------------------------------------------------------------
// Installer un moteur
// ---------------------------------------------------------------------------

/// Une session sur unrealengine.com. Le client n'est verrouillé que le temps
/// d'obtenir un code d'échange : la bibliothèque reste utilisable pendant l'installation.
async fn unreal_session(state: &AppState) -> Result<UnrealSession> {
    let code = state.client.lock().await.exchange_code().await?;
    Ok(UnrealSession::connect(&code).await?)
}

fn eula_missing() -> Error {
    Error::Msg("accepte d'abord le CLUF d'Unreal Engine, depuis l'écran Moteurs".into())
}

/// Télécharge le zip, l'extrait et enregistre le moteur dans Install.ini.
async fn install_engine(
    state: &AppState,
    queue: &Queue,
    id: u64,
    version: &str,
    folder: &Path,
) -> Result<Summary> {
    let session = unreal_session(state).await?;
    if !session.eula_accepted().await? {
        return Err(eula_missing());
    }
    let engine = session.engine_download(version).await?;
    let install = EngineInstall::new(&engine, Some(folder.join(version)))?;
    let send = |p: InstallProgress| queue.set_progress(id, p.into());

    // Déjà extrait (si l'enregistrement avait échoué) : il ne reste qu'à l'enregistrer.
    let files = if install.is_installed() {
        0
    } else {
        install.check_space()?;
        download_zip(state, &install, engine.clone(), &send).await?;
        install.extract(&send).await?
    };
    install.register()?;
    Ok(Summary {
        files,
        bytes: engine.size,
        dir: install.dir.display().to_string(),
        note: None,
    })
}

/// Télécharge le zip jusqu'au bout : une coupure reprend là où le fichier s'est arrêté,
/// une adresse expirée (elle ne vaut qu'une heure) est remplacée par une fraîche.
async fn download_zip(
    state: &AppState,
    install: &EngineInstall,
    mut engine: EngineDownload,
    send: &(dyn Fn(InstallProgress) + Send + Sync),
) -> Result<()> {
    let mut stuck = 0;
    loop {
        let before = install.downloaded();
        match install.download(&engine, send).await? {
            Downloaded::Complete => return Ok(()),
            Downloaded::Expired => {
                engine = unreal_session(state)
                    .await?
                    .engine_download(&install.version)
                    .await?;
            }
            Downloaded::Interrupted => tokio::time::sleep(Duration::from_secs(2)).await,
        }
        // Trois essais de suite sans un octet de plus : on abandonne, « Réessayer » reprendra.
        stuck = if install.downloaded() > before {
            0
        } else {
            stuck + 1
        };
        if stuck >= 3 {
            return Err(Error::Msg("le téléchargement n'avance plus".into()));
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
    open_in_browser(&app, LOGIN_URL)
}

fn open_in_browser(app: &AppHandle, url: &str) -> Result<()> {
    app.opener()
        .open_url(url, None::<&str>)
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
    // Sans compte, les jobs en attente échoueraient l'un après l'autre : on met tout en pause.
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
    let (title, thumbnail, target) = describe(&state, &request)?;
    // L'id et l'état définitifs sont fixés par la file.
    let job = Job {
        id: 0,
        request,
        title,
        thumbnail,
        target,
        status: Status::Waiting,
        artifact_id: None,
        finished_at: None,
        progress: None,
    };
    queue.add(job).map_err(Error::Msg)
}

/// Vérifie une demande et renvoie ce que la file affiche : titre, vignette et cible.
fn describe(state: &AppState, request: &Request) -> Result<(String, Option<String>, String)> {
    match request {
        Request::Download { asset_id, folder } => {
            let asset = state.asset(asset_id)?;
            latest_version(&asset)?;
            let target = folder.join(folder_name(&asset.title));
            Ok((
                asset.title.clone(),
                thumbnail(&asset),
                target.display().to_string(),
            ))
        }
        Request::AddToProject {
            asset_id, uproject, ..
        } => {
            let asset = state.asset(asset_id)?;
            let plan = plan_add(&asset, uproject)?;
            // Le projet passe en tête des récents dès qu'il entre dans la file.
            if let Ok(mut recent) = RecentProjects::load() {
                let _ = recent.remember(&plan.project.uproject);
            }
            Ok((asset.title.clone(), thumbnail(&asset), plan.project.name))
        }
        Request::InstallEngine { version, folder } => {
            let dir = folder.join(version);
            if EngineInstall::is_installed_in(version, &dir) {
                return Err(Error::Msg(format!(
                    "UE {version} est déjà installé dans {}",
                    dir.display()
                )));
            }
            EngineInstall::check_dir(version, &dir)?;
            let title = format!("Unreal Engine {version}");
            Ok((title, None, dir.display().to_string()))
        }
    }
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
// Moteurs
// ---------------------------------------------------------------------------

/// Les moteurs enregistrés dans Install.ini, du plus récent au plus ancien.
#[tauri::command]
pub fn local_engines() -> Result<Vec<LocalEngine>> {
    let engines = installed_engines()?
        .into_iter()
        .map(|e| LocalEngine {
            version: e.version,
            dir: e.dir.display().to_string(),
            by_uehub: e.by_uehub,
        })
        .collect();
    Ok(engines)
}

/// La dernière liste d'Epic gardée sur le disque : elle s'affiche sans attendre le
/// réseau. Pas encore de liste (premier passage) : `null`.
#[tauri::command]
pub fn cached_engines() -> Option<EngineCatalog> {
    EngineList::load().map(|list| catalog(list, true))
}

/// Ce qu'Epic propose aujourd'hui pour Linux, du plus récent au plus ancien. Il faut
/// ouvrir une session unrealengine.com : quelques secondes. La liste lue remplace
/// celle du disque.
#[tauri::command]
pub async fn available_engines(state: State<'_, AppState>) -> Result<EngineCatalog> {
    let session = unreal_session(&state).await?;
    if !session.eula_accepted().await? {
        // Epic ne propose rien à ce compte : la liste gardée ne doit plus s'afficher.
        EngineList::forget();
        return Ok(catalog(EngineList::checked_now(Vec::new()), false));
    }
    Ok(catalog(session.engine_list().await?, true))
}

/// Le dossier où ranger les moteurs : celui que tu as choisi, sinon ~/UnrealEngine.
#[tauri::command]
pub fn engines_folder(app: AppHandle) -> Result<String> {
    let folder = match Settings::load(&app).engines_folder {
        Some(folder) => folder,
        None => engines_dir()?,
    };
    Ok(folder.display().to_string())
}

/// Retient le dossier des moteurs pour les prochains lancements.
#[tauri::command]
pub fn set_engines_folder(app: AppHandle, folder: PathBuf) -> Result<()> {
    let mut settings = Settings::load(&app);
    settings.engines_folder = Some(folder);
    settings
        .save(&app)
        .map_err(|e| Error::Msg(format!("réglages non enregistrés : {e}")))
}

/// La place libre, en octets, sur le disque du dossier des moteurs.
#[tauri::command]
pub fn free_space(folder: PathBuf) -> Result<u64> {
    Ok(uehub_core::free_space(&folder)?)
}

/// Le moteur enregistré dans ce dossier. L'interface ne désigne un moteur que par
/// son dossier : on vérifie qu'il est bien dans Install.ini avant d'y toucher.
fn registered_engine(dir: &Path) -> Result<InstalledEngine> {
    installed_engines()?
        .into_iter()
        .find(|e| e.dir == dir)
        .ok_or_else(|| Error::Msg(format!("aucun moteur enregistré dans {}", dir.display())))
}

/// Lance l'éditeur d'un moteur installé, sans projet.
#[tauri::command]
pub fn launch_engine(dir: PathBuf) -> Result<()> {
    Ok(launch_editor(&registered_engine(&dir)?, None)?)
}

/// Ouvre un projet avec le moteur installé de sa version.
#[tauri::command]
pub fn open_project(uproject: PathBuf) -> Result<()> {
    let project = UnrealProject::open(&uproject)?;
    let installed = installed_engines()?;
    let engine = engine_for(&installed, &project).ok_or_else(|| {
        let version = project.engine_version().unwrap_or_else(|| "?".into());
        Error::Msg(format!(
            "UE {version} n'est pas installé : installe-le depuis l'écran Moteurs"
        ))
    })?;
    Ok(launch_editor(engine, Some(&project.uproject))?)
}

/// Désinstalle un moteur installé par uehub : son dossier, puis sa ligne d'Install.ini.
#[tauri::command]
pub async fn remove_engine(dir: PathBuf) -> Result<()> {
    let engine = registered_engine(&dir)?;
    Ok(uehub_core::uninstall_engine(&engine.dir).await?)
}

/// Ouvre la page du CLUF d'Unreal Engine dans le navigateur par défaut.
#[tauri::command]
pub fn open_eula_page(app: AppHandle) -> Result<()> {
    open_in_browser(&app, EULA_URL)
}

// ---------------------------------------------------------------------------
// Projets récents
// ---------------------------------------------------------------------------

/// Les projets récents encore lisibles, le plus récent en premier.
#[tauri::command]
pub fn recent_projects() -> Result<Vec<RecentProject>> {
    let recent = RecentProjects::load()?;
    let installed = installed_engines().unwrap_or_default();
    let projects = recent
        .paths()
        .iter()
        .filter_map(|path| UnrealProject::open(path).ok())
        .map(|p| RecentProject {
            opens_with: engine_for(&installed, &p)
                .map(|e| e.version.clone().unwrap_or_else(|| "?".into())),
            path: p.uproject.display().to_string(),
            engine: p.engine_version(),
            name: p.name,
        })
        .collect();
    Ok(projects)
}

/// Ce que uehub a ajouté aux projets récents, comparé à la bibliothèque chargée. Sans
/// bibliothèque (pas encore chargée, ou hors ligne), les fiches arrivent sans mise à jour.
#[tauri::command]
pub fn added_assets(state: State<'_, AppState>) -> Result<Vec<AddedView>> {
    let library = state.library.lock().unwrap();
    let mut views = Vec::new();
    for path in RecentProjects::load()?.paths() {
        let Ok(project) = UnrealProject::open(path) else {
            continue;
        };
        for added in ProjectAssets::load(&project.dir).list() {
            let asset = library.iter().find(|a| a.asset_id == added.asset_id);
            let update = match (asset, project.engine) {
                (Some(asset), Some(engine)) => update_for(added, asset, engine),
                _ => None,
            };
            views.push(AddedView {
                project: project.uproject.display().to_string(),
                asset_id: added.asset_id.clone(),
                engine: added.engine.clone(),
                added_at: added.added_at,
                files: added.files.len(),
                update: update.map(UpdateView::from),
            });
        }
    }
    Ok(views)
}

#[tauri::command]
pub fn forget_project(uproject: PathBuf) -> Result<()> {
    RecentProjects::load()?.forget(&uproject)?;
    Ok(())
}