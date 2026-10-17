use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use uehub_core::{
    DownloadSummary, EpicClient, FabAsset, LOGIN_URL, Progress, find_assets, folder_name,
    pick_version,
};

#[derive(Parser)]
#[command(name = "uehub", version, about = "Client Fab pour Linux")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Affiche l'URL de connexion Epic
    AuthUrl,
    /// Se connecte avec le code `authorizationCode`
    Login { code: String },
    /// Affiche le compte connecté
    Whoami,
    /// Liste ta bibliothèque Fab
    Library,
    /// Télécharge un asset de ta bibliothèque
    Download {
        /// Titre, morceau de titre ou asset_id
        query: String,
        /// Version d'Unreal visée, ex. 5.4 (par défaut : la plus récente)
        #[arg(long)]
        ue: Option<String>,
        /// Dossier de destination (par défaut : ./<titre de l'asset>)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Nombre de chunks téléchargés en parallèle
        #[arg(long, default_value_t = 8)]
        jobs: usize,
    },
    /// Se déconnecte et supprime la session locale
    Logout,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let mut client = EpicClient::new()?;

    match cli.command {
        Command::AuthUrl => {
            println!("1. Ouvre cette page et connecte-toi :\n\n{LOGIN_URL}\n");
            println!("2. Copie la valeur de \"authorizationCode\" puis lance :");
            println!("   uehub login <CODE>");
        }
        Command::Login { code } => {
            client.login_with_code(&code).await?;
            let name = client.display_name().unwrap_or_default();
            println!("Connecté en tant que {name}");
        }
        Command::Whoami => {
            client.resume().await?;
            println!("{}", client.display_name().unwrap_or_default());
        }
        Command::Library => {
            let assets = client.fab_library().await?;
            println!("{} élément(s) dans ta bibliothèque Fab\n", assets.len());
            for asset in &assets {
                let versions: Vec<&str> = asset
                    .project_versions
                    .iter()
                    .flat_map(|v| v.engine_versions.iter().map(String::as_str))
                    .collect();
                println!("- {}  [{}]", asset.title, versions.join(", "));
            }
        }
        Command::Download {
            query,
            ue,
            out,
            jobs,
        } => {
            let library = client.fab_library().await?;
            let asset = select_asset(&library, &query)?;
            let version = pick_version(asset, ue.as_deref())
                .with_context(|| format!("aucune version de « {} » pour UE {ue:?}", asset.title))?;
            let out = out.unwrap_or_else(|| PathBuf::from(folder_name(&asset.title)));

            println!("{} → {}", asset.title, out.display());
            println!("Compatible : {}", version.engine_versions.join(", "));

            let bar = progress_bar()?;
            let summary = client
                .download(asset, version, &out, jobs, &|p| show_progress(&bar, p))
                .await?;
            bar.finish_and_clear();
            print_summary(&summary);
        }
        Command::Logout => {
            client.logout().await?;
            println!("Déconnecté.");
        }
    }
    Ok(())
}

/// Trouve un seul asset, ou explique pourquoi ce n'est pas possible.
fn select_asset<'a>(library: &'a [FabAsset], query: &str) -> Result<&'a FabAsset> {
    match find_assets(library, query).as_slice() {
        [one] => Ok(*one),
        [] => bail!("aucun asset ne correspond à « {query} »"),
        many => {
            eprintln!("Plusieurs assets correspondent :");
            for a in many {
                eprintln!("- {}", a.title);
            }
            bail!("précise ta recherche");
        }
    }
}

fn progress_bar() -> Result<ProgressBar> {
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("{msg:9} [{bar:40}] {pos}/{len} ({eta})")?
            .progress_chars("=> "),
    );
    Ok(bar)
}

fn show_progress(bar: &ProgressBar, p: Progress) {
    let (msg, done, total) = match p {
        Progress::Chunks { done, total } => ("chunks", done, total),
        Progress::Files { done, total } => ("fichiers", done, total),
    };
    bar.set_message(msg);
    bar.set_length(total as u64);
    bar.set_position(done as u64);
}

fn print_summary(summary: &DownloadSummary) {
    println!(
        "Terminé : {} fichiers, {:.1} Mo dans {}",
        summary.files,
        summary.bytes as f64 / 1_000_000.0,
        summary.dir.display()
    );
}