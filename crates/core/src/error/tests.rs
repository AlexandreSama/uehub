use super::*;
use std::time::Duration;

use crate::test_support::{Reply, dead_address, serve, silent_address};

/// L'erreur de reqwest pour cette adresse, devenue une erreur du core.
async fn fail(client: &reqwest::Client, url: &str) -> NetError {
    let result = client.get(url).send().await;
    let e = match result {
        Ok(response) => match response.error_for_status() {
            Ok(response) => response.json::<serde_json::Value>().await.unwrap_err(),
            Err(e) => e,
        },
        Err(e) => e,
    };
    e.into()
}

#[tokio::test]
async fn each_network_failure_gets_its_sentence() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();

    // Rien n'écoute : pas de connexion.
    let dead = dead_address();
    let e = fail(&client, &format!("{dead}/id/api/set-sid?sid=SIDSECRET")).await;
    assert_eq!(e.kind, NetKind::Offline);
    assert_eq!(e.host.as_deref(), Some("127.0.0.1"));
    assert_eq!(
        e.to_string(),
        "No connection: can't reach 127.0.0.1. Check your Internet access."
    );

    // Un serveur muet : délai dépassé.
    let silent = silent_address().await;
    let e = fail(&client, &format!("{silent}/linux")).await;
    assert_eq!(e.kind, NetKind::Timeout);
    assert!(e.to_string().starts_with("Timed out: 127.0.0.1"), "{e}");

    // Une réponse d'erreur, puis une réponse illisible.
    let base = serve(|request, _| match request.path.as_str() {
        "/down" => Reply::new("503 Service Unavailable"),
        _ => Reply::new("200 OK").body("<html>pas du JSON</html>"),
    })
    .await;
    let e = fail(&client, &format!("{base}/down")).await;
    assert_eq!(e.kind, NetKind::Status(503));
    assert_eq!(e.to_string(), "Error 503 returned by 127.0.0.1.");
    let e = fail(&client, &format!("{base}/page")).await;
    assert_eq!(e.kind, NetKind::Unreadable);
    assert_eq!(
        e.to_string(),
        "Unreadable answer: 127.0.0.1 may have changed its format."
    );
}

#[tokio::test]
async fn the_details_go_to_the_log_without_the_address() {
    let dead = dead_address();
    let url = format!("{dead}/dl/Linux_Unreal_Engine_5.8.3.zip?X-Amz-Signature=SECRET");
    let e: CoreError = reqwest::get(&url).await.unwrap_err().into();
    // La phrase pour l'interface, la cause technique pour le journal.
    let detail = e.detail();
    assert!(
        detail.starts_with("Offline 127.0.0.1: error sending request"),
        "{detail}"
    );
    assert!(detail.contains("Connection refused"), "{detail}");
    for text in [e.to_string(), detail, format!("{e:?}")] {
        assert!(!text.contains("SECRET") && !text.contains("/dl/"), "{text}");
    }
}

#[test]
fn epic_answers_stay_in_the_log() {
    let e = CoreError::epic(
        "Reading your Fab library".into(),
        EpicAPIError::HttpError {
            status: reqwest::StatusCode::FORBIDDEN,
            body: r#"{"errorCode":"errors.com.epicgames.common.missing_permission"}"#.into(),
        },
    );
    assert_eq!(
        e.to_string(),
        "Reading your Fab library: Epic answered 403 Forbidden."
    );
    assert!(e.detail().contains("missing_permission"), "{}", e.detail());
    // Des identifiants refusés : il faut se reconnecter.
    let e = CoreError::epic("x".into(), EpicAPIError::InvalidCredentials);
    assert!(matches!(e, CoreError::NotLoggedIn));
}

#[test]
fn causes_follow_the_chain() {
    let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "accès refusé");
    let e = CoreError::from(io);
    assert_eq!(e.to_string(), "accès refusé");
    assert_eq!(e.detail(), "accès refusé");
}
