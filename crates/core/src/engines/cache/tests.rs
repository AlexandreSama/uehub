use super::*;

fn download(version: &str, size: u64) -> EngineDownload {
    EngineDownload {
        version: version.into(),
        file_name: format!("Linux_Unreal_Engine_{version}.zip"),
        size,
        created_at: Some("2026-09-22T12:45:18.215Z".into()),
        url: format!("https://cdn/{version}.zip?X-Amz-Signature=SECRET"),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn listed_engine_keeps_what_the_screen_shows_and_drops_the_address() {
    let listed = ListedEngine::from(&download("5.8.3", 30_000_000_000));
    assert_eq!(
        listed,
        ListedEngine {
            version: "5.8.3".into(),
            size: 30_000_000_000,
            created_at: Some("2026-09-22T12:45:18.215Z".into())
        }
    );
    let list = EngineList::checked_now(vec![listed]);
    assert!(list.checked_at.abs_diff(now()) <= 5, "{}", list.checked_at);
    let json = String::from_utf8(serde_json::to_vec_pretty(&list).unwrap()).unwrap();
    assert!(json.contains("\"checkedAt\""), "{json}");
    assert!(json.contains("\"createdAt\""), "{json}");
    assert!(
        !json.contains("SECRET") && !json.contains("url") && !json.contains("https"),
        "{json}"
    );
}

#[test]
fn round_trip_keeps_order_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("deep/er/engines.json");
    let list = EngineList {
        checked_at: 1_792_400_000,
        engines: vec![
            ListedEngine::from(&download("5.8.3", 3)),
            ListedEngine::from(&download("5.10.0", 10)),
            ListedEngine {
                version: "5.7.4".into(),
                size: 7,
                created_at: None,
            },
        ],
    };
    list.save_to(&file).unwrap();
    assert!(!file.with_extension("json.tmp").exists());
    assert_eq!(EngineList::load_from(&file), Some(list.clone()));
    // Une deuxième liste remplace la première.
    let shorter = EngineList {
        checked_at: 1_792_400_100,
        engines: list.engines[..1].to_vec(),
    };
    shorter.save_to(&file).unwrap();
    assert_eq!(EngineList::load_from(&file), Some(shorter));
}

#[test]
fn missing_or_damaged_file_gives_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(EngineList::load_from(&dir.path().join("absent.json")), None);
    let file = dir.path().join("engines.json");
    std::fs::write(&file, b"{\"checkedAt\": 12, \"engines\": [").unwrap();
    assert_eq!(EngineList::load_from(&file), None);
    std::fs::write(&file, b"[]").unwrap();
    assert_eq!(EngineList::load_from(&file), None);
}

#[test]
fn an_unwritable_place_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-dir");
    std::fs::write(&blocker, b"x").unwrap();
    let list = EngineList::checked_now(Vec::new());
    assert!(list.save_to(&blocker.join("engines.json")).is_err());
}

#[test]
fn the_cache_lives_in_the_user_cache_dir() {
    // ~/.cache/uehub/engines.json, ou ce que dit XDG_CACHE_HOME. On ne fait que lire le
    // chemin : les tests n'écrivent jamais dans ton vrai cache.
    assert!(file().unwrap().ends_with("uehub/engines.json"));
}
