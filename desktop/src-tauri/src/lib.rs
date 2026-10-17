//! L'application Tauri : la fenêtre, les plugins et les commandes appelées par React.

mod commands;
mod thumbs;

use tauri::Manager;
use uehub_core::{EpicClient, ThumbCache};

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
            app.manage(ThumbCache::new()?);
            Ok(())
        })
        // Chaque <img src="thumb://…"> arrive ici. On répond depuis une tâche à part
        // pour ne pas bloquer la fenêtre pendant un téléchargement.
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let cache = app.state::<ThumbCache>();
                responder.respond(thumbs::respond(&cache, request).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::session,
            commands::open_login_page,
            commands::login,
            commands::logout,
            commands::library,
            commands::download,
            commands::add_to_project,
            commands::cancel_job,
            commands::recent_projects,
            commands::forget_project,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}