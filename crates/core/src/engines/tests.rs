use super::*;

#[test]
fn versions_come_from_engine_zip_names_only() {
    assert_eq!(
        engine_version("Linux_Unreal_Engine_5.8.3.zip").as_deref(),
        Some("5.8.3")
    );
    assert_eq!(
        engine_version("Linux_Unreal_Engine_5.10.0.zip").as_deref(),
        Some("5.10.0")
    );
    for name in [
        "Linux_Unreal_Engine_.zip",
        "Linux_Unreal_Engine_5..3.zip",
        "Linux_Unreal_Engine_5.8.3-rc1.zip",
        "Linux_Unreal_Engine_5.8.3.zip.sig",
        "Linux_Bridge_5.8.3.zip",
    ] {
        assert_eq!(engine_version(name), None, "{name}");
    }
}

#[test]
fn version_keys_compare_as_numbers() {
    assert_eq!(version_key("5.8.3"), [5, 8, 3]);
    assert!(version_key("5.10.0") > version_key("5.9.4"));
    assert!(version_key("5.8.10") > version_key("5.8.9"));
}
