//! `uehub engines` : les moteurs Unreal précompilés pour Linux.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use uehub_core::{
    Downloaded, EULA_URL, EngineInstall, EpicClient, InstallProgress, installed_engines,
    replaced_by,
};

/// `uehub engines` : les moteurs installés, puis ceux qu'Epic propose.
pub async fn list(client: &mut EpicClient) -> Result<()> {
    let installed = installed_engines()?;
    println!("Moteurs installés :");
    if installed.is_empty() {
        println!("  aucun");
    }
    for engine in &installed {
        let version = engine.version.as_deref().unwrap_or("?");
        // Un plus récent de la même branche est là : celui-ci peut partir.
        let replaced = replaced_by(version, &installed)
            .map(|newer| format!("   (remplacé par {newer})"))
            .unwrap_or_default();
        println!("  UE {version:<8} {}{replaced}", engine.dir.display());
    }

    let session = client.open_unreal_session().await?;
    if !session.eula_accepted().await? {
        // Sans CLUF accepté, la page Linux ne propose aucun zip.
        println!("\nAccepte d'abord le CLUF d'Unreal Engine : {EULA_URL}");
        return Ok(());
    }
    println!("\nMoteurs proposés par Epic :");
    let list = session.engine_list().await?;
    for engine in &list.engines {
        let day = engine.created_at.as_deref().and_then(|d| d.get(..10));
        let mark = if installed
            .iter()
            .any(|i| i.version.as_deref() == Some(engine.version.as_str()))
        {
            "   (installé)"
        } else {
            ""
        };
        println!(
            "  UE {:<8} {:>8}   {}{mark}",
            engine.version,
            gigabytes(engine.size),
            day.unwrap_or("")
        );
    }
    
    // Les correctifs sortis pour les moteurs installés, avec la commande qui les installe.
    let updates: Vec<_> = installed
        .iter()
        .filter_map(|e| {
            let version = e.version.as_deref()?;
            Some((version, list.update_for(version, &installed)?))
        })
        .collect();
    if !updates.is_empty() {
        println!("\nMises à jour :");
        for (version, update) in updates {
            println!(
                "  UE {version} → {}   uehub engines install {}",
                update.version, update.version
            );
        }
    }
    println!("\nPour en installer un : uehub engines install 5.8.3");
    Ok(())
}

/// `uehub engines install <version>` : télécharge, extrait et enregistre un moteur.
pub async fn install(client: &mut EpicClient, version: &str, dir: Option<PathBuf>) -> Result<()> {
    let session = client.open_unreal_session().await?;
    if !session.eula_accepted().await? {
        bail!("accepte d'abord le CLUF d'Unreal Engine : {EULA_URL}");
    }
    let mut engine = session.engine_download(version).await?;
    let install = EngineInstall::new(&engine, dir)?;
    if install.is_installed() {
        // Il manque peut-être l'enregistrement (s'il avait échoué) : on le refait.
        let id = install.register()?;
        println!(
            "UE {version} est déjà installé dans {} (identifiant {id})",
            install.dir.display()
        );
        return Ok(());
    }
    install.check_space()?;
    println!(
        "UE {version} ({}) → {}",
        gigabytes(engine.size),
        install.dir.display()
    );

    // 1. Le zip. L'adresse ne vaut qu'une heure : quand elle expire, on en redemande
    //    une à la page Linux et on reprend là où le fichier s'est arrêté.
    let bar = new_bar(
        "{msg:11} [{bar:40}] {decimal_bytes}/{decimal_total_bytes} ({decimal_bytes_per_sec}, reste {eta})",
    )?;
    bar.set_message("zip");
    let show = |p: InstallProgress| show_progress(&bar, p);
    let mut stuck = 0;
    loop {
        let before = install.downloaded();
        let outcome = install.download(&engine, &show).await?;
        if outcome == Downloaded::Complete {
            break;
        }
        // Trois essais de suite sans un octet de plus : on abandonne.
        stuck = if install.downloaded() > before {
            0
        } else {
            stuck + 1
        };
        if stuck >= 3 {
            bail!("le téléchargement n'avance plus : relance la commande pour reprendre");
        }
        if outcome == Downloaded::Expired {
            bar.println("adresse expirée, j'en demande une nouvelle");
            engine = client
                .open_unreal_session()
                .await?
                .engine_download(version)
                .await?;
        } else {
            bar.println("connexion coupée, je reprends");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    bar.finish_and_clear();

    // 2. L'extraction, puis l'enregistrement.
    let bar = new_bar("{msg:11} [{bar:40}] {pos}/{len} fichiers ({eta})")?;
    bar.set_message("extraction");
    let files = install
        .extract(&|p: InstallProgress| show_progress(&bar, p))
        .await?;
    bar.finish_and_clear();
    let id = install.register()?;

    println!(
        "Installé : UE {version} ({files} fichiers) dans {}",
        install.dir.display()
    );
    println!("Enregistré dans Install.ini sous l'identifiant {id}");
    println!(
        "Pour le lancer : {}",
        install
            .dir
            .join("Engine/Binaries/Linux/UnrealEditor")
            .display()
    );
    Ok(())
}

fn new_bar(template: &str) -> Result<ProgressBar> {
    let bar = ProgressBar::new(0);
    bar.set_style(ProgressStyle::with_template(template)?.progress_chars("=> "));
    Ok(bar)
}

fn show_progress(bar: &ProgressBar, progress: InstallProgress) {
    let (done, total) = match progress {
        InstallProgress::Download { done, total } => (done, total),
        InstallProgress::Extract { done, total } => (done as u64, total as u64),
    };
    bar.set_length(total);
    bar.set_position(done);
}

/// 39 818 576 005 → "39,8 Go"
fn gigabytes(bytes: u64) -> String {
    format!("{:.1} Go", bytes as f64 / 1e9).replace('.', ",")
}