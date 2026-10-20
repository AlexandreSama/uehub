use super::*;
use serde_json::json;
use uehub_core::ListedEngine;

#[test]
fn added_view_matches_typescript() {
    let view = AddedView {
        project: "/p/ForestShooter/ForestShooter.uproject".into(),
        asset_id: "grass".into(),
        engine: "5.3".into(),
        added_at: 1_792_300_000,
        files: 61,
        update: Some(AssetUpdate::CloserEngine("5.4".into()).into()),
    };
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        json!({"project": "/p/ForestShooter/ForestShooter.uproject", "assetId": "grass", "engine": "5.3",
               "addedAt": 1_792_300_000u64, "files": 61, "update": {"kind": "closerEngine", "engine": "5.4"}})
    );
    assert_eq!(
        serde_json::to_value(UpdateView::from(AssetUpdate::NewBuild)).unwrap(),
        json!({"kind": "newBuild"})
    );
    let none = AddedView {
        update: None,
        ..view
    };
    assert_eq!(serde_json::to_value(&none).unwrap()["update"], json!(null));
}

#[test]
fn catalog_matches_typescript() {
    let list = EngineList {
        checked_at: 1_792_400_000,
        engines: vec![
            ListedEngine {
                version: "5.8.3".into(),
                size: 30_000_000_000,
                created_at: Some("2026-09-22T12:45:18.215Z".into()),
            },
            ListedEngine {
                version: "5.7.4".into(),
                size: 2,
                created_at: None,
            },
        ],
    };
    let value = serde_json::to_value(catalog(list, true)).unwrap();
    assert_eq!(
        value,
        json!({"eulaAccepted": true, "checkedAt": 1_792_400_000u64, "engines": [
            {"version": "5.8.3", "size": 30_000_000_000u64, "extracted": extracted_size(30_000_000_000), "date": "2026-09-22"},
            {"version": "5.7.4", "size": 2, "extracted": extracted_size(2), "date": null}
        ]})
    );
    let none = serde_json::to_value(catalog(EngineList::checked_now(Vec::new()), false)).unwrap();
    assert_eq!(none["eulaAccepted"], json!(false));
    assert_eq!(none["engines"], json!([]));
    assert!(none["checkedAt"].as_u64().unwrap() > 1_790_000_000);
}

fn done(removed: usize) -> DownloadSummary {
    DownloadSummary {
        files: 3,
        bytes: 10,
        dir: PathBuf::from("/p"),
        removed,
    }
}

#[test]
fn summary_note_tells_what_the_update_removed() {
    assert_eq!(summary(done(0), None).note, None);
    assert_eq!(
        summary(done(0), Some("conv.".into())).note.as_deref(),
        Some("conv.")
    );
    assert_eq!(
        summary(done(1), None).note.as_deref(),
        Some("1 fichier de l'ancienne version effacé.")
    );
    assert_eq!(
        summary(done(4), Some("Pas de version 5.4.".into()))
            .note
            .as_deref(),
        Some("Pas de version 5.4. 4 fichiers de l'ancienne version effacés.")
    );
}

#[test]
fn removal_view_and_same_file() {
    let v = RemovalView::from(Removal {
        deleted: 5,
        shared: 2,
        missing: 1,
    });
    assert_eq!(
        serde_json::to_value(v).unwrap(),
        json!({"deleted": 5, "shared": 2, "missing": 1})
    );
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    std::fs::create_dir_all(d.join("P")).unwrap();
    std::fs::write(d.join("P/P.uproject"), "{}").unwrap();
    assert!(same_file(
        &d.join("P/P.uproject"),
        &d.join("P/../P/P.uproject")
    ));
    assert!(!same_file(&d.join("P/P.uproject"), &d.join("Q/Q.uproject")));
}

fn engine(version: Option<&str>, dir: &str, ours: bool) -> InstalledEngine {
    InstalledEngine {
        id: dir.into(),
        dir: PathBuf::from(dir),
        version: version.map(String::from),
        by_uehub: ours,
    }
}

#[test]
fn local_view_says_updates_and_replacements() {
    let installed = [
        engine(Some("5.8.3"), "/e/5.8.3", true),
        engine(Some("5.7.3"), "/e/5.7.3", true),
        engine(Some("5.7.4"), "/e/5.7.4", true),
        engine(Some("5.5.1"), "/src/UnrealEngine", false),
        engine(None, "/e/broken", true),
    ];
    let list = EngineList {
        checked_at: 1,
        engines: ["5.9.0", "5.8.4", "5.8.3", "5.7.4", "5.5.4"]
            .iter()
            .map(|v| ListedEngine {
                version: v.to_string(),
                size: 1,
                created_at: None,
            })
            .collect(),
    };
    let view = serde_json::to_value(local_view(&installed, Some(&list))).unwrap();
    assert_eq!(
        view[0],
        json!({"version": "5.8.3", "dir": "/e/5.8.3", "byUehub": true, "update": "5.8.4", "replacedBy": null})
    );
    assert_eq!(view[1]["update"], json!(null));
    assert_eq!(view[1]["replacedBy"], json!("5.7.4"));
    assert_eq!(view[2]["update"], json!(null));
    assert_eq!(view[2]["replacedBy"], json!(null));
    assert_eq!(view[3]["update"], json!("5.5.4"));
    assert_eq!(
        view[4],
        json!({"version": null, "dir": "/e/broken", "byUehub": true, "update": null, "replacedBy": null})
    );
    // Pas encore de liste : les remplaçants sont connus, les mises à jour non.
    let view = serde_json::to_value(local_view(&installed, None)).unwrap();
    assert_eq!(view[0]["update"], json!(null));
    assert_eq!(view[1]["replacedBy"], json!("5.7.4"));
}
