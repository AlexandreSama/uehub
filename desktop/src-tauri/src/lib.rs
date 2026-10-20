//! L'application Tauri : la fenêtre, les plugins et les commandes appelées par React.

mod commands;
mod journal;
mod notify;
mod queue;
mod settings;
mod thumbs;
mod update;

use tauri::Manager;
use uehub_core::i18n::{self, Lang};
use uehub_core::{EpicClient, ThumbCache};

use commands::AppState;
use queue::Queue;
use update::PendingUpdate;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // La langue d'abord : tout ce qui suit parle déjà celle du système. Puis le journal,
    // pour qu'il voie tout le reste, même un échec au démarrage.
    i18n::init(Lang::from_system());
    uehub_core::journal::start();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            journal::started(app.handle());
            // Recharge la session enregistrée (trousseau ou fichier) dès le démarrage.
            let client = EpicClient::new()?;
            app.manage(AppState::new(client));
            app.manage(ThumbCache::new()?);
            app.manage(PendingUpdate::default());
            // La file enregistrée revient en pause ; le worker attend qu'on la relance.
            app.manage(Queue::load(app.handle().clone())?);
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
            commands::open_fab,
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
            commands::remove_asset,
            commands::added_assets,
            commands::local_engines,
            commands::cached_engines,
            commands::available_engines,
            commands::engines_folder,
            commands::set_engines_folder,
            commands::free_space,
            commands::open_eula_page,
            commands::launch_engine,
            commands::open_project,
            commands::remove_engine,
            update::app_info,
            update::check_update,
            update::install_update,
            journal::language,
            journal::log_file,
            journal::show_log,
            journal::log_webview,
        ])
        .run(tauri::generate_context!())
        .expect("uehub could not start");
}
