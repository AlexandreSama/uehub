//! Les moteurs Unreal précompilés pour Linux : la liste d'Epic, leur installation,
//! et les moteurs déjà installés sur la machine.

mod cache;
mod install;
mod launch;
mod page;
mod session;
#[cfg(test)]
mod tests;
mod updates;

pub use cache::{EngineList, ListedEngine};
pub use install::{
    Downloaded, EngineInstall, InstallProgress, InstalledEngine, engines_dir, extracted_size,
    free_space, installed_engines, uninstall_engine,
};
pub use launch::{engine_for, launch_editor};
pub use session::UnrealSession;
pub use updates::replaced_by;

use crate::CoreError;

/// La page où accepter le CLUF d'Unreal Engine : sans lui, Epic ne propose aucun zip.
pub const EULA_URL: &str = "https://www.unrealengine.com/eulacheck/unreal?state=%2Flinux";

/// Un zip du moteur proposé sur unrealengine.com/linux.
#[derive(Clone)]
pub struct EngineDownload {
    /// "5.8.3"
    pub version: String,
    /// "Linux_Unreal_Engine_5.8.3.zip"
    pub file_name: String,
    /// Taille du zip en octets
    pub size: u64,
    /// Date de mise en ligne, ex. "2026-09-22T12:45:18.215Z"
    pub created_at: Option<String>,
    /// Adresse de téléchargement signée, valable une heure. Elle ne sort pas du core.
    url: String,
}

/// "Linux_Unreal_Engine_5.8.3.zip" → "5.8.3"
fn engine_version(file_name: &str) -> Option<String> {
    let version = file_name
        .strip_prefix("Linux_Unreal_Engine_")?
        .strip_suffix(".zip")?;
    let valid = !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    valid.then(|| version.to_string())
}

/// "5.10.0" → [5, 10, 0] : 5.10 passe bien après 5.9.
fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// Une erreur réseau sans l'adresse appelée : elle peut contenir un code de session
/// ou une signature de téléchargement.
fn net(e: reqwest::Error) -> CoreError {
    CoreError::Http(e.without_url())
}
