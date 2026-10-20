use super::*;
use std::path::PathBuf;

fn list(versions: &[&str]) -> EngineList {
    EngineList {
        checked_at: 1,
        engines: versions
            .iter()
            .map(|v| ListedEngine {
                version: v.to_string(),
                size: 1,
                created_at: None,
            })
            .collect(),
    }
}

fn installed(versions: &[Option<&str>]) -> Vec<InstalledEngine> {
    versions
        .iter()
        .enumerate()
        .map(|(i, v)| InstalledEngine {
            id: format!("id{i}"),
            dir: PathBuf::from(format!("/e/{i}")),
            version: v.map(String::from),
            by_uehub: true,
        })
        .collect()
}

fn update(l: &EngineList, version: &str, inst: &[InstalledEngine]) -> Option<String> {
    l.update_for(version, inst).map(|e| e.version.clone())
}

#[test]
fn newer_patch_of_the_same_branch_is_an_update() {
    let l = list(&["5.9.0", "5.8.4", "5.8.3", "5.8.2", "5.7.4"]);
    let inst = installed(&[Some("5.8.3")]);
    assert_eq!(update(&l, "5.8.3", &inst).as_deref(), Some("5.8.4"));
    // Pas la 5.9.0 : une autre branche.
    let inst = installed(&[Some("5.8.4")]);
    assert_eq!(update(&l, "5.8.4", &inst), None);
    let inst = installed(&[Some("5.7.4")]);
    assert_eq!(update(&l, "5.7.4", &inst), None);
}

#[test]
fn the_newest_wins_and_numbers_compare_as_numbers() {
    // 5.8.10 est plus récente que 5.8.9 ; l'ordre de la liste ne compte pas.
    let l = list(&["5.8.9", "5.8.10", "5.8.2"]);
    let inst = installed(&[Some("5.8.2")]);
    assert_eq!(update(&l, "5.8.2", &inst).as_deref(), Some("5.8.10"));
    // 5.10 n'est pas la branche 5.1.
    let l = list(&["5.10.1", "5.1.0", "5.1.1"]);
    let inst = installed(&[Some("5.1.0")]);
    assert_eq!(update(&l, "5.1.0", &inst).as_deref(), Some("5.1.1"));
}

#[test]
fn nothing_when_the_newest_is_already_installed_or_newer() {
    let l = list(&["5.8.4", "5.8.3"]);
    let inst = installed(&[Some("5.8.3"), Some("5.8.4")]);
    assert_eq!(update(&l, "5.8.3", &inst), None);
    // Une 5.8.5 compilée à la main est plus récente que tout ce que propose Epic.
    let inst = installed(&[Some("5.8.3"), Some("5.8.5")]);
    assert_eq!(update(&l, "5.8.3", &inst), None);
    // La 5.8.2 et la 5.8.3 installées : la 5.8.3 porte la mise à jour, la 5.8.2 est remplacée.
    let inst = installed(&[Some("5.8.2"), Some("5.8.3")]);
    assert_eq!(update(&l, "5.8.2", &inst), None);
    assert_eq!(update(&l, "5.8.3", &inst).as_deref(), Some("5.8.4"));
    // Une branche plus récente installée ne compte pas.
    let inst = installed(&[Some("5.8.3"), Some("5.9.0")]);
    assert_eq!(update(&l, "5.8.3", &inst).as_deref(), Some("5.8.4"));
    // Version inconnue, liste vide, branche absente de la liste.
    assert_eq!(update(&l, "", &inst), None);
    assert_eq!(update(&list(&[]), "5.8.3", &inst), None);
    assert_eq!(update(&l, "5.6.1", &installed(&[Some("5.6.1")])), None);
    // Les moteurs sans version (pas de Build.version) sont ignorés.
    let inst = installed(&[Some("5.8.3"), None]);
    assert_eq!(update(&l, "5.8.3", &inst).as_deref(), Some("5.8.4"));
}

#[test]
fn replaced_by_the_newest_installed_of_the_branch() {
    let inst = installed(&[
        Some("5.9.0"),
        Some("5.8.4"),
        Some("5.8.3"),
        Some("5.8.2"),
        None,
    ]);
    assert_eq!(replaced_by("5.8.3", &inst), Some("5.8.4"));
    assert_eq!(replaced_by("5.8.2", &inst), Some("5.8.4"));
    assert_eq!(replaced_by("5.8.4", &inst), None);
    assert_eq!(replaced_by("5.9.0", &inst), None);
    assert_eq!(replaced_by("", &inst), None);
    let inst = installed(&[Some("5.8.10"), Some("5.8.9")]);
    assert_eq!(replaced_by("5.8.9", &inst), Some("5.8.10"));
}
