use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn engine(id: &str, version: &str, dir: &str) -> InstalledEngine {
    InstalledEngine {
        id: id.into(),
        dir: PathBuf::from(dir),
        version: Some(version.into()),
        by_uehub: true,
    }
}

fn project(association: &str, engine: Option<(u32, u32)>) -> UnrealProject {
    UnrealProject {
        uproject: "/p/P.uproject".into(),
        dir: "/p".into(),
        name: "P".into(),
        engine,
        association: association.into(),
    }
}

#[test]
fn picks_the_engine_of_the_project() {
    let installed = [
        engine("AAAA-1", "5.4.1", "/e/541"),
        engine("BBBB-2", "5.4.4", "/e/544"),
        engine("CCCC-3", "5.8.3", "/e/583"),
        engine("DDDD-4", "5.40.0", "/e/5400"),
    ];
    let dir = |p: &UnrealProject| engine_for(&installed, p).map(|e| e.dir.clone());
    // Par version : le correctif le plus récent de la branche, jamais 5.40 pour 5.4.
    assert_eq!(dir(&project("5.4", Some((5, 4)))), Some("/e/544".into()));
    // Par identifiant (accolades et casse comprises), même s'il est plus ancien.
    assert_eq!(
        dir(&project("{aaaa-1}", Some((5, 4)))),
        Some("/e/541".into())
    );
    // Un identifiant inconnu : on se rabat sur la version.
    assert_eq!(dir(&project("{ZZZZ}", Some((5, 8)))), Some("/e/583".into()));
    // Pas installé, ou version inconnue.
    assert_eq!(dir(&project("5.6", Some((5, 6)))), None);
    assert_eq!(dir(&project("", None)), None);
}

#[test]
fn launches_the_editor_with_the_project() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let bin = root.join("Engine/Binaries/Linux");
    fs::create_dir_all(&bin).unwrap();
    // Un faux éditeur qui note où il a démarré et ce qu'il a reçu.
    let out = root.join("args.txt");
    let script = format!(
        "#!/bin/sh\necho \"$PWD|$*\" > {}.tmp\nmv {0}.tmp {0}\n",
        out.display()
    );
    fs::write(bin.join("UnrealEditor"), script).unwrap();
    fs::set_permissions(bin.join("UnrealEditor"), fs::Permissions::from_mode(0o755)).unwrap();
    let e = InstalledEngine {
        id: "X".into(),
        dir: root.to_path_buf(),
        version: Some("5.8.3".into()),
        by_uehub: true,
    };
    launch_editor(&e, Some(Path::new("/p/My Game.uproject"))).unwrap();
    // uehub n'attend pas l'éditeur : on attend nous-mêmes son fichier, cinq secondes au plus.
    for _ in 0..100 {
        if out.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(
        fs::read_to_string(&out).unwrap().trim(),
        format!("{}|/p/My Game.uproject", root.display())
    );
    let missing = InstalledEngine {
        dir: root.join("nope"),
        ..e
    };
    let err = launch_editor(&missing, None).unwrap_err().to_string();
    assert!(err.contains("introuvable"), "{err}");
}
