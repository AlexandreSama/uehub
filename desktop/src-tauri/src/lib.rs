//! L'application Tauri : la fenêtre, les plugins et les commandes appelées par React.

mod commands;
mod notify;
mod queue;
mod thumbs;
mod settings;

use tauri::Manager;
use uehub_core::{EpicClient, ThumbCache};

use commands::AppState;
use queue::Queue;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Recharge la session enregistrée (trousseau ou fichier) dès le démarrage.
            let client = EpicClient::new()?;
            app.manage(AppState::new(client));
            app.manage(ThumbCache::new()?);
            // La file enregistrée revient en pause ; le worker attend qu'on la relance.
            app.manage(Queue::load(app.handle().clone())?);
            tauri::async_runtime::spawn(queue::worker(app.handle().clone()));
            tauri::async_runtime::spawn(queue::worker(app.handle().clone()));
            notify::setup(app.handle())?;
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
            commands::enqueue,
            commands::jobs,
            commands::pause_job,
            commands::resume_job,
            commands::remove_job,
            commands::resume_all,
            commands::clear_finished,
            commands::recent_projects,
            commands::forget_project,
            commands::added_assets,
            commands::local_engines,
            commands::available_engines,
            commands::engines_folder,
            commands::set_engines_folder,
            commands::open_eula_page,
            commands::launch_engine,
            commands::open_project,
            commands::remove_engine,
            commands::free_space,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}