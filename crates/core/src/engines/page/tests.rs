use super::*;

/// Un zip tel que la vraie page l'écrivait le 18 octobre 2026.
fn blob(version: &str, size: u64) -> String {
    format!(
        r#"{{"name":"Linux_Unreal_Engine_{version}.zip","createdAt":"2026-09-22T12:45:18.215Z","size":{size},"version":"","semver":"","operatingSystem":"","downloadUrl":"https://ucs-blob-store.s3-accelerate.amazonaws.com/blobs/Linux_Unreal_Engine_{version}.zip?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Expires=3600&X-Amz-Signature=sig{version}"}}"#
    )
}

fn table(blobs: &[String]) -> String {
    format!(
        r#"{{"refSections":[{{"__typename":"CosmosReferenceSection","section":{{"__typename":"CosmosSectionBlobTable","config":null,"blobs":[{}]}}}}]}}"#,
        blobs.join(",")
    )
}

/// Échappe comme du JSON rangé dans une chaîne JavaScript : " devient \", \ devient \\,
/// et & devient &.
fn escape(json: &str) -> String {
    json.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('&', "\\u0026")
}

#[test]
fn raw_escaped_and_double_escaped_pages_give_the_same_list() {
    let json = table(&[
        blob("5.8.3", 39_818_576_005),
        blob("5.10.0", 5),
        blob("5.9.4", 7),
    ]);
    let pages = [
        format!("<script>{json}</script>"),
        format!(
            r#"<script>self.__next_f.push([1,"{}"])</script>"#,
            escape(&json)
        ),
        format!(r#"<script>x("{}")</script>"#, escape(&escape(&json))),
        format!(
            "<div data-x=\"{}\"></div>",
            json.replace('&', "&amp;").replace('"', "&quot;")
        )
        .replace("&quot;", "\""),
    ];
    for page in &pages {
        let found = parse_linux_page(page);
        let versions: Vec<&str> = found.iter().map(|d| d.version.as_str()).collect();
        assert_eq!(versions, ["5.10.0", "5.9.4", "5.8.3"], "{page}");
        let d = &found[2];
        assert_eq!(d.file_name, "Linux_Unreal_Engine_5.8.3.zip");
        assert_eq!(d.size, 39_818_576_005);
        assert_eq!(d.created_at.as_deref(), Some("2026-09-22T12:45:18.215Z"));
        assert!(
            d.url
                .contains("&X-Amz-Expires=3600&X-Amz-Signature=sig5.8.3"),
            "{}",
            d.url
        );
    }
}

#[test]
fn duplicates_other_files_and_broken_json_are_skipped() {
    let other = r#"{"name":"Linux_Fab_Plugin_5.8.zip","size":1,"downloadUrl":"https://s3/x"}"#;
    let preview = r#"{"name":"Linux_Unreal_Engine_5.9.0_preview-1.zip","size":1,"downloadUrl":"https://s3/y"}"#;
    let no_url = r#"{"name":"Linux_Unreal_Engine_5.7.4.zip","size":1}"#;
    let broken = r#"{"name":"Linux_Unreal_Engine_5.6.1.zip","size":"#;
    let page = format!(
        "{}{}{}{}{}{}",
        table(&[blob("5.8.3", 1), blob("5.8.3", 1)]),
        other,
        preview,
        no_url,
        blob("5.8.2", 2),
        broken
    );
    let found = parse_linux_page(&page);
    let names: Vec<&str> = found.iter().map(|d| d.file_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Linux_Unreal_Engine_5.8.3.zip",
            "Linux_Unreal_Engine_5.8.2.zip"
        ]
    );
    assert!(parse_linux_page("").is_empty());
    assert!(parse_linux_page("<p>Linux_Unreal_Engine_5.8.3.zip</p>").is_empty());
}

#[test]
fn name_not_first_in_the_object() {
    let page = r#"{"size":3,"downloadUrl":"https://s3/z?X-Amz-Signature=1","nested":{"a":1},"name":"Linux_Unreal_Engine_5.5.4.zip"}"#;
    let found = parse_linux_page(page);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].size, 3);
}

#[test]
fn decode_keeps_json_valid() {
    assert_eq!(decode_pass(r#"a\"b\\c\/d&e\n"\"#), r#"a"b\c/d&e\n"\"#);
    assert_eq!(unescape(r#"\\\"x\\\" \\u0026 &amp;"#), r#""x" & &"#);
}

#[test]
fn json_with_spaces_after_colons() {
    let page =
        r#"{ "name": "Linux_Unreal_Engine_5.8.3.zip", "size": 9, "downloadUrl": "https://s3/a" }"#;
    let found = parse_linux_page(page);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "5.8.3");
}
