use super::*;
use crate::download::pick_for_project;
use egs_api::api::types::download_manifest::{DownloadManifest, FileManifestList};
use egs_api::api::types::fab_library::ProjectVersion;

fn uproject(dir: &Path, name: &str, association: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("{name}.uproject"));
    fs::write(
        &path,
        format!(r#"{{"FileVersion":3,"EngineAssociation":"{association}","Modules":[]}}"#),
    )
    .unwrap();
    path
}

fn build_version(root: &Path, major: u32, minor: u32) {
    let dir = root.join("Engine/Build");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("Build.version"),
        format!(r#"{{"MajorVersion":{major},"MinorVersion":{minor},"PatchVersion":2}}"#),
    )
    .unwrap();
}

#[test]
fn engine_detection() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path();
    let engine = tmp.join("engine");
    build_version(&engine, 5, 6);
    // Un Install.ini à nous : la même clé hors de [Installations] ne compte pas.
    let ini = tmp.join("Install.ini");
    fs::write(
        &ini,
        format!(
            "[Other]\n{{0A1B}}=/wrong\n[Installations]\n{{0A1B}}={}\n",
            engine.display()
        ),
    )
    .unwrap();
    let open = |path: &Path| UnrealProject::open_with(path, Some(&ini));

    // Par numéro de version.
    let p = open(&uproject(&tmp.join("MyGame"), "MyGame", "5.4")).unwrap();
    assert_eq!(p.engine, Some((5, 4)));
    assert_eq!(p.name, "MyGame");
    assert!(p.uproject.ends_with("MyGame/MyGame.uproject"));
    assert!(p.dir.ends_with("MyGame"));
    // Par identifiant, quelle que soit la casse ; un identifiant inconnu ne donne rien.
    assert_eq!(
        open(&uproject(tmp, "G", "{0a1b}")).unwrap().engine,
        Some((5, 6))
    );
    assert_eq!(open(&uproject(tmp, "X", "{ffff}")).unwrap().engine, None);
    assert_eq!(
        UnrealProject::open_with(&uproject(tmp, "Y", "{0a1b}"), None)
            .unwrap()
            .engine,
        None
    );
    // Sans association : le projet est rangé dans l'arborescence du moteur.
    let tree = tmp.join("tree");
    build_version(&tree, 5, 5);
    let inside = open(&uproject(&tree.join("Games/S"), "S", "")).unwrap();
    assert_eq!(inside.engine, Some((5, 5)));
    assert!(open(Path::new("/nope/Nope.uproject")).is_err());
}

fn plan_with(files: &[&str]) -> DownloadPlan {
    let manifest = DownloadManifest {
        file_manifest_list: files
            .iter()
            .map(|f| FileManifestList {
                filename: f.to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    DownloadPlan::from_manifest(&manifest)
}

#[test]
fn plugin_layouts_and_conflicts() {
    let proj = Path::new("/p");
    let target = |files: &[&str]| plugin_target(proj, &plan_with(files));
    assert_eq!(
        target(&["Foo.uplugin", "Source/Foo/Foo.Build.cs"]).unwrap(),
        Path::new("/p/Plugins/Foo")
    );
    assert_eq!(
        target(&["Foo/Foo.uplugin"]).unwrap(),
        Path::new("/p/Plugins")
    );
    assert_eq!(
        target(&["Plugins/Foo/Foo.uplugin"]).unwrap(),
        Path::new("/p")
    );
    assert!(target(&["Content/a.uasset"]).is_err());

    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    fs::create_dir_all(dir.join("Content/Pack")).unwrap();
    fs::write(dir.join("Content/Pack/A.uasset"), b"x").unwrap();
    let plan = plan_with(&["Content/Pack/A.uasset", "Content/Pack/B.uasset"]);
    assert_eq!(plan.existing_files(dir), vec!["Content/Pack/A.uasset"]);
}

fn version(id: &str, engines: &[&str]) -> ProjectVersion {
    ProjectVersion {
        artifact_id: id.into(),
        engine_versions: engines.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn kinds_and_compat() {
    let mut a = FabAsset {
        distribution_method: "ASSET_PACK".into(),
        ..Default::default()
    };
    assert_eq!(AssetKind::of(&a), AssetKind::AssetPack);
    a.distribution_method = "CODE_PLUGIN".into();
    assert_eq!(AssetKind::of(&a), AssetKind::Plugin);
    a.distribution_method = "COMPLETE_PROJECT".into();
    assert_eq!(AssetKind::of(&a), AssetKind::CompleteProject);
    a.project_versions = vec![
        version("V1", &["UE_4.27", "UE_5.0"]),
        version("V2", &["UE_5.1", "UE_5.4"]),
    ];
    let pick =
        |engine| pick_for_project(&a, engine).map(|(v, exact)| (v.artifact_id.clone(), exact));
    assert_eq!(pick((5, 4)), Some(("V2".into(), true)));
    assert_eq!(pick((5, 6)), Some(("V2".into(), false)));
    assert_eq!(pick((5, 10)), Some(("V2".into(), false)));
    assert_eq!(pick((4, 26)), None);
    assert_eq!(parse_version("5.4.2"), Some((5, 4)));
}
