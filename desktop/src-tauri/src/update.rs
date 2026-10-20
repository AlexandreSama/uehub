//! Les mises à jour de uehub lui-même.
//!
//! Installé depuis un .deb, un .rpm ou une AppImage (ce que fabrique `tauri build`),
//! uehub se met à jour seul : il lit latest.json sur GitHub Releases, télécharge le
//! paquet, vérifie sa signature et l'installe. Construit autrement (le paquet AUR,
//! `cargo build`, `tauri dev`), il laisse faire le gestionnaire de paquets.

use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::utils::config::BundleType;
use tauri::utils::platform::bundle_type;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};
use uehub_core::{CONNECT_TIMEOUT, READ_TIMEOUT, t};

use crate::commands::{Error, Result};

/// Cette copie de uehub, et la façon dont elle se met à jour.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    /// "deb", "rpm" ou "appimage" : uehub se met à jour seul. `null` : une copie construite
    /// sans paquet (AUR, `cargo build`), que met à jour le gestionnaire de paquets.
    package: Option<&'static str>,
    /// Lancée par `tauri dev`
    dev: bool,
}

/// Une version plus récente, publiée sur GitHub Releases.
#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    /// "2026-10-19"
    date: Option<String>,
    /// Ce qui change, tel qu'écrit dans latest.json
    notes: Option<String>,
}

/// L'avancement de l'installation : {"step":"download","done":…,"total":…}, puis {"step":"install"}.
#[derive(Clone, Serialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum UpdateProgress {
    Download { done: u64, total: Option<u64> },
    Install,
}

/// La mise à jour trouvée par `check_update`, que `install_update` installera : on
/// installe exactement ce qu'on a annoncé.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

/// Le paquet d'où vient ce binaire. Le bundler de Tauri l'inscrit dans le binaire
/// en fabriquant le .deb, le .rpm ou l'AppImage ; un binaire sans paquet n'a rien.
pub fn package() -> Option<&'static str> {
    match bundle_type()? {
        BundleType::Deb => Some("deb"),
        BundleType::Rpm => Some("rpm"),
        BundleType::AppImage => Some("appimage"),
        _ => None,
    }
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        package: package(),
        dev: cfg!(debug_assertions),
    }
}

/// Cherche une version plus récente. `null` : uehub est à jour.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>> {
    if package().is_none() {
        return Err(Error::Msg(t!("errors.package_manager")));
    }
    let update = updater(&app)?.check().await?;
    match &update {
        Some(update) => log::info!("update available: {}", update.version),
        None => log::info!("no update: uehub is up to date"),
    }
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        date: u.date.map(|d| d.date().to_string()),
        notes: u.body.clone(),
    });
    *pending.0.lock().unwrap() = update;
    Ok(info)
}

/// Télécharge la mise à jour annoncée, vérifie sa signature, l'installe et relance uehub.
/// Pour un .deb ou un .rpm, le système demande ton mot de passe.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
    on_progress: Channel<UpdateProgress>,
) -> Result<()> {
    let update = pending
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| Error::Msg(t!("errors.no_update")))?;
    log::info!("installing uehub {}", update.version);
    let mut done = 0;
    update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                let _ = on_progress.send(UpdateProgress::Download { done, total });
            },
            || {
                let _ = on_progress.send(UpdateProgress::Install);
            },
        )
        .await?;
    // La file est enregistrée : ce qui tournait revient en pause au redémarrage.
    log::info!("update installed, restarting");
    app.restart()
}

/// Le plugin, avec les mêmes délais que les clients du core : sans réponse, la
/// recherche finit par « délai dépassé » au lieu d'attendre indéfiniment.
fn updater(app: &AppHandle) -> tauri_plugin_updater::Result<Updater> {
    app.updater_builder()
        .configure_client(|client| {
            client
                .connect_timeout(CONNECT_TIMEOUT)
                .read_timeout(READ_TIMEOUT)
        })
        .build()
}

/// Une phrase par erreur du plugin. Le réseau est déjà parti vers le core (voir
/// `From<tauri_plugin_updater::Error>`) : il reste la signature, l'installation du paquet
/// et ce que le serveur a renvoyé.
pub fn sentence(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => t!("errors.update_signature"),
        E::AuthenticationFailed => t!("errors.update_cancelled"),
        E::DebInstallFailed | E::PackageInstallFailed => t!("errors.update_install"),
        E::ReleaseNotFound | E::TargetNotFound(_) | E::TargetsNotFound(_) | E::Serialization(_) => {
            t!("errors.update_no_release")
        }
        E::Network(_) => t!("errors.update_download"),
        _ => t!("errors.update_failed"),
    }
}

#[cfg(test)]
mod tests;
