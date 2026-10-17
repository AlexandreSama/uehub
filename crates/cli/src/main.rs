use anyhow::Result;
use clap::{Parser, Subcommand};
use uehub_core::{EpicClient, LOGIN_URL};

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
        Command::Logout => {
            client.logout().await?;
            println!("Déconnecté.");
        }
    }
    Ok(())
}