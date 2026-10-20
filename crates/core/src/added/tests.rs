use super::*;
use egs_api::api::types::fab_library::BuildVersion;

fn version(id: &str, engines: &[&str], build: &str) -> ProjectVersion {
    ProjectVersion {
        artifact_id: id.into(),
        engine_versions: engines.iter().map(|e| format!("UE_{e}")).collect(),
        build_versions: if build.is_empty() {
            vec![]
        } else {
            vec![BuildVersion {
                build_version: build.into(),
                platform: "Windows".into(),
            }]
        },
        ..Default::default()
    }
}

fn asset(kind: &str, versions: Vec<ProjectVersion>) -> FabAsset {
    FabAsset {
        asset_id: "grass".into(),
        title: "Megascans: Grass".into(),
        distribution_method: kind.into(),
        project_versions: versions,
        ..Default::default()
    }
}

/// Une fiche qui cite ces fichiers.
fn fiche(id: &str, files: &[&str]) -> AddedAsset {
    AddedAsset {
        asset_id: id.into(),
        title: format!("Asset {id}"),
        artifact_id: format!("{id}-5.3"),
        build_version: "1.0-Windows".into(),
        engine: "5.3".into(),
        added_at: 1,
        files: files.iter().map(|f| f.to_string()).collect(),
    }
}

/// Un faux projet, avec ses dossiers Content et Plugins.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("Content")).unwrap();
    std::fs::create_dir_all(dir.path().join("Plugins")).unwrap();
    dir
}

/// Écrit ces fichiers dans le projet ; chacun contient son propre chemin.
fn write(dir: &Path, files: &[&str]) {
    for f in files {
        let path = dir.join(f);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, f.as_bytes()).unwrap();
    }
}

#[test]
fn new_records_the_installed_version() {
    let a = asset(
        "ASSET_PACK",
        vec![version("art-old", &["5.0", "5.1", "5.3"], "1.0-Windows")],
    );
    let added = AddedAsset::new(&a, &a.project_versions[0]);
    assert_eq!(added.asset_id, "grass");
    assert_eq!(added.artifact_id, "art-old");
    assert_eq!(added.build_version, "1.0-Windows");
    assert_eq!(added.engine, "5.3");
    assert!(added.added_at > 1_700_000_000);
    assert!(added.files.is_empty());
}

#[test]
fn update_detection() {
    let installed = asset("ASSET_PACK", vec![version("art53", &["5.2", "5.3"], "1.0")]);
    let added = AddedAsset::new(&installed, &installed.project_versions[0]);

    // Rien n'a changé.
    assert_eq!(update_for(&added, &installed, (5, 4)), None);
    assert_eq!(update_for(&added, &installed, (5, 3)), None);
    // Le vendeur a publié une nouvelle build de la même version.
    let rebuilt = asset("ASSET_PACK", vec![version("art53", &["5.2", "5.3"], "1.1")]);
    assert_eq!(
        update_for(&added, &rebuilt, (5, 4)),
        Some(AssetUpdate::NewBuild)
    );
    // Une version pour le moteur du projet est apparue…
    let closer = asset(
        "ASSET_PACK",
        vec![
            version("art53", &["5.2", "5.3"], "1.0"),
            version("art54", &["5.4"], "1.0"),
        ],
    );
    assert_eq!(
        update_for(&added, &closer, (5, 4)),
        Some(AssetUpdate::CloserEngine("5.4".into()))
    );
    // … mais un projet 5.3 a déjà la bonne.
    assert_eq!(update_for(&added, &closer, (5, 3)), None);
    // Le même moteur sous un nouvel identifiant : une nouvelle publication.
    let renamed = asset(
        "ASSET_PACK",
        vec![version("art53b", &["5.2", "5.3"], "1.0")],
    );
    assert_eq!(
        update_for(&added, &renamed, (5, 4)),
        Some(AssetUpdate::NewBuild)
    );
    // La version installée a été retirée, il ne reste qu'une plus ancienne : rien de mieux.
    let older = asset("ASSET_PACK", vec![version("art52", &["5.2"], "2.0")]);
    assert_eq!(update_for(&added, &older, (5, 4)), None);
    // Plus rien ne va au projet.
    let newer_only = asset("ASSET_PACK", vec![version("art56", &["5.6"], "1.0")]);
    assert_eq!(update_for(&added, &newer_only, (5, 4)), None);
    // Une fiche sans numéro de build (Fab n'en donnait pas) ne signale jamais de nouvelle build.
    let mut no_build = added.clone();
    no_build.build_version.clear();
    assert_eq!(update_for(&no_build, &rebuilt, (5, 4)), None);
}

#[test]
fn records_are_saved_in_the_project_and_replaced_per_asset() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let empty = ProjectAssets::load(dir);
    assert!(empty.list().is_empty());
    assert!(!dir.join("Saved").exists(), "lire n'écrit rien");

    let a = asset("ASSET_PACK", vec![version("art53", &["5.3"], "1.0")]);
    let mut first = AddedAsset::new(&a, &a.project_versions[0]);
    first.files = vec!["Content/Megascans/Grass/A.uasset".into()];
    let mut store = ProjectAssets::load(dir);
    store.record(first.clone()).unwrap();
    let file = dir.join("Saved/uehub/assets.json");
    assert!(file.is_file());
    assert!(!dir.join("Saved/uehub/assets.json.tmp").exists());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"artifactId\": \"art53\""), "{text}");
    assert!(text.contains("\"buildVersion\""));
    assert!(text.contains("\"addedAt\""));

    let mut other = first.clone();
    other.asset_id = "rocks".into();
    store.record(other).unwrap();
    let mut again = first.clone();
    again.build_version = "1.1".into();
    store.record(again.clone()).unwrap();

    let back = ProjectAssets::load(dir);
    assert_eq!(back.list().len(), 2);
    let grass = back.list().iter().find(|x| x.asset_id == "grass").unwrap();
    assert_eq!(grass, &again);

    std::fs::write(&file, "{ not json").unwrap();
    assert!(
        ProjectAssets::load(dir).list().is_empty(),
        "un fichier abîmé se lit comme vide"
    );
}

#[test]
fn remove_keeps_shared_files_prunes_empty_dirs_and_drops_the_fiche() {
    let project = project();
    let d = project.path();
    let a = [
        "Content/A/Meshes/rock.uasset",
        "Content/A/Maps/demo.umap",
        "Content/Shared/T_Noise.uasset",
    ];
    let b = ["Content/B/tree.uasset", "Content/Shared/T_Noise.uasset"];
    write(d, &a);
    write(d, &b);
    // Un fichier que tu as ajouté toi-même à côté du pack.
    write(d, &["Content/A/mine.uasset"]);
    let mut assets = ProjectAssets::load(d);
    assert_eq!(assets.record(fiche("a", &a)).unwrap(), Removal::default());
    assert_eq!(assets.record(fiche("b", &b)).unwrap(), Removal::default());

    let removal = ProjectAssets::load(d).remove("a").unwrap();
    assert_eq!(
        removal,
        Removal {
            deleted: 2,
            shared: 1,
            missing: 0
        }
    );
    assert!(!d.join("Content/A/Meshes").exists() && !d.join("Content/A/Maps").exists());
    // Content/A garde ton fichier : il reste, et Content aussi.
    assert!(d.join("Content/A/mine.uasset").is_file());
    assert!(d.join("Content/Shared/T_Noise.uasset").is_file());
    let left = ProjectAssets::load(d);
    let ids: Vec<&str> = left.list().iter().map(|x| x.asset_id.as_str()).collect();
    assert_eq!(ids, ["b"]);

    // Retirer b efface maintenant la texture partagée, et vide Content/B et
    // Content/Shared ; Content reste.
    let removal = ProjectAssets::load(d).remove("b").unwrap();
    assert_eq!(
        removal,
        Removal {
            deleted: 2,
            shared: 0,
            missing: 0
        }
    );
    assert!(!d.join("Content/B").exists() && !d.join("Content/Shared").exists());
    assert!(d.join("Content").is_dir());
    assert!(ProjectAssets::load(d).list().is_empty());

    let err = ProjectAssets::load(d).remove("b").unwrap_err().to_string();
    assert!(err.contains("n'a pas été ajouté"), "{err}");
}

#[test]
fn plugin_folder_goes_but_plugins_stays() {
    let project = project();
    let d = project.path();
    let p = [
        "Plugins/Clouds/Clouds.uplugin",
        "Plugins/Clouds/Content/M_Cloud.uasset",
    ];
    write(d, &p);
    ProjectAssets::load(d).record(fiche("p", &p)).unwrap();
    let removal = ProjectAssets::load(d).remove("p").unwrap();
    assert_eq!(removal.deleted, 2);
    assert!(!d.join("Plugins/Clouds").exists());
    assert!(d.join("Plugins").is_dir());
}

#[test]
fn missing_files_and_paths_outside_the_project_are_never_deleted() {
    // Le projet est un sous-dossier : on peut placer un fichier juste à côté.
    let root = tempfile::tempdir().unwrap();
    let d = root.path().join("Project");
    std::fs::create_dir_all(d.join("Content")).unwrap();
    let outside = root.path().join("outside");
    std::fs::write(&outside, b"keep me").unwrap();
    let outside_abs = outside.to_string_lossy().into_owned();
    write(&d, &["Content/ok.uasset"]);
    let files = [
        "Content/ok.uasset",
        "Content/gone.uasset",
        outside_abs.as_str(),
        "../outside",
        "Content/../../outside",
        "",
        "Saved/../Content/ok2",
    ];
    ProjectAssets::load(&d).record(fiche("h", &files)).unwrap();
    let removal = ProjectAssets::load(&d).remove("h").unwrap();
    assert_eq!(
        removal,
        Removal {
            deleted: 1,
            shared: 0,
            missing: 6
        }
    );
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep me");
}

#[test]
fn a_file_that_cannot_be_deleted_stops_and_keeps_the_fiche() {
    let project = project();
    let d = project.path();
    write(
        d,
        &["Content/X/a.uasset", "Content/X/dir_not_file/inner.uasset"],
    );
    // remove_file sur un dossier échoue, même pour root : la fiche doit rester.
    let files = ["Content/X/a.uasset", "Content/X/dir_not_file"];
    ProjectAssets::load(d).record(fiche("x", &files)).unwrap();
    let err = ProjectAssets::load(d).remove("x").unwrap_err().to_string();
    assert!(
        err.contains("impossible d'effacer Content/X/dir_not_file"),
        "{err}"
    );
    assert_eq!(ProjectAssets::load(d).list().len(), 1);
    // Un nouvel essai : le premier fichier est déjà parti, le dossier refuse toujours.
    assert!(ProjectAssets::load(d).remove("x").is_err());
}

#[test]
fn an_update_deletes_what_the_new_version_no_longer_has() {
    let project = project();
    let d = project.path();
    let old = [
        "Content/G/a.uasset",
        "Content/G/Old/b.uasset",
        "Content/G/c.uasset",
        "Content/Shared/s.uasset",
    ];
    let other = ["Content/Other/o.uasset", "Content/Shared/s.uasset"];
    write(d, &old);
    write(d, &other);
    let mut assets = ProjectAssets::load(d);
    assets.record(fiche("g", &old)).unwrap();
    assets.record(fiche("o", &other)).unwrap();

    // La nouvelle version garde a et c, ajoute d ; b et la texture partagée n'y sont plus.
    let new = [
        "Content/G/a.uasset",
        "Content/G/c.uasset",
        "Content/G/New/d.uasset",
    ];
    write(d, &new);
    let mut updated = fiche("g", &new);
    updated.build_version = "1.1-Windows".into();
    let removal = ProjectAssets::load(d).record(updated).unwrap();
    assert_eq!(
        removal,
        Removal {
            deleted: 1,
            shared: 1,
            missing: 0
        }
    );
    assert!(!d.join("Content/G/Old").exists());
    assert!(
        d.join("Content/Shared/s.uasset").is_file(),
        "o s'en sert encore"
    );
    for f in new {
        assert!(d.join(f).is_file(), "{f}");
    }
    let fiches = ProjectAssets::load(d);
    let g = fiches.list().iter().find(|a| a.asset_id == "g").unwrap();
    assert_eq!(g.build_version, "1.1-Windows");
    assert_eq!(g.files.len(), 3);
    assert_eq!(fiches.list().len(), 2);
}
