use super::*;
use crate::test_support::{Reply, Request, dead_address, serve};

/// Comment répond le faux site.
#[derive(Clone, Copy)]
struct Fake {
    /// La connexion silencieuse aboutit au cookie du site
    silent_works: bool,
    /// La page Linux contient des zips
    page_has_zips: bool,
}

/// Un faux epicgames.com + unrealengine.com, qui répond comme les vrais le 18 octobre 2026.
/// Les valeurs secrètes contiennent « SECRET » : aucune ne doit sortir dans une erreur.
async fn fake_epic(fake: Fake) -> String {
    serve(move |request, base| answer(&request, base, fake)).await
}

fn answer(request: &Request, base: &str, fake: Fake) -> Reply {
    let auth = request.has_cookie("EPIC_AUTH_b8e6");
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/id/api/reputation") => Reply::new("404 Not Found")
            .header("Set-Cookie", "XSRF-TOKEN=xsrfSECRET; Path=/id"),
        ("POST", "/id/api/exchange") => {
            let xsrf = request.header("x-xsrf-token") == Some("xsrfSECRET");
            if xsrf && request.body.contains("CODESECRET") {
                Reply::new("200 OK")
            } else {
                Reply::new("409 Conflict").body(
                    r#"{"errorCode":"errors.com.epicgames.accountportal.csrf_token_invalid"}"#,
                )
            }
        }
        ("GET", "/id/api/redirect") => Reply::new("200 OK")
            .header("Set-Cookie", "EPIC_SSO=ssoSECRET; Path=/; HttpOnly")
            .body(r#"{"redirectUrl":"https://www.epicgames.com/account/personal","authorizationCode":null,"exchangeCode":null,"sid":"SIDSECRET"}"#),
        ("GET", "/id/api/set-sid") => {
            assert_eq!(request.query["sid"], "SIDSECRET");
            Reply::new("204 No Content").header(
                "Set-Cookie",
                "EPIC_BEARER_TOKEN=staleSECRET; Path=/; HttpOnly",
            )
        }
        ("GET", "/api/cosmos/auth") if auth => Reply::new("200 OK").body(
            r#"{"account":{"displayName":"Djinn","id":"acc1","isLoggedIn":true},"accountId":"acc1","bearerTokenValid":false}"#,
        ),
        ("GET", "/api/cosmos/auth") => Reply::new("200 OK").body(
            r#"{"account":{"isLoggedIn":false},"accountId":"","bearerTokenValid":false,"silentLogin":{"fingerprint":"fp","url":"/auth/login?clientId=b8e66790fe244b89a47f4e2c7ed77047&prompt=none"}}"#,
        ),
        ("GET", "/auth/login") => Reply::new("302 Found").header(
            "Location",
            &format!(
                "{base}/id/api/login?client_id={}&redirect_uri={base}/auth/callback&response_type=code&state=st&prompt=none",
                request.query["clientId"]
            ),
        ),
        ("GET", "/id/api/login") if fake.silent_works && request.has_cookie("EPIC_SSO") => {
            let back = format!("{}?state=st&code=codeSECRET", request.query["redirect_uri"]);
            Reply::new("302 Found").header("Location", &back)
        }
        ("GET", "/id/api/login") => Reply::new("302 Found").header("Location", "/id/login"),
        ("GET", "/id/login") => {
            Reply::new("200 OK").body("<html><title>Connexion</title></html>")
        }
        ("GET", "/auth/callback") => Reply::new("302 Found")
            .header("Location", "/")
            .header(
                "Set-Cookie",
                "EPIC_AUTH_b8e6=authSECRET; Path=/; HttpOnly; SameSite=Lax",
            ),
        ("GET", "/") => Reply::new("200 OK").body("<html><title>Unreal Engine</title></html>"),
        ("GET", "/linux") => Reply::new("200 OK").body(linux_page(base, auth && fake.page_has_zips)),
        ("GET", "/api/cosmos/eula/accept") if auth => {
            Reply::new("200 OK").body(r#"{"accepted":true}"#)
        }
        ("GET", "/api/cosmos/eula/accept") => Reply::new("401 Unauthorized")
            .body(r#"{"error":"Not logged in","isLoggedIn":false}"#),
        _ => Reply::new("404 Not Found"),
    }
}

/// La page Linux : le JSON des zips, échappé dans un script comme sur la vraie page.
fn linux_page(base: &str, with_zips: bool) -> String {
    let json = if with_zips {
        format!(
            r#"{{"blobs":[{{"name":"Linux_Unreal_Engine_5.8.2.zip","createdAt":"2026-08-25T00:00:00Z","size":2,"downloadUrl":"{base}/dl/Linux_Unreal_Engine_5.8.2.zip?X-Amz-Expires=3600&X-Amz-Signature=s"}},{{"name":"Linux_Unreal_Engine_5.8.3.zip","createdAt":"2026-09-22T12:45:18.215Z","size":3,"downloadUrl":"{base}/dl/Linux_Unreal_Engine_5.8.3.zip?X-Amz-Expires=3600&X-Amz-Signature=s"}}]}}"#
        )
    } else {
        r#"{"blobs":[]}"#.to_string()
    };
    let escaped = json.replace('"', "\\\"").replace('&', "\\u0026");
    format!(r#"<html><script>self.__next_f.push([1,"{escaped}"])</script></html>"#)
}

fn sites(base: &str) -> Sites {
    Sites {
        epic: base.into(),
        unreal: base.into(),
    }
}

const WORKS: Fake = Fake {
    silent_works: true,
    page_has_zips: true,
};

#[tokio::test]
async fn opens_through_the_silent_login_and_reads_the_page() {
    let base = fake_epic(WORKS).await;
    let session = UnrealSession::open(sites(&base), "CODESECRET")
        .await
        .unwrap();
    assert!(session.eula_accepted().await.unwrap());
    let all = session.engine_downloads().await.unwrap();
    let versions: Vec<&str> = all.iter().map(|d| d.version.as_str()).collect();
    assert_eq!(versions, ["5.8.3", "5.8.2"]);
    assert_eq!(all[0].size, 3);
    let zip = format!("{base}/dl/Linux_Unreal_Engine_5.8.3.zip?X-Amz-Expires=3600&");
    assert!(all[0].url.starts_with(&zip), "{}", all[0].url);
    let one = session.engine_download("5.8.2").await.unwrap();
    assert_eq!(one.file_name, "Linux_Unreal_Engine_5.8.2.zip");
    let err = session
        .engine_download("4.27.2")
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(
        err.contains("UE 4.27.2 n'est pas proposé sur la page Linux"),
        "{err}"
    );
}

#[tokio::test]
async fn errors_say_which_step_without_secrets() {
    let base = fake_epic(WORKS).await;
    let err = UnrealSession::open(sites(&base), "WRONG")
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(
        err.contains("connexion sur epicgames.com : 409 Conflict"),
        "{err}"
    );
    assert!(err.contains("csrf_token_invalid"), "{err}");

    let no_silent = Fake {
        silent_works: false,
        ..WORKS
    };
    let base = fake_epic(no_silent).await;
    let err = UnrealSession::open(sites(&base), "CODESECRET")
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(
        err.contains("ne reconnaît pas la session après la connexion silencieuse"),
        "{err}"
    );
    assert!(!err.contains("SECRET"), "{err}");

    let empty_page = Fake {
        page_has_zips: false,
        ..WORKS
    };
    let base = fake_epic(empty_page).await;
    let session = UnrealSession::open(sites(&base), "CODESECRET")
        .await
        .unwrap();
    let err = session.engine_downloads().await.err().unwrap().to_string();
    assert!(
        err.contains("la page Linux ne contient aucun zip du moteur"),
        "{err}"
    );

    // Une requête qui échoue ne montre jamais son adresse : le SID est dedans.
    let dead = dead_address();
    let e = reqwest::Client::new()
        .get(format!("{dead}/id/api/set-sid?sid=SIDSECRET"))
        .send()
        .await
        .err()
        .unwrap();
    let opened = UnrealSession::open(sites(&dead), "CODESECRET").await;
    let text = format!("{} {:?}", net(e), opened.err().unwrap());
    assert!(!text.contains("SIDSECRET"), "{text}");
}

#[tokio::test]
async fn the_engine_list_has_no_addresses() {
    let base = fake_epic(WORKS).await;
    let session = UnrealSession::open(sites(&base), "CODESECRET")
        .await
        .unwrap();
    // read_engine_list : la liste sans la garder, pour ne pas toucher à ton cache.
    let list = session.read_engine_list().await.unwrap();
    let versions: Vec<&str> = list.engines.iter().map(|e| e.version.as_str()).collect();
    assert_eq!(versions, ["5.8.3", "5.8.2"]);
    assert_eq!(list.engines[0].size, 3);
    assert_eq!(
        list.engines[1].created_at.as_deref(),
        Some("2026-08-25T00:00:00Z")
    );
    // Ce qui irait sur le disque : ni adresse, ni signature.
    let text = serde_json::to_string(&list).unwrap();
    assert!(
        !text.contains("X-Amz") && !text.contains("/dl/") && !text.contains("SECRET"),
        "{text}"
    );

    // Une page sans zip : une erreur, pas une liste vide qui effacerait la bonne.
    let empty_page = Fake {
        page_has_zips: false,
        ..WORKS
    };
    let base = fake_epic(empty_page).await;
    let session = UnrealSession::open(sites(&base), "CODESECRET")
        .await
        .unwrap();
    assert!(session.read_engine_list().await.is_err());
}
