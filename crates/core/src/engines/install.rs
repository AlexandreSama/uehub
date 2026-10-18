//! Installer un moteur précompilé : télécharger le zip (avec reprise), l'extraire,
//! puis l'enregistrer dans ~/.config/Epic/UnrealEngine/Install.ini.

use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use directories::BaseDirs;
use futures::StreamExt;
use reqwest::StatusCode;
use reqwest::header::RANGE;
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

use super::{EngineDownload, net, version_key};
use crate::{CoreError, Result};

/// L'éditeur, relatif au dossier du moteur : sa présence prouve que l'extraction a marché.
pub(super) const EDITOR: &str = "Engine/Binaries/Linux/UnrealEditor";

/// Une fois extrait, le moteur pèse environ 1,8 fois son zip (39,8 Go → ~70 Go pour la 5.8).
const EXTRACT_RATIO: f64 = 1.8;

/// Sans un octet reçu pendant ce temps, la connexion est considérée comme coupée.
const STALLED: Duration = Duration::from_secs(60);

/// La marque que uehub laisse dans un moteur qu'il a installé : seuls ceux-là se
/// désinstallent depuis uehub. Un moteur compilé par toi n'est jamais supprimé.
const MARKER: &str = ".uehub";

/// Où en est une installation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InstallProgress {
    /// Octets du zip téléchargés
    Download { done: u64, total: u64 },
    /// Fichiers extraits
    Extract { done: usize, total: usize },
}

/// Comment s'est fini un essai de téléchargement.
#[derive(Debug, PartialEq)]
pub enum Downloaded {
    /// Le zip est complet
    Complete,
    /// L'adresse a expiré (elle n'est valable qu'une heure) : il en faut une fraîche
    Expired,
    /// La connexion a été coupée en route : on peut reprendre
    Interrupted,
}

/// Une installation : où va le moteur, et où se télécharge son zip.
pub struct EngineInstall {
    pub version: String,
    /// Dossier du moteur, ex. ~/UnrealEngine/5.8.3
    pub dir: PathBuf,
    /// Le zip, à côté du dossier : ~/UnrealEngine/Linux_Unreal_Engine_5.8.3.zip
    zip: PathBuf,
    /// Taille attendue du zip
    size: u64,
    http: reqwest::Client,
}

impl EngineInstall {
    /// Prépare l'installation de `download` dans `dir` (par défaut ~/UnrealEngine/<version>).
    pub fn new(download: &EngineDownload, dir: Option<PathBuf>) -> Result<Self> {
        let dir = match dir {
            Some(dir) => std::path::absolute(dir)?,
            None => engines_dir()?.join(&download.version),
        };
        Self::check_dir(&download.version, &dir)?;
        // Une connexion muette pendant une minute est coupée : le téléchargement reprendra.
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(STALLED)
            .build()?;
        Ok(Self {
            version: download.version.clone(),
            zip: Self::zip_path(&download.version, &dir)?,
            dir,
            size: download.size,
            http,
        })
    }

    /// Refuse un dossier qui contient déjà autre chose : l'extraction y mêlerait ses
    /// fichiers, et abandonner l'installation le supprimerait. Un dossier absent ou vide,
    /// ou une installation de ce moteur (en cours ou finie), est accepté.
    pub fn check_dir(version: &str, dir: &Path) -> Result<()> {
        let ours = Self::zip_path(version, dir)?.exists() || dir.join(EDITOR).is_file();
        let empty = fs::read_dir(dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(true);
        if ours || empty {
            return Ok(());
        }
        Err(CoreError::Download(format!(
            "{} existe déjà et ne contient pas ce moteur : choisis un autre dossier",
            dir.display()
        )))
    }

    /// Le zip se télécharge à côté du dossier du moteur, sur le même disque :
    /// ~/UnrealEngine/5.8.3 → ~/UnrealEngine/Linux_Unreal_Engine_5.8.3.zip
    pub fn zip_path(version: &str, dir: &Path) -> Result<PathBuf> {
        let parent = dir
            .parent()
            .ok_or_else(|| CoreError::Download("dossier d'installation invalide".into()))?;
        Ok(parent.join(format!("Linux_Unreal_Engine_{version}.zip")))
    }

    /// Un moteur est-il déjà installé là ? (l'éditeur existe et aucun zip n'attend)
    pub fn is_installed_in(version: &str, dir: &Path) -> bool {
        let zip_waiting = Self::zip_path(version, dir).is_ok_and(|zip| zip.exists());
        dir.join(EDITOR).is_file() && !zip_waiting
    }

    /// Octets du zip déjà sur le disque : c'est de là que repart le téléchargement.
    pub fn downloaded(&self) -> u64 {
        fs::metadata(&self.zip).map(|m| m.len()).unwrap_or(0)
    }

    /// Le moteur est-il déjà là ?
    pub fn is_installed(&self) -> bool {
        Self::is_installed_in(&self.version, &self.dir)
    }

    /// Vérifie qu'il reste la place pour la fin du zip et pour l'extraction (estimée).
    pub fn check_space(&self) -> Result<()> {
        let rest = self.size.saturating_sub(self.downloaded());
        let extracted = (self.size as f64 * EXTRACT_RATIO) as u64;
        check_space(&self.dir, rest + extracted)
    }

    /// Télécharge le zip à l'adresse de `download` (fraîche, même version), en reprenant
    /// là où le fichier s'est arrêté.
    pub async fn download(
        &self,
        download: &EngineDownload,
        on_progress: &(dyn Fn(InstallProgress) + Send + Sync),
    ) -> Result<Downloaded> {
        if let Some(parent) = self.zip.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut done = self.downloaded();
        if done > self.size {
            // Plus gros qu'annoncé : ce n'est pas le bon fichier, on recommence.
            tokio::fs::remove_file(&self.zip).await?;
            done = 0;
        }
        if done == self.size {
            return Ok(Downloaded::Complete);
        }
        let total = self.size;
        on_progress(InstallProgress::Download { done, total });

        // On demande la suite du fichier, à partir de l'octet `done`.
        let mut response = match self
            .http
            .get(&download.url)
            .header(RANGE, format!("bytes={done}-"))
            .send()
            .await
        {
            Ok(response) => response,
            // Réseau coupé ou serveur muet avant même de répondre : on réessaiera.
            Err(e) if e.is_connect() || e.is_timeout() => return Ok(Downloaded::Interrupted),
            Err(e) => return Err(net(e)),
        };
        let mut file = match response.status() {
            // Le serveur reprend là où on en était.
            StatusCode::PARTIAL_CONTENT => {
                tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.zip)
                    .await?
            }
            // Il renvoie tout le fichier : on repart de zéro.
            StatusCode::OK => {
                done = 0;
                tokio::fs::File::create(&self.zip).await?
            }
            // Une adresse signée périmée donne 403.
            StatusCode::FORBIDDEN => return Ok(Downloaded::Expired),
            status => {
                return Err(CoreError::Download(format!(
                    "le serveur a répondu {status}"
                )));
            }
        };

        let mut reported = done;
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk).await?;
                    done += chunk.len() as u64;
                    // Un message tous les 16 Mo : 40 Go en font déjà 2 500.
                    if done - reported >= 16 << 20 {
                        on_progress(InstallProgress::Download { done, total });
                        reported = done;
                    }
                }
                Ok(None) => break,
                // Coupure réseau : ce qui est écrit reste, on reprendra.
                Err(_) => break,
            }
        }
        file.flush().await?;
        on_progress(InstallProgress::Download { done, total });
        Ok(if done >= total {
            Downloaded::Complete
        } else {
            Downloaded::Interrupted
        })
    }

    /// Extrait le zip dans le dossier du moteur, vérifie l'éditeur, puis supprime le zip.
    /// Les fichiers déjà extraits avec la bonne taille sont gardés : une reprise ne refait
    /// pas tout. Renvoie le nombre d'entrées du zip.
    pub async fn extract(
        &self,
        on_progress: &(dyn Fn(InstallProgress) + Send + Sync),
    ) -> Result<usize> {
        let stop = Arc::new(AtomicBool::new(false));
        // L'extraction tourne sur un thread à part. Si cette tâche est abandonnée (pause),
        // ce garde est détruit et l'extraction s'arrête au fichier suivant.
        let _stop_on_drop = StopOnDrop(stop.clone());
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let (zip, dir) = (self.zip.clone(), self.dir.clone());
        let task = tokio::task::spawn_blocking(move || {
            extract_zip(&zip, &dir, &stop, &|progress| {
                let _ = sender.unbounded_send(progress);
            })
        });
        // Le canal se ferme quand le thread se termine.
        while let Some(progress) = receiver.next().await {
            on_progress(progress);
        }
        let files = task
            .await
            .map_err(|e| CoreError::Download(format!("extraction : {e}")))??;

        if !self.dir.join(EDITOR).is_file() {
            return Err(CoreError::Download(format!(
                "le zip ne contient pas {EDITOR}"
            )));
        }

        if !self.dir.join(EDITOR).is_file() {
            return Err(CoreError::Download(format!(
                "le zip ne contient pas {EDITOR}"
            )));
        }
        tokio::fs::write(self.dir.join(MARKER), &self.version).await?;
        tokio::fs::remove_file(&self.zip).await?;
        Ok(files)
    }

    /// Enregistre le moteur dans Install.ini et renvoie son identifiant.
    pub fn register(&self) -> Result<String> {
        register_engine(&install_ini()?, &self.dir)
    }

    /// Abandon d'une installation pas finie : supprime le zip et ce qui a déjà été extrait.
    /// Sans zip, l'installation est finie (ou n'a jamais commencé) : on ne touche à rien.
    pub async fn discard(version: &str, dir: &Path) -> Result<()> {
        let zip = Self::zip_path(version, dir)?;
        if !zip.exists() {
            return Ok(());
        }
        // Le dossier d'abord : si ça échoue, le zip reste et on pourra recommencer.
        if dir.exists() {
            tokio::fs::remove_dir_all(dir).await?;
        }
        tokio::fs::remove_file(&zip).await?;
        Ok(())
    }
}

/// Met le drapeau d'arrêt quand il est détruit.
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// ~/UnrealEngine : chaque moteur y a son dossier, ex. ~/UnrealEngine/5.8.3.
pub fn engines_dir() -> Result<PathBuf> {
    let dirs = BaseDirs::new().ok_or(CoreError::NoDataDir)?;
    Ok(dirs.home_dir().join("UnrealEngine"))
}

/// ~/.config/Epic/UnrealEngine/Install.ini, là où les outils Unreal cherchent les moteurs.
fn install_ini() -> Result<PathBuf> {
    let dirs = BaseDirs::new().ok_or(CoreError::NoDataDir)?;
    Ok(dirs.config_dir().join("Epic/UnrealEngine/Install.ini"))
}

fn check_space(dir: &Path, needed: u64) -> Result<()> {
    // Le dossier n'existe peut-être pas encore : on mesure le premier parent existant.
    let existing = dir
        .ancestors()
        .find(|path| path.exists())
        .unwrap_or(Path::new("/"));
    let available = fs4::available_space(existing)?;
    if available < needed {
        return Err(CoreError::Download(format!(
            "pas assez de place : il faut environ {} Go libres dans {}, il en reste {} Go",
            gigabytes(needed),
            existing.display(),
            gigabytes(available)
        )));
    }
    Ok(())
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.0}", bytes as f64 / 1e9)
}

fn zip_error(e: zip::result::ZipError) -> CoreError {
    CoreError::Download(format!("zip : {e}"))
}

/// Extrait tout le zip dans `dir` et renvoie son nombre d'entrées.
/// Bloquant : à lancer sur un thread à part.
fn extract_zip(
    zip: &Path,
    dir: &Path,
    stop: &AtomicBool,
    on_progress: &dyn Fn(InstallProgress),
) -> Result<usize> {
    let mut archive = ZipArchive::new(BufReader::new(File::open(zip)?)).map_err(zip_error)?;
    let total = archive.len();
    let root = common_root(&mut archive)?;

    // La place qu'il faut vraiment : la taille des fichiers pas encore extraits.
    let mut needed = 0;
    for i in 0..total {
        let entry = archive.by_index_raw(i).map_err(zip_error)?;
        if let Some(target) = target_of(entry.enclosed_name(), root.as_deref(), dir)
            && entry.is_file()
            && !fs::metadata(&target).is_ok_and(|m| m.len() == entry.size())
        {
            needed += entry.size();
        }
    }
    check_space(dir, needed)?;

    for i in 0..total {
        if stop.load(Ordering::Relaxed) {
            return Err(CoreError::Download("extraction interrompue".into()));
        }
        extract_entry(&mut archive, i, root.as_deref(), dir)?;
        if (i + 1) % 200 == 0 || i + 1 == total {
            on_progress(InstallProgress::Extract { done: i + 1, total });
        }
    }
    Ok(total)
}

/// Écrit l'entrée `i` du zip : dossier, lien symbolique ou fichier.
fn extract_entry(
    archive: &mut ZipArchive<BufReader<File>>,
    i: usize,
    root: Option<&Path>,
    dir: &Path,
) -> Result<()> {
    let mut entry = archive.by_index(i).map_err(zip_error)?;
    // Un chemin qui sortirait du dossier (`../`, absolu) est ignoré.
    let Some(target) = target_of(entry.enclosed_name(), root, dir) else {
        return Ok(());
    };
    let relative = target.strip_prefix(dir).unwrap_or(&target).to_path_buf();

    if entry.is_dir() {
        fs::create_dir_all(&target)?;
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if entry.is_symlink() {
        // Le contenu de l'entrée est la cible du lien.
        let mut link = String::new();
        entry.read_to_string(&mut link)?;
        if stays_inside(&relative, Path::new(&link)) {
            let _ = fs::remove_file(&target);
            symlink(&link, &target)?;
        }
        return Ok(());
    }

    let present = fs::metadata(&target).is_ok_and(|m| m.len() == entry.size());
    if !present {
        let mut out = File::create(&target)?;
        // Le crate vérifie le CRC en fin de lecture. Un fichier abîmé est effacé,
        // sinon la reprise le croirait complet.
        if let Err(e) = io::copy(&mut entry, &mut out) {
            drop(out);
            let _ = fs::remove_file(&target);
            return Err(CoreError::Download(format!("{} : {e}", relative.display())));
        }
    }
    // Même pour un fichier déjà là : une coupure a pu arriver avant ce réglage.
    let mode = file_mode(entry.unix_mode(), &relative);
    fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
    Ok(())
}

/// Si tout le zip est rangé dans un seul dossier (autre qu'Engine), on le retire :
/// le moteur doit se retrouver directement dans `dir`. Les chemins dangereux, qui seront
/// ignorés, ne comptent pas.
fn common_root(archive: &mut ZipArchive<BufReader<File>>) -> Result<Option<PathBuf>> {
    let mut root: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(zip_error)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(Component::Normal(first)) = name.components().next() else {
            continue;
        };
        match &root {
            None => root = Some(PathBuf::from(first)),
            Some(root) if root.as_os_str() == first => {}
            Some(_) => return Ok(None),
        }
    }
    Ok(root.filter(|root| root.as_os_str() != "Engine"))
}

/// Où écrire une entrée du zip, sans le dossier commun éventuel.
fn target_of(name: Option<PathBuf>, root: Option<&Path>, dir: &Path) -> Option<PathBuf> {
    let name = name?;
    let relative = match root {
        Some(root) => name.strip_prefix(root).ok()?.to_path_buf(),
        None => name,
    };
    (!relative.as_os_str().is_empty()).then(|| dir.join(relative))
}

/// Les droits du fichier. Un zip fait sous Windows n'en a pas : on rend alors exécutables
/// les scripts et les binaires Linux, sinon l'éditeur ne se lancerait pas. Le propriétaire
/// garde toujours le droit d'écrire, pour qu'une reprise puisse réécrire le fichier.
fn file_mode(unix_mode: Option<u32>, relative: &Path) -> u32 {
    let mode = unix_mode.map(|m| m & 0o777).unwrap_or_else(|| {
        let script = relative.extension().is_some_and(|e| e == "sh");
        let binary =
            relative.starts_with("Engine/Binaries/Linux") && relative.extension().is_none();
        if script || binary { 0o755 } else { 0o644 }
    });
    mode | 0o200
}

/// Un lien symbolique relatif qui reste dans le dossier du moteur.
fn stays_inside(relative: &Path, link: &Path) -> bool {
    let mut depth = relative.parent().map_or(0, |p| p.components().count()) as i64;
    for component in link.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// Les lignes `identifiant=dossier` de la section [Installations].
fn installations(text: &str) -> Vec<(String, PathBuf)> {
    let mut in_section = false;
    let mut found = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[Installations]");
        } else if in_section && let Some((id, dir)) = line.split_once('=') {
            found.push((id.trim().to_string(), PathBuf::from(dir.trim())));
        }
    }
    found
}

/// Ajoute `dir` à la section [Installations] (créée si besoin), sans toucher au reste.
/// Un moteur déjà enregistré garde son identifiant.
fn register_engine(ini: &Path, dir: &Path) -> Result<String> {
    let text = fs::read_to_string(ini).unwrap_or_default();
    if let Some((id, _)) = installations(&text).into_iter().find(|(_, d)| d == dir) {
        return Ok(id);
    }
    // Un GUID sans accolades, comme ceux qu'Unreal écrit sous Linux.
    let id = uuid::Uuid::new_v4().hyphenated().to_string().to_uppercase();
    let line = format!("{id}={}", dir.display());
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    match lines
        .iter()
        .position(|l| l.trim().eq_ignore_ascii_case("[Installations]"))
    {
        Some(at) => lines.insert(at + 1, line),
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[Installations]".into());
            lines.push(line);
        }
    }
    if let Some(parent) = ini.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(ini, lines.join("\n") + "\n")?;
    Ok(id)
}

/// Retire `dir` de la section [Installations], sans toucher au reste.
fn unregister_engine(ini: &Path, dir: &Path) -> Result<()> {
    let Ok(text) = fs::read_to_string(ini) else {
        return Ok(());
    };
    let mut in_section = false;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            let line = line.trim();
            if line.starts_with('[') {
                in_section = line.eq_ignore_ascii_case("[Installations]");
                return true;
            }
            let ours = line
                .split_once('=')
                .is_some_and(|(_, d)| Path::new(d.trim()) == dir);
            !(in_section && ours)
        })
        .collect();
    fs::write(ini, kept.join("\n") + "\n")?;
    Ok(())
}

/// Désinstalle un moteur installé par uehub : son dossier et sa ligne d'Install.ini.
pub async fn uninstall_engine(dir: &Path) -> Result<()> {
    uninstall_from(&install_ini()?, dir).await
}

async fn uninstall_from(ini: &Path, dir: &Path) -> Result<()> {
    if !dir.join(MARKER).is_file() {
        return Err(CoreError::Engine(format!(
            "{} n'a pas été installé par uehub : supprime-le toi-même si tu en es sûr",
            dir.display()
        )));
    }
    // 1. On le renomme : c'est instantané, et une suppression interrompue laissera un
    //    dossier au nom explicite plutôt qu'un moteur à moitié vide encore enregistré.
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let doomed = dir.with_file_name(format!(".{name}.suppression"));
    if doomed.exists() {
        tokio::fs::remove_dir_all(&doomed).await?;
    }
    tokio::fs::rename(dir, &doomed).await?;
    // 2. Plus de ligne dans Install.ini : Unreal et uehub l'oublient.
    unregister_engine(ini, dir)?;
    // 3. Le gros du travail : des dizaines de milliers de fichiers.
    tokio::fs::remove_dir_all(&doomed).await?;
    Ok(())
}

/// Un moteur enregistré dans Install.ini.
pub struct InstalledEngine {
    /// Son identifiant (un GUID), celui que les .uproject peuvent citer
    pub id: String,
    pub dir: PathBuf,
    /// "5.8.3", lu dans Engine/Build/Build.version
    pub version: Option<String>,
    /// Installé par uehub (il porte sa marque) : uehub peut le désinstaller
    pub by_uehub: bool,
}

/// Les moteurs enregistrés dont le dossier existe encore, du plus récent au plus ancien.
pub fn installed_engines() -> Result<Vec<InstalledEngine>> {
    Ok(installed_in(&install_ini()?))
}

fn installed_in(ini: &Path) -> Vec<InstalledEngine> {
    let text = fs::read_to_string(ini).unwrap_or_default();
    let mut engines: Vec<InstalledEngine> = installations(&text)
        .into_iter()
        .filter(|(_, dir)| dir.join("Engine").is_dir())
        .map(|(id, dir)| InstalledEngine {
            version: build_version(&dir),
            by_uehub: dir.join(MARKER).is_file(),
            id,
            dir,
        })
        .collect();
    engines.sort_by_key(|e| std::cmp::Reverse(e.version.as_deref().map(version_key)));
    engines
}

/// "5.8.3", d'après Engine/Build/Build.version.
fn build_version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("Engine/Build/Build.version")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let part = |key: &str| json[key].as_u64();
    Some(format!(
        "{}.{}.{}",
        part("MajorVersion")?,
        part("MinorVersion")?,
        part("PatchVersion").unwrap_or(0)
    ))
}