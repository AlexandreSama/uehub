//! L'application Tauri : la fenêtre, les plugins et les commandes appelées par React.

mod commands;

use tauri::Manager;
use uehub_core::EpicClient;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Recharge la session enregistrée (trousseau ou fichier) dès le démarrage.
            let client = EpicClient::new()?;
            app.manage(AppState::new(client));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session,
            commands::open_login_page,
            commands::login,
            commands::logout,
            commands::library,
            commands::download,
            commands::add_to_project,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}