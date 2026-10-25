//! `uehub engines` : les moteurs Unreal précompilés pour Linux.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use uehub_core::i18n::bytes;
use uehub_core::{
    Downloaded, EULA_URL, EngineInstall, EpicClient, InstallProgress, installed_engines,
    replaced_by, t,
};

/// `uehub engines` : les moteurs installés, puis ceux qu'Epic propose.
pub async fn list(client: &mut EpicClient) -> Result<()> {
    let installed = installed_engines()?;
    println!("{}", t!("cli.installed_engines"));
    if installed.is_empty() {
        println!("  {}", t!("cli.none"));
    }
    for engine in &installed {
        let version = engine.version.as_deref().unwrap_or("?");
        // Un plus récent de la même branche est là : celui-ci peut partir.
        let replaced = replaced_by(version, &installed)
            .map(|newer| format!("   {}", t!("cli.replaced_by", version = newer)))
            .unwrap_or_default();
        println!("  UE {version:<8} {}{replaced}", engine.dir.display());
    }

    let session = client.open_unreal_session().await?;
    if !session.eula_accepted().await? {
        // Sans CLUF accepté, la page Linux ne propose aucun zip.
        println!("\n{}", t!("cli.accept_eula", url = EULA_URL));
        return Ok(());
    }
    println!("\n{}", t!("cli.offered_engines"));
    let list = session.engine_list().await?;
    for engine in &list.engines {
        let day = engine.created_at.as_deref().and_then(|d| d.get(..10));
        let mark = if installed
            .iter()
            .any(|i| i.version.as_deref() == Some(engine.version.as_str()))
        {
            format!("   {}", t!("cli.installed_mark"))
        } else {
            String::new()
        };
        println!(
            "  UE {:<8} {:>8}   {}{mark}",
            engine.version,
            bytes(engine.size),
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
        println!("\n{}", t!("cli.updates"));
        for (version, update) in updates {
            println!(
                "  UE {version} → {}   uehub engines install {}",
                update.version, update.version
            );
        }
    }
    println!("\n{}", t!("cli.install_hint"));
    Ok(())
}

/// `uehub engines install <version>` : télécharge, extrait et enregistre un moteur.
pub async fn install(client: &mut EpicClient, version: &str, dir: Option<PathBuf>) -> Result<()> {
    let session = client.open_unreal_session().await?;
    if !session.eula_accepted().await? {
        bail!(t!("cli.accept_eula", url = EULA_URL));
    }
    let mut engine = session.engine_download(version).await?;
    let install = EngineInstall::new(&engine, dir)?;
    if install.is_installed() {
        // Il manque peut-être l'enregistrement (s'il avait échoué) : on le refait.
        let id = install.register()?;
        let dir = install.dir.display();
        println!(
            "{}",
            t!(
                "cli.already_installed",
                version = version,
                dir = dir,
                id = id
            )
        );
        return Ok(());
    }
    install.check_space()?;
    println!(
        "UE {version} ({}) → {}",
        bytes(engine.size),
        install.dir.display()
    );

    // 1. Le moteur, extrait pendant qu'il se télécharge. L'adresse ne vaut qu'une heure :
    //    quand elle expire, on en redemande une à la page Linux et on reprend au fichier
    //    en cours.
    let bar = new_bar(&t!("cli.zip_bar"))?;
    bar.set_message(t!("cli.installing"));
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
            bail!(t!("cli.stuck"));
        }
        if outcome == Downloaded::Expired {
            bar.println(t!("cli.expired"));
            engine = client
                .open_unreal_session()
                .await?
                .engine_download(version)
                .await?;
        } else {
            bar.println(t!("cli.resuming"));
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    bar.finish_and_clear();

    // 2. La fin, puis l'enregistrement. Avec le zip sur le disque (un serveur qui ignore
    //    Range, ou une installation commencée avec uehub 0.2), il faut d'abord l'extraire.
    let files = if install.zip_on_disk() {
        let bar = new_bar(&t!("cli.extract_bar"))?;
        bar.set_message(t!("cli.extracting"));
        let files = install
            .extract(&|p: InstallProgress| show_progress(&bar, p))
            .await?;
        bar.finish_and_clear();
        files
    } else {
        install.extract(&|_| {}).await?
    };
    let id = install.register()?;

    let dir = install.dir.display();
    println!(
        "{}",
        t!("cli.installed", n = files, version = version, dir = dir)
    );
    println!("{}", t!("cli.registered", id = id));
    let editor = install.dir.join("Engine/Binaries/Linux/UnrealEditor");
    println!("{}", t!("cli.to_launch", path = editor.display()));
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
