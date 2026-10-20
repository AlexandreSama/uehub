//! La file d'attente des téléchargements : un seul à la fois, dans l'ordre d'arrivée.
//! Elle est enregistrée sur le disque ; au lancement, ce qui n'était pas fini revient en pause.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::future::{AbortHandle, Abortable};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;
use uehub_core::{EngineInstall, t};

use crate::commands::{AppState, ProgressEvent, Summary, run_job};
use crate::notify;

// ---------------------------------------------------------------------------
// Ce qu'on range dans la file (et dans queue.json)
// ---------------------------------------------------------------------------

/// Ce que l'utilisateur demande. En JSON : {"kind":"download","assetId":"…","folder":"…"}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    /// Télécharger la version la plus récente dans `folder/<titre de l'asset>`
    Download { asset_id: String, folder: PathBuf },
    /// Ajouter au projet, avec la version qui correspond à son moteur
    AddToProject {
        asset_id: String,
        uproject: PathBuf,
        overwrite: bool,
    },
    /// Installer un moteur Unreal précompilé dans `folder/<version>`
    InstallEngine { version: String, folder: PathBuf },
}

/// Ce qu'un job retiré avant la fin laisse sur le disque.
pub enum Leftover {
    /// Le cache de chunks d'une version d'asset
    Chunks(String),
    /// Le zip d'un moteur et ce qui en a déjà été extrait
    Engine { version: String, dir: PathBuf },
}

impl Leftover {
    fn of(job: Job) -> Option<Self> {
        match job.request {
            Request::InstallEngine { version, folder } => Some(Self::Engine {
                dir: folder.join(&version),
                version,
            }),
            _ => job.artifact_id.map(Self::Chunks),
        }
    }
}

/// Où en est un job. En JSON : {"state":"waiting"}, {"state":"done","summary":{…}}…
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    Waiting,
    Running,
    Paused,
    Done { summary: Summary },
    Failed { error: String },
}

/// Un téléchargement de la file.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub request: Request,
    /// Titre et vignette de l'asset, pour l'affichage avant même le chargement de la bibliothèque
    pub title: String,
    pub thumbnail: Option<String>,
    /// Nom du projet ou dossier de destination
    pub target: String,
    pub status: Status,
    /// La version téléchargée, connue une fois le job lancé : c'est aussi le nom
    /// de son dossier de chunks dans le cache.
    pub artifact_id: Option<String>,
    /// Fin du job, en millisecondes depuis 1970 : la barre du bas montre le bilan du plus récent.
    #[serde(default)]
    pub finished_at: Option<u64>,
    /// Avancement du job en cours. Envoyé à l'interface, jamais relu depuis le disque.
    #[serde(skip_deserializing)]
    pub progress: Option<ProgressEvent>,
}

impl Job {
    fn is_done(&self) -> bool {
        matches!(self.status, Status::Done { .. })
    }
}

// ---------------------------------------------------------------------------
// Les règles de la file, sans Tauri ni disque
// ---------------------------------------------------------------------------

pub struct Jobs {
    list: Vec<Job>,
    next_id: u64,
}

impl Jobs {
    /// Reprend une file enregistrée. Rien ne tourne au lancement :
    /// ce qui attendait ou tournait passe en pause.
    pub fn restore(mut list: Vec<Job>) -> Self {
        for job in &mut list {
            if matches!(job.status, Status::Waiting | Status::Running) {
                job.status = Status::Paused;
            }
            job.progress = None;
        }
        let next_id = list.iter().map(|j| j.id + 1).max().unwrap_or(1);
        Self { list, next_id }
    }

    /// Ajoute un job en fin de file, sauf si la même demande y est déjà.
    pub fn add(&mut self, mut job: Job) -> Result<(), String> {
        if self
            .list
            .iter()
            .any(|j| j.request == job.request && !j.is_done())
        {
            return Err(t!("errors.already_queued", title = job.title));
        }
        job.id = self.next_id;
        job.status = Status::Waiting;
        self.next_id += 1;
        self.list.push(job);
        Ok(())
    }

    /// Le premier job en attente passe « en cours ».
    pub fn start_next(&mut self) -> Option<Job> {
        let job = self
            .list
            .iter_mut()
            .find(|j| matches!(j.status, Status::Waiting))?;
        job.status = Status::Running;
        Some(job.clone())
    }

    pub fn finish(&mut self, id: u64, status: Status) {
        if let Some(job) = self.get_mut(id) {
            job.status = status;
            job.progress = None;
            job.finished_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as u64);
        }
    }

    pub fn pause(&mut self, id: u64) {
        if let Some(job) = self.get_mut(id) {
            pause_job(job);
        }
    }

    pub fn pause_all(&mut self) {
        self.list.iter_mut().for_each(pause_job);
    }

    /// Remet en attente un job en pause ou en erreur.
    pub fn resume(&mut self, id: u64) {
        if let Some(job) = self.get_mut(id)
            && matches!(job.status, Status::Paused | Status::Failed { .. })
        {
            job.status = Status::Waiting;
        }
    }

    pub fn resume_all(&mut self) {
        for job in &mut self.list {
            if matches!(job.status, Status::Paused) {
                job.status = Status::Waiting;
            }
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<Job> {
        let index = self.list.iter().position(|j| j.id == id)?;
        Some(self.list.remove(index))
    }

    pub fn clear_finished(&mut self) {
        self.list.retain(|j| !j.is_done());
    }

    /// Un job pas terminé a-t-il encore besoin de ces fichiers ? Le même asset peut être
    /// en file pour deux projets, et un moteur retiré peut y être remis aussitôt.
    pub fn still_needs(&self, leftover: &Leftover) -> bool {
        self.list
            .iter()
            .filter(|j| !j.is_done())
            .any(|j| match (leftover, &j.request) {
                (Leftover::Chunks(id), _) => j.artifact_id.as_deref() == Some(id.as_str()),
                (Leftover::Engine { dir, .. }, Request::InstallEngine { version, folder }) => {
                    folder.join(version) == *dir
                }
                (Leftover::Engine { .. }, _) => false,
            })
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Job> {
        self.list.iter_mut().find(|j| j.id == id)
    }
}

fn pause_job(job: &mut Job) {
    if matches!(job.status, Status::Waiting | Status::Running) {
        job.status = Status::Paused;
        job.progress = None;
    }
}

// ---------------------------------------------------------------------------
// La file partagée par les commandes et le worker
// ---------------------------------------------------------------------------

pub struct Queue {
    app: AppHandle,
    /// ~/.local/share/<identifiant de l'app>/queue.json
    file: PathBuf,
    jobs: Mutex<Jobs>,
    /// Réveille le worker quand il y a du nouveau
    wake: Notify,
    /// Le job en cours et de quoi l'arrêter
    current: Mutex<Option<(u64, AbortHandle)>>,
    /// Ce que les jobs retirés ont laissé, supprimé par le worker entre deux jobs
    trash: Mutex<Vec<Leftover>>,
}

impl Queue {
    /// Recharge la file enregistrée (vide si le fichier manque ou est abîmé).
    pub fn load(app: AppHandle) -> tauri::Result<Self> {
        let file = app.path().app_data_dir()?.join("queue.json");
        let saved: Vec<Job> = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Ok(Self {
            app,
            file,
            jobs: Mutex::new(Jobs::restore(saved)),
            wake: Notify::new(),
            current: Mutex::new(None),
            trash: Mutex::new(Vec::new()),
        })
    }

    pub fn snapshot(&self) -> Vec<Job> {
        self.jobs.lock().unwrap().list.clone()
    }

    /// Applique un changement, enregistre la file et l'envoie à l'interface.
    fn update<T>(&self, change: impl FnOnce(&mut Jobs) -> T) -> T {
        let mut jobs = self.jobs.lock().unwrap();
        let result = change(&mut jobs);
        if let Err(e) = self.save(&jobs.list) {
            log::warn!("queue not saved: {e}");
        }
        let _ = self.app.emit("queue", &jobs.list);
        result
    }

    fn save(&self, list: &[Job]) -> std::io::Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_vec_pretty(list)?)
    }

    pub fn add(&self, job: Job) -> Result<(), String> {
        self.update(|jobs| jobs.add(job))?;
        self.wake.notify_one();
        Ok(())
    }

    pub fn pause(&self, id: u64) {
        self.update(|jobs| jobs.pause(id));
        self.abort_if_running(Some(id));
    }

    /// Met tout en pause, par exemple avant une déconnexion.
    pub fn pause_all(&self) {
        self.update(|jobs| jobs.pause_all());
        self.abort_if_running(None);
    }

    pub fn resume(&self, id: u64) {
        self.update(|jobs| jobs.resume(id));
        self.wake.notify_one();
    }

    pub fn resume_all(&self) {
        self.update(|jobs| jobs.resume_all());
        self.wake.notify_one();
    }

    /// Retire un job. S'il n'était pas fini, ce qu'il a laissé part à la corbeille.
    pub fn remove(&self, id: u64) {
        let Some(job) = self.update(|jobs| jobs.remove(id)) else {
            return;
        };
        if !job.is_done()
            && let Some(leftover) = Leftover::of(job)
        {
            self.trash.lock().unwrap().push(leftover);
        }
        self.abort_if_running(Some(id));
        self.wake.notify_one();
    }

    pub fn clear_finished(&self) {
        self.update(|jobs| jobs.clear_finished());
    }

    /// Appelé à chaque chunk : on prévient l'interface sans réécrire le fichier.
    pub fn set_progress(&self, id: u64, progress: ProgressEvent) {
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(id) {
            job.progress = Some(progress);
        }
        let _ = self.app.emit("queue", &jobs.list);
    }

    pub fn set_artifact(&self, id: u64, artifact_id: &str) {
        self.update(|jobs| {
            if let Some(job) = jobs.get_mut(id) {
                job.artifact_id = Some(artifact_id.to_string());
            }
        });
    }

    /// Passe au job suivant. `current` est rempli sous le même verrou que la liste :
    /// une pause demandée au même instant trouve forcément de quoi arrêter le job.
    fn start_next(&self, handle: AbortHandle) -> Option<Job> {
        self.update(|jobs| {
            let job = jobs.start_next()?;
            *self.current.lock().unwrap() = Some((job.id, handle));
            Some(job)
        })
    }

    /// Arrête le job en cours si c'est `id`, ou quel qu'il soit si `id` vaut `None`.
    fn abort_if_running(&self, id: Option<u64>) {
        if let Some((current, handle)) = &*self.current.lock().unwrap()
            && id.is_none_or(|id| id == *current)
        {
            handle.abort();
        }
    }

    /// Vide la corbeille, sauf ce qu'un autre job utilise encore.
    async fn empty_trash(&self) {
        let trash = std::mem::take(&mut *self.trash.lock().unwrap());
        for leftover in trash {
            let needed = self.jobs.lock().unwrap().still_needs(&leftover);
            if needed {
                continue;
            }
            let result = match &leftover {
                Leftover::Chunks(artifact_id) => uehub_core::discard_download(artifact_id).await,
                Leftover::Engine { version, dir } => EngineInstall::discard(version, dir).await,
            };
            if let Err(e) = result {
                log::warn!("files of a removed job not deleted: {}", e.detail());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Le worker
// ---------------------------------------------------------------------------

/// Prend les jobs en attente un par un, tant que l'app tourne.
pub async fn worker(app: AppHandle) {
    let queue = app.state::<Queue>();
    let state = app.state::<AppState>();
    loop {
        // Entre deux téléchargements : aucun chunk n'est en cours d'écriture.
        queue.empty_trash().await;

        let (handle, registration) = AbortHandle::new_pair();
        let Some(job) = queue.start_next(handle) else {
            queue.wake.notified().await;
            continue;
        };

        log::info!("job {} started: {}", job.id, describe(&job));
        let result = Abortable::new(run_job(&state, &queue, &job), registration).await;
        *queue.current.lock().unwrap() = None;

        // Interrompu (pause ou retrait) : la file est déjà à jour, et pas de notification.
        let Ok(result) = result else {
            log::info!("job {} stopped (pause or removal)", job.id);
            continue;
        };
        let status = match result {
            Ok(summary) => {
                log::info!("job {} done: {}", job.id, summary.text());
                Status::Done { summary }
            }
            // L'interface montre la phrase ; le journal garde aussi la cause.
            Err(e) => {
                log::warn!("job {} failed: {}", job.id, e.detail());
                Status::Failed {
                    error: e.to_string(),
                }
            }
        };
        queue.update(|jobs| jobs.finish(job.id, status.clone()));
        notify::job_finished(&app, &Job { status, ..job });
    }
}

/// Ce que fait un job, pour le journal : "download \"Megascans: Grass\" into ~/Assets".
fn describe(job: &Job) -> String {
    let what = match &job.request {
        Request::Download { .. } => "download",
        Request::AddToProject { .. } => "add",
        Request::InstallEngine { .. } => "install",
    };
    format!("{what} \"{}\" into {}", job.title, job.target)
}

#[cfg(test)]
mod tests;
