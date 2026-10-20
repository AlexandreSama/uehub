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

#[test]
fn each_updater_failure_gets_its_sentence() {
    use tauri_plugin_updater::Error as E;
    let signature = Error::from(E::MissingSignedVersion);
    assert_eq!(
        signature.to_string(),
        "Signature rejected: the downloaded update may not come from uehub, so it wasn't installed."
    );
    // L'anglais du plugin reste pour le journal.
    assert!(signature.detail().contains("requireSignedVersion"));
    assert_eq!(
        Error::from(E::AuthenticationFailed).to_string(),
        "Installation cancelled: the password wasn't given."
    );
    assert_eq!(
        Error::from(E::ReleaseNotFound).to_string(),
        "The update server returned no release for this system."
    );
    assert_eq!(
        Error::from(E::TempDirNotFound).to_string(),
        "The update failed. The details are in the log."
    );
}

#[tokio::test]
async fn the_network_reads_like_everywhere_else() {
    // Une adresse où rien n'écoute : comme latest.json sans réseau.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/latest.json", listener.local_addr().unwrap());
    drop(listener);
    let e = reqwest::get(&url).await.unwrap_err();
    let e = Error::from(tauri_plugin_updater::Error::Reqwest(e));
    assert_eq!(
        e.to_string(),
        "No connection: can't reach 127.0.0.1. Check your Internet access."
    );
    assert!(e.detail().contains("Connection refused"), "{}", e.detail());
    // L'interface reçoit la phrase.
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!("No connection: can't reach 127.0.0.1. Check your Internet access.")
    );
}
