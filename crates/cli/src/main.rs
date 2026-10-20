mod engines;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use uehub_core::i18n::{self, Lang};
use uehub_core::{
    AssetKind, DownloadSummary, EpicClient, FabAsset, LOGIN_URL, Progress, ProjectAssets,
    RecentProjects, UnrealProject, find_assets, folder_name, parse_version, pick_for_project,
    pick_version, t,
};

// Les textes de l'aide viennent des fichiers de langue : clap les lit quand il construit
// la commande, après i18n::init.
#[derive(Parser)]
#[command(name = "uehub", version, about = t!("cli.about"))]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = t!("cli.help.auth_url"))]
    AuthUrl,
    #[command(about = t!("cli.help.login"))]
    Login { code: String },
    #[command(about = t!("cli.help.whoami"))]
    Whoami,
    #[command(about = t!("cli.help.library"))]
    Library,
    #[command(about = t!("cli.help.download"))]
    Download {
        #[arg(help = t!("cli.help.query"))]
        query: String,
        #[arg(long, help = t!("cli.help.ue_download"))]
        ue: Option<String>,
        #[arg(long, help = t!("cli.help.out"))]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 8, help = t!("cli.help.jobs"))]
        jobs: usize,
    },
    #[command(about = t!("cli.help.add"))]
    Add {
        #[arg(help = t!("cli.help.query"))]
        query: String,
        #[arg(help = t!("cli.help.project"))]
        project: PathBuf,
        #[arg(long, help = t!("cli.help.ue_add"))]
        ue: Option<String>,
        #[arg(long, help = t!("cli.help.force"))]
        force: bool,
        #[arg(long, default_value_t = 8, help = t!("cli.help.jobs"))]
        jobs: usize,
    },
    #[command(about = t!("cli.help.remove"))]
    Remove {
        #[arg(help = t!("cli.help.query_added"))]
        query: String,
        #[arg(help = t!("cli.help.project"))]
        project: PathBuf,
    },
    #[command(about = t!("cli.help.engines"))]
    Engines {
        #[command(subcommand)]
        action: Option<EngineAction>,
    },
    #[command(about = t!("cli.help.logout"))]
    Logout,
}

#[derive(Subcommand)]
enum EngineAction {
    #[command(about = t!("cli.help.install"))]
    Install {
        #[arg(help = t!("cli.help.version"))]
        version: String,
        #[arg(long, help = t!("cli.help.dir"))]
        dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // La langue du système, avant que clap n'écrive l'aide.
    i18n::init(Lang::from_system());
    env_logger::init();
    let cli = Cli::parse();
    let mut client = EpicClient::new()?;

    match cli.command {
        Command::AuthUrl => {
            println!("{}", t!("cli.auth_open", url = LOGIN_URL));
            println!("{}", t!("cli.auth_copy"));
            println!("   uehub login <CODE>");
        }
        Command::Login { code } => {
            client.login_with_code(&code).await?;
            let name = client.display_name().unwrap_or_default();
            println!("{}", t!("cli.signed_in", name = name));
            let storage = client.session_storage();
            println!("{}", t!("account.session_in", storage = storage));
        }
        Command::Whoami => {
            client.resume().await?;
            println!("{}", client.display_name().unwrap_or_default());
            let storage = client.session_storage();
            println!("{}", t!("account.session_in", storage = storage));
        }
        Command::Library => {
            let assets = client.fab_library().await?;
            println!("{}\n", t!("cli.library", n = assets.len()));
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
            let version = pick_version(asset, ue.as_deref()).with_context(|| {
                let ue = ue.as_deref().unwrap_or("?");
                t!("cli.no_version", title = asset.title, ue = ue)
            })?;
            let out = out.unwrap_or_else(|| PathBuf::from(folder_name(&asset.title)));

            println!("{} → {}", asset.title, out.display());
            let versions = version.engine_versions.join(", ");
            println!("{}", t!("cli.compatible", versions = versions));

            let bar = progress_bar()?;
            let summary = client
                .download(asset, version, &out, jobs, &|p| show_progress(&bar, p))
                .await?;
            bar.finish_and_clear();
            print_summary(&summary);
        }
        Command::Add {
            query,
            project,
            ue,
            force,
            jobs,
        } => {
            let project = UnrealProject::open(&project)
                .with_context(|| t!("cli.cant_open", path = project.display()))?;
            let engine = match &ue {
                Some(v) => parse_version(v.trim_start_matches("UE_"))
                    .with_context(|| t!("cli.ue_expected"))?,
                None => project.engine.with_context(|| t!("cli.project_version"))?,
            };
            let ue = format!("{}.{}", engine.0, engine.1);
            println!("{}", t!("cli.project", name = project.name, engine = ue));

            let library = client.fab_library().await?;
            let asset = select_asset(&library, &query)?;
            let kind = AssetKind::of(asset);
            let (version, exact) = pick_for_project(asset, engine)
                .with_context(|| t!("errors.no_version_for", title = asset.title, engine = ue))?;
            if !exact {
                if kind == AssetKind::Plugin {
                    let versions = version.engine_versions.join(", ");
                    bail!(t!("cli.plugin_not_for", engine = ue, versions = versions));
                }
                let used = version.engine_versions.last().map_or("?", String::as_str);
                println!("{}", t!("cli.converted", engine = ue, used = used));
            }
            println!("{} ({})", asset.title, kind_label(kind));

            let bar = progress_bar()?;
            let summary = client
                .add_to_project(asset, version, &project, force, jobs, &|p| {
                    show_progress(&bar, p)
                })
                .await?;
            bar.finish_and_clear();
            print_summary(&summary);

            // Pour que ce projet apparaisse aussi dans l'app.
            if let Ok(mut recent) = RecentProjects::load() {
                let _ = recent.remember(&project.uproject);
            }
        }
        Command::Remove { query, project } => {
            let project = UnrealProject::open(&project)
                .with_context(|| t!("cli.cant_open", path = project.display()))?;
            // On cherche dans les fiches du projet : pas besoin d'Epic ni de réseau.
            let mut assets = ProjectAssets::load(&project.dir);
            let q = query.to_lowercase();
            let found: Vec<_> = assets
                .list()
                .iter()
                .filter(|a| a.asset_id == query || a.title.to_lowercase().contains(&q))
                .map(|a| (a.asset_id.clone(), a.title.clone()))
                .collect();
            let (asset_id, title) = match found.as_slice() {
                [one] => one.clone(),
                [] => bail!(t!(
                    "cli.no_added_match",
                    query = query,
                    project = project.name
                )),
                many => {
                    let titles: Vec<&str> = many.iter().map(|(_, title)| title.as_str()).collect();
                    bail!(t!("cli.several_added", titles = titles.join(", ")))
                }
            };
            let removal = assets.remove(&asset_id)?;
            println!(
                "{}",
                t!(
                    "cli.removed",
                    n = removal.deleted,
                    title = title,
                    project = project.name
                )
            );
            if removal.shared > 0 {
                println!("{}", t!("removal.shared", n = removal.shared));
            }
            if removal.missing > 0 {
                println!("{}", t!("removal.missing", n = removal.missing));
            }
        }
        Command::Engines { action: None } => engines::list(&mut client).await?,
        Command::Engines {
            action: Some(EngineAction::Install { version, dir }),
        } => engines::install(&mut client, &version, dir).await?,
        Command::Logout => {
            client.logout().await?;
            println!("{}", t!("cli.signed_out"));
        }
    }
    Ok(())
}

/// Trouve un seul asset, ou explique pourquoi ce n'est pas possible.
fn select_asset<'a>(library: &'a [FabAsset], query: &str) -> Result<&'a FabAsset> {
    match find_assets(library, query).as_slice() {
        [one] => Ok(*one),
        [] => bail!(t!("cli.no_match", query = query)),
        many => {
            eprintln!("{}", t!("cli.several"));
            for a in many {
                eprintln!("- {}", a.title);
            }
            bail!(t!("cli.refine"));
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
        Progress::Chunks { done, total, .. } => ("chunks".to_string(), done, total),
        Progress::Files { done, total } => (t!("cli.files"), done, total),
    };
    bar.set_message(msg);
    bar.set_length(total as u64);
    bar.set_position(done as u64);
}

fn print_summary(summary: &DownloadSummary) {
    let size = i18n::bytes(summary.bytes);
    let files = t!("summary.files", n = summary.files, size = size);
    println!(
        "{}",
        t!("cli.done", summary = files, dir = summary.dir.display())
    );
    if summary.removed > 0 {
        println!("{}", t!("summary.removed", n = summary.removed));
    }
}

/// "Pack d'assets", "Plugin", "Projet complet", comme dans l'app.
fn kind_label(kind: AssetKind) -> String {
    match kind {
        AssetKind::AssetPack => t!("kinds.pack"),
        AssetKind::Plugin => t!("kinds.plugin"),
        AssetKind::CompleteProject => t!("kinds.project"),
    }
}

#[cfg(test)]
mod tests;
