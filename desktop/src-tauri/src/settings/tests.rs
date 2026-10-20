use super::*;

#[test]
fn settings_json_is_tolerant() {
    let empty: Settings = serde_json::from_str("{}").unwrap();
    assert!(empty.engines_folder.is_none());
    let s: Settings =
        serde_json::from_str(r#"{"enginesFolder":"/home/a/Jeux","later":1}"#).unwrap();
    assert_eq!(
        s.engines_folder.as_deref(),
        Some(std::path::Path::new("/home/a/Jeux"))
    );
    assert_eq!(
        serde_json::to_string(&s).unwrap(),
        r#"{"enginesFolder":"/home/a/Jeux"}"#
    );
    assert!(serde_json::from_slice::<Settings>(b"not json").is_err());
}
