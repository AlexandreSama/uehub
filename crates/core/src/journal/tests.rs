use super::*;

/// Des lignes comme celles que reqwest, egs-api ou uehub pourraient écrire, avec des
/// secrets semblables à ceux d'Epic. Tout ce qui contient « SECRET » doit disparaître.
const LINES: &[&str] = &[
    // Une adresse signée de téléchargement
    "GET https://cdn.unrealengine.com/dl/Linux_Unreal_Engine_5.8.3.zip?X-Amz-Expires=3600&X-Amz-Signature=SECRETsig failed",
    // Le SID dans une adresse, et le code dans une autre
    "error sending request for url (https://www.unrealengine.com/id/api/set-sid?sid=SECRETsid)",
    "redirect to http://127.0.0.1:4000/auth/callback?state=st&code=SECRETcode#frag",
    // Des cookies et un en-tête d'autorisation
    "Set-Cookie: EPIC_SSO=SECRETsso; Path=/; HttpOnly",
    "cookie jar: EPIC_BEARER_TOKEN=SECRETbearer; XSRF-TOKEN=SECRETxsrf",
    "Authorization: bearer SECRETtoken",
    "sending Bearer SECRETbare to the API",
    // Du JSON et du Debug de Rust
    r#"body: {"exchangeCode":"SECRETexchange","access_token": "SECRETaccess"}"#,
    r#"UserData { access_token: "SECRETdebug", refresh_token: "SECRETrefresh" }"#,
    // Un JWT d'Epic et un code d'échange nu (32 caractères hexadécimaux)
    "token eg1~eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJTRUNSRVQifQ.SECRETsigned",
    "code 0123456789abcdef0123456789ABCDEF refused",
];

#[test]
fn secrets_never_reach_the_file() {
    for line in LINES {
        let clean = scrub(line, None);
        assert!(!clean.contains("SECRET"), "{line}\n→ {clean}");
        assert!(!clean.contains("0123456789abcdef"), "{clean}");
        assert!(!clean.contains("eyJ"), "{clean}");
    }
}

#[test]
fn what_helps_to_understand_stays() {
    // Le site et le chemin d'une adresse restent : c'est ce qui dit quelle étape a échoué.
    let clean = scrub(LINES[0], None);
    assert!(
        clean.contains("https://cdn.unrealengine.com/dl/Linux_Unreal_Engine_5.8.3.zip?[hidden]"),
        "{clean}"
    );
    // Les codes d'erreur d'Epic aussi, et les phrases qui parlent de session.
    let epic = r#"Signing in on epicgames.com: Epic answered 409 Conflict. [{"errorCode":"errors.com.epicgames.accountportal.csrf_token_invalid"}]"#;
    assert_eq!(scrub(epic, None), epic);
    let prose = "Opening the unrealengine.com session: Epic answered 401 Unauthorized.";
    assert_eq!(scrub(prose, None), prose);
    let net = "Offline www.fab.com: error sending request: client error (Connect): tcp connect error: Connection refused (os error 111)";
    assert_eq!(scrub(net, None), net);
    // Le dossier personnel devient « ~ ».
    assert_eq!(
        scrub(
            "extracting into /home/alex/UnrealEngine/5.8.3",
            Some("/home/alex")
        ),
        "extracting into ~/UnrealEngine/5.8.3"
    );
}

#[test]
fn the_file_is_private_and_rotates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("logs/uehub.log");
    let journal = Journal::open(&path, Some("/home/alex".into())).unwrap();
    journal.write("first line\n").unwrap();
    let mode = fs::metadata(&path).unwrap().permissions();
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
        0o600,
        "lisible par toi seul"
    );

    // Au-delà d'1 Mo, le fichier devient uehub.log.1 et un nouveau commence.
    let big = "x".repeat(MAX_BYTES as usize);
    journal.write(&big).unwrap();
    journal.write("after the rotation\n").unwrap();
    let old = fs::read_to_string(dir.path().join("logs/uehub.log.1")).unwrap();
    assert!(old.starts_with("first line\n"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "after the rotation\n");
}

#[test]
fn records_are_filtered_and_scrubbed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("uehub.log");
    let journal = Journal::open(&path, None).unwrap();
    let record = |target: &'static str, level: Level, text: &'static str| {
        journal.log(
            &Record::builder()
                .target(target)
                .level(level)
                .args(format_args!("{text}"))
                .build(),
        );
    };
    record("uehub_desktop_lib::queue", Level::Info, "job 3 started");
    // Les bibliothèques ne parlent qu'à partir de warn.
    record("reqwest::connect", Level::Info, "starting new connection");
    record(
        "egs_api::api::egs",
        Level::Error,
        "GET https://x.test/a?code=SECRET",
    );
    let text = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].ends_with(" INFO  uehub_desktop_lib::queue: job 3 started"));
    assert!(lines[1].ends_with("ERROR egs_api::api::egs: GET https://x.test/a?[hidden]"));
}

#[test]
fn times_are_written_in_utc() {
    assert_eq!(utc(0), "1970-01-01 00:00:00Z");
    assert_eq!(utc(1_792_440_000), "2026-10-19 20:00:00Z");
    assert_eq!(utc(951_825_600), "2000-02-29 12:00:00Z");
}
