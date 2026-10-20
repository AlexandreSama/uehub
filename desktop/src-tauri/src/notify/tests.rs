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
    assert_eq!(text(1_234_000_000), "61 files, 1.2 GB");
    assert_eq!(text(1_000_000_000), "61 files, 1 GB");
    assert_eq!(text(1_572_864), "61 files, 1.6 MB");
    assert_eq!(text(999), "61 files, 1 kB");
    assert_eq!(text(0), "61 files, 0 kB");
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
            "Added: Megascans: Grass".into(),
            "61 files, 1.2 GB in ForestShooter".into()
        ))
    );

    let noted = job(
        add(),
        "ForestShooter",
        Status::Done {
            summary: summary(1_234_000_000, Some("No version for UE 5.5.")),
        },
    );
    assert_eq!(
        message(&noted).unwrap().1,
        "61 files, 1.2 GB in ForestShooter\nNo version for UE 5.5."
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
            "Downloaded: Megascans: Grass".into(),
            "61 files, 2 MB in /home/a/Assets/Megascans_Grass".into()
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
            "Installed: Unreal Engine 5.8.3".into(),
            "61 files, 39.8 GB in /home/a/UnrealEngine/5.8.3".into()
        ))
    );

    let failed = job(
        add(),
        "ForestShooter",
        Status::Failed {
            error: "connection cut".into(),
        },
    );
    assert_eq!(
        message(&failed),
        Some(("Failed: Megascans: Grass".into(), "connection cut".into()))
    );

    for status in [Status::Waiting, Status::Running, Status::Paused] {
        assert!(message(&job(add(), "ForestShooter", status)).is_none());
    }
}
