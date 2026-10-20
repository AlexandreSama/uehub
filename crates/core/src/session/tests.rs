use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use keyring_core::api::CredentialStoreApi;
use keyring_core::mock;

fn sample() -> UserData {
    serde_json::from_str(r#"{"displayName":"Djinn","account_id":"abc123"}"#).unwrap()
}

/// Un trousseau en mémoire, à la place de GNOME Keyring ou KWallet : le vrai garde ta
/// session, un test ne doit pas y toucher.
fn mock_keyring() -> Entry {
    mock::Store::new()
        .unwrap()
        .build(SERVICE, ACCOUNT, None)
        .unwrap()
}

/// Une session rangée dans ce dossier, avec ou sans trousseau.
fn store(dir: &Path, keyring: Option<Entry>) -> SessionStore {
    SessionStore {
        file: dir.join("session.json"),
        keyring,
    }
}

fn round_trip(store: &SessionStore) {
    assert!(store.load().unwrap().is_none());
    store.save(&sample()).unwrap();
    let back = store.load().unwrap().unwrap();
    assert_eq!(back.display_name.as_deref(), Some("Djinn"));
    store.clear().unwrap();
    assert!(store.load().unwrap().is_none());
    // Effacer deux fois ne gêne pas.
    store.clear().unwrap();
}

#[test]
fn keyring_round_trip_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), Some(mock_keyring()));
    assert_eq!(store.backend(), "trousseau système");
    store.save(&sample()).unwrap();
    assert!(
        !store.file.exists(),
        "avec un trousseau, rien sur le disque"
    );
    store.clear().unwrap();
    round_trip(&store);
}

#[test]
fn without_keyring_the_file_is_private() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(&dir.path().join("deep"), None);
    assert_eq!(store.backend(), "fichier (aucun trousseau disponible)");
    store.save(&sample()).unwrap();
    let mode = fs::metadata(&store.file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "lisible par toi seul");
    store.clear().unwrap();
    assert!(!store.file.exists());
    round_trip(&store);
}

#[test]
fn an_old_session_file_moves_into_the_keyring() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("session.json");
    write_private_file(&file, &serde_json::to_string(&sample()).unwrap()).unwrap();
    let store = store(dir.path(), Some(mock_keyring()));
    store.migrate_file_to_keyring().unwrap();
    assert!(!file.exists(), "le fichier a été déplacé");
    let back = store.load().unwrap().unwrap();
    assert_eq!(back.display_name.as_deref(), Some("Djinn"));
}

#[test]
fn a_damaged_file_is_an_error_not_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), None);
    fs::write(&store.file, "{ not json").unwrap();
    assert!(store.load().is_err());
}
