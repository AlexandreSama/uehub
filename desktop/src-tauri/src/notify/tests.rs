use super::*;
use crate::commands::Summary;
use serde_json::json;

fn summary(bytes: u64, note: Option<&str>) -> Summary {
    serde_json::from_value(json!({"files": 61, "bytes": bytes, "dir": "/p", "note": note})).unwrap()
}

fn job(request: Request, target: &str, status: Status) -> Job {
    Job {
        id: 1,
        request,
        title: "Megascans: Grass".into(),
        thumbnail: None,
        target: target.into(),
        status,
        artifact_id: None,
        finished_at: None,
        progress: None,
    }
}

fn add() -> Request {
    Request::AddToProject {
        asset_id: "a".into(),
        uproject: "/p/F.uproject".into(),
        overwrite: false,
    }
}

#[test]
fn sizes_read_like_the_interface() {
    let text = |b| summary(b, None).text();
    assert_eq!(text(1_234_000_000), "61 fichiers, 1,2 Go");
    assert_eq!(text(1_000_000_000), "61 fichiers, 1 Go");
    assert_eq!(text(1_572_864), "61 fichiers, 1,6 Mo");
    assert_eq!(text(999), "61 fichiers, 1 Ko");
    assert_eq!(text(0), "61 fichiers, 0 Ko");
    assert_eq!(summary(5, Some("n")).note(), Some("n"));
    assert_eq!(summary(5, None).note(), None);
}

#[test]
fn messages_for_finished_jobs_only() {
    let done = job(
        add(),
        "ForestShooter",
        Status::Done {
            summary: summary(1_234_000_000, None),
        },
    );
    assert_eq!(
        message(&done),
        Some((
            "Ajout terminé : Megascans: Grass".into(),
            "61 fichiers, 1,2 Go dans ForestShooter".into()
        ))
    );

    let noted = job(
        add(),
        "ForestShooter",
        Status::Done {
            summary: summary(1_234_000_000, Some("Pas de version pour UE 5.5.")),
        },
    );
    assert_eq!(
        message(&noted).unwrap().1,
        "61 fichiers, 1,2 Go dans ForestShooter\nPas de version pour UE 5.5."
    );

    let dl = Request::Download {
        asset_id: "a".into(),
        folder: "/home/a/Assets".into(),
    };
    let downloaded = job(
        dl,
        "/home/a/Assets/Megascans_Grass",
        Status::Done {
            summary: summary(2_000_000, None),
        },
    );
    assert_eq!(
        message(&downloaded),
        Some((
            "Téléchargement terminé : Megascans: Grass".into(),
            "61 fichiers, 2 Mo dans /home/a/Assets/Megascans_Grass".into()
        ))
    );

    let ue = Request::InstallEngine {
        version: "5.8.3".into(),
        folder: "/home/a/UnrealEngine".into(),
    };
    let mut installed = job(
        ue,
        "/home/a/UnrealEngine/5.8.3",
        Status::Done {
            summary: summary(39_818_576_005, None),
        },
    );
    installed.title = "Unreal Engine 5.8.3".into();
    assert_eq!(
        message(&installed),
        Some((
            "Installation terminée : Unreal Engine 5.8.3".into(),
            "61 fichiers, 39,8 Go dans /home/a/UnrealEngine/5.8.3".into()
        ))
    );

    let failed = job(
        add(),
        "ForestShooter",
        Status::Failed {
            error: "réseau coupé".into(),
        },
    );
    assert_eq!(
        message(&failed),
        Some(("Échec : Megascans: Grass".into(), "réseau coupé".into()))
    );

    for status in [Status::Waiting, Status::Running, Status::Paused] {
        assert!(message(&job(add(), "ForestShooter", status)).is_none());
    }
}
