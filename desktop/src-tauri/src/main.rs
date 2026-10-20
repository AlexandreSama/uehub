// Évite une fenêtre de console sous Windows en release. Ne pas retirer.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nvidia_explicit_sync();
    uehub_desktop_lib::run()
}

/// Les pilotes NVIDIA récents, sous Wayland : avec la synchronisation explicite, la fenêtre
/// de WebKitGTK peut rester vide ou se fermer sur « Error 71 (Protocol error) dispatching to
/// Wayland display ». Le pilote NVIDIA lit cette variable en se chargeant ; les autres
/// pilotes l'ignorent. Posée ici, elle vaut pour l'app lancée depuis le menu comme pour
/// `npm run tauri dev`. Si elle est déjà définie, uehub la laisse telle quelle.
fn nvidia_explicit_sync() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
        // SAFETY : tout au début de main, avant qu'un thread ne lise l'environnement.
        unsafe { std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1") };
    }
}
