//! Lancer l'éditeur d'un moteur installé, seul ou sur un projet.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use super::install::{EDITOR, InstalledEngine};
use super::version_key;
use crate::{CoreError, Result, UnrealProject};

/// Le moteur installé qui doit ouvrir ce projet : celui que son .uproject désigne
/// par identifiant, sinon le plus récent de la même version (5.4 → 5.4.4 plutôt que 5.4.1).
pub fn engine_for<'a>(
    installed: &'a [InstalledEngine],
    project: &UnrealProject,
) -> Option<&'a InstalledEngine> {
    let wanted = project.association.trim_matches(['{', '}']);
    if let Some(engine) = installed
        .iter()
        .find(|e| !wanted.is_empty() && e.id.eq_ignore_ascii_case(wanted))
    {
        return Some(engine);
    }
    let (major, minor) = project.engine?;
    let branch = format!("{major}.{minor}.");
    installed
        .iter()
        .filter(|e| e.version.as_deref().is_some_and(|v| v.starts_with(&branch)))
        .max_by_key(|e| e.version.as_deref().map(version_key))
}

/// Lance UnrealEditor, avec le projet à ouvrir s'il y en a un. uehub n'attend pas
/// l'éditeur : il continue de tourner si on ferme uehub.
pub fn launch_editor(engine: &InstalledEngine, project: Option<&Path>) -> Result<()> {
    let editor = engine.dir.join(EDITOR);
    if !editor.is_file() {
        return Err(CoreError::Engine(format!(
            "{} introuvable : le moteur est-il complet ?",
            editor.display()
        )));
    }
    let mut command = Command::new(&editor);
    if let Some(project) = project {
        command.arg(project);
    }
    let mut child = command
        .current_dir(&engine.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Son propre groupe de processus : un Ctrl+C dans le terminal de `tauri dev`
        // n'arrête pas l'éditeur avec l'app.
        .process_group(0)
        .spawn()
        .map_err(|e| CoreError::Engine(format!("UnrealEditor ne démarre pas : {e}")))?;
    // Quand l'éditeur se ferme, ce thread recueille sa fin : sans lui, il resterait
    // un processus « zombie » jusqu'à la fermeture de uehub.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}