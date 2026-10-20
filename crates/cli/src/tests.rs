use super::*;
use clap::CommandFactory;

#[test]
fn the_commands_are_well_formed() {
    // clap vérifie ses propres règles : un nom en double, un argument mal déclaré…
    Cli::command().debug_assert();
}

#[test]
fn remove_takes_a_query_and_a_project() {
    let cli = Cli::try_parse_from(["uehub", "remove", "grass", "Forest/Forest.uproject"]).unwrap();
    let Command::Remove { query, project } = cli.command else {
        panic!("ce n'est pas la commande remove");
    };
    assert_eq!(query, "grass");
    assert_eq!(project, PathBuf::from("Forest/Forest.uproject"));
    let missing = Cli::try_parse_from(["uehub", "remove", "grass"]);
    assert!(missing.is_err(), "le projet est obligatoire");
}

#[test]
fn add_and_engines_install_have_defaults() {
    let cli = Cli::try_parse_from(["uehub", "add", "grass", "P.uproject"]).unwrap();
    let Command::Add {
        ue, force, jobs, ..
    } = cli.command
    else {
        panic!("ce n'est pas la commande add");
    };
    assert_eq!((ue, force, jobs), (None, false, 8));

    let cli = Cli::try_parse_from(["uehub", "engines", "install", "5.8.3"]).unwrap();
    let Command::Engines {
        action: Some(EngineAction::Install { version, dir }),
    } = cli.command
    else {
        panic!("ce n'est pas uehub engines install");
    };
    assert_eq!(version, "5.8.3");
    assert_eq!(dir, None);
}
