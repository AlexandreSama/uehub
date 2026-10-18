//! Les réglages de l'app, rangés dans settings.json à côté de la file (queue.json).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// En JSON : {"enginesFolder":"/home/alex/UnrealEngine"}. Un réglage absent du fichier
/// prend sa valeur par défaut : on peut en ajouter sans casser les anciens fichiers.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Le dossier où installer les moteurs, s'il a été changé
    pub engines_folder: Option<PathBuf>,
}

impl Settings {
    /// Les réglages enregistrés, ou ceux par défaut si le fichier manque ou est abîmé.
    pub fn load(app: &AppHandle) -> Self {
        file(app)
            .ok()
            .and_then(|file| std::fs::read(file).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let file = file(app).map_err(|e| e.to_string())?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&file, json).map_err(|e| e.to_string())
    }
}

/// ~/.local/share/<identifiant de l'app>/settings.json
fn file(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join("settings.json"))
}