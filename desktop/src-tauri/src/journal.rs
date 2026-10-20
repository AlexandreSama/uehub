//! Le journal, côté app : la première ligne de chaque lancement, les erreurs de la
//! WebView, et de quoi retrouver le fichier depuis l'interface.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use uehub_core::{i18n, journal, t};

use crate::commands::{Error, Result};

/// La ligne qui ouvre chaque lancement : de quoi situer un rapport de bug.
pub fn started(app: &AppHandle) {
    let package = crate::update::package().unwrap_or("no package");
    log::info!(
        "uehub {} started ({package}, language {}, {})",
        app.package_info().version,
        i18n::lang().code(),
        system()
    );
}

/// Le système, d'après /etc/os-release : "Arch Linux", "Ubuntu 24.04.1 LTS"…
fn system() -> String {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|name| name.trim_matches('"').to_string())
        .unwrap_or_else(|| std::env::consts::OS.into())
}

/// La langue de l'interface, choisie au démarrage d'après celle du système.
#[tauri::command]
pub fn language() -> &'static str {
    i18n::lang().code()
}

/// Le chemin du journal, pour l'afficher. `null` : pas de dossier pour lui.
#[tauri::command]
pub fn log_file() -> Option<String> {
    journal::path().map(|path| path.display().to_string())
}

/// Montre le journal dans le gestionnaire de fichiers. S'il ne sait pas désigner un
/// fichier, on ouvre au moins son dossier.
#[tauri::command]
pub fn show_log(app: AppHandle) -> Result<()> {
    let path = journal::path().ok_or_else(|| Error::Msg(t!("errors.no_log")))?;
    let opener = app.opener();
    if path.exists() && opener.reveal_item_in_dir(&path).is_ok() {
        return Ok(());
    }
    let dir = path.parent().unwrap_or(&path);
    opener
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| Error::Msg(t!("errors.open_log", detail = e)))
}

/// Une erreur JavaScript (exception, promesse rejetée, plantage d'un composant React),
/// envoyée par la WebView. Coupée à 4 000 caractères : une pile d'appels suffit.
#[tauri::command]
pub fn log_webview(message: String) {
    let message: String = message.chars().take(4000).collect();
    log::error!(target: "uehub::webview", "{message}");
}
