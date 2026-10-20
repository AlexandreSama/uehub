use super::*;
use serde_json::json;

#[test]
fn progress_matches_typescript() {
    // UpdateProgress dans api.ts : on avance en octets, puis on installe.
    let download = UpdateProgress::Download {
        done: 5,
        total: None,
    };
    assert_eq!(
        serde_json::to_value(download).unwrap(),
        json!({"step": "download", "done": 5, "total": null})
    );
    assert_eq!(
        serde_json::to_value(UpdateProgress::Install).unwrap(),
        json!({"step": "install"})
    );
}

#[test]
fn update_info_matches_typescript() {
    let info = UpdateInfo {
        version: "0.2.0".into(),
        date: Some("2026-10-19".into()),
        notes: None,
    };
    assert_eq!(
        serde_json::to_value(info).unwrap(),
        json!({"version": "0.2.0", "date": "2026-10-19", "notes": null})
    );
}
