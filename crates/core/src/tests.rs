use super::*;
use egs_api::api::types::download_manifest::FileManifestList;
use egs_api::api::types::fab_library::ProjectVersion;

fn plan(names: &[&str]) -> DownloadPlan {
    let manifest = DownloadManifest {
        file_manifest_list: names
            .iter()
            .map(|n| FileManifestList {
                filename: n.to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    DownloadPlan::from_manifest(&manifest)
}

#[test]
fn record_lists_files_from_the_project_root() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let asset = FabAsset {
        asset_id: "clouds".into(),
        title: "Clouds".into(),
        ..Default::default()
    };
    let version = ProjectVersion {
        artifact_id: "art".into(),
        engine_versions: vec!["UE_5.4".into()],
        ..Default::default()
    };
    let record = Record {
        project_dir: dir.to_path_buf(),
        added: AddedAsset::new(&asset, &version),
    };

    // Un plugin s'écrit dans Plugins/Clouds : ses fichiers partent de la racine du projet.
    record.save(
        &plan(&["Clouds.uplugin", "Content/Sky.uasset"]),
        &dir.join("Plugins/Clouds"),
    );
    let saved = ProjectAssets::load(dir);
    assert_eq!(
        saved.list()[0].files,
        [
            "Plugins/Clouds/Clouds.uplugin",
            "Plugins/Clouds/Content/Sky.uasset"
        ]
    );
    assert_eq!(saved.list()[0].engine, "5.4");

    // Un pack s'écrit à la racine du projet.
    record.save(&plan(&["Content/A.uasset"]), dir);
    assert_eq!(
        ProjectAssets::load(dir).list()[0].files,
        ["Content/A.uasset"]
    );

    // Un projet où l'on ne peut pas écrire : un simple message, l'ajout a déjà réussi.
    std::fs::write(dir.join("file"), "x").unwrap();
    let blocked = Record {
        project_dir: dir.join("file"),
        added: record.added.clone(),
    };
    assert_eq!(
        blocked.save(&plan(&["Content/A.uasset"]), &dir.join("file")),
        0
    );
}
