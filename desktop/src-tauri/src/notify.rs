//! Les notifications de bureau : une à la fin de chaque job, quand tu ne regardes pas l'app.

#[cfg(test)]
mod tests;

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::queue::{Job, Request, Status};

/// Un clic sur une notification ramène la fenêtre au premier plan.
pub fn setup(app: &AppHandle) -> tauri_plugin_notification::Result<()> {
    let handle = app.clone();
    app.notification().on_action(move |action| {
        if action.action_id() == "tap" {
            bring_to_front(&handle);
        }
    })
}

fn bring_to_front(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Prévient que `job` est terminé, sauf si la fenêtre est déjà au premier plan :
/// la barre du bas l'affiche alors sous tes yeux.
pub fn job_finished(app: &AppHandle, job: &Job) {
    let focused = app
        .get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    if focused {
        return;
    }
    let Some((title, body)) = message(job) else {
        return;
    };
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification non affichée : {e}");
    }
}

/// Le titre et le texte de la notification, ou `None` si le job n'est pas terminé.
fn message(job: &Job) -> Option<(String, String)> {
    match &job.status {
        Status::Done { summary } => {
            let title = match job.request {
                Request::Download { .. } => format!("Téléchargement terminé : {}", job.title),
                Request::AddToProject { .. } => format!("Ajout terminé : {}", job.title),
                Request::InstallEngine { .. } => format!("Installation terminée : {}", job.title),
            };
            let mut body = format!("{} dans {}", summary.text(), job.target);
            if let Some(note) = summary.note() {
                body = format!("{body}\n{note}");
            }
            Some((title, body))
        }
        Status::Failed { error } => Some((format!("Échec : {}", job.title), error.clone())),
        Status::Waiting | Status::Running | Status::Paused => None,
    }
}
