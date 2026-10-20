use super::*;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::test_support::{Reply, serve};

const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0fake-jpeg-bytes";

/// Un faux Fab qui sert /img/ok.jpg, et compte les requêtes.
async fn server(hits: Arc<AtomicUsize>) -> String {
    serve(move |request, _| {
        hits.fetch_add(1, Ordering::SeqCst);
        if request.path == "/img/ok.jpg" {
            Reply::new("200 OK").body(JPEG)
        } else {
            Reply::new("404 Not Found")
        }
    })
    .await
}

/// Un cache dans ce dossier, qui n'accepte que les images du faux serveur.
fn cache(dir: &Path, base: &str) -> ThumbCache {
    ThumbCache {
        dir: dir.to_path_buf(),
        http: reqwest::Client::new(),
        allowed: Box::leak(Box::new([&*Box::leak(
            format!("{base}/img/").into_boxed_str(),
        )])),
    }
}

#[tokio::test]
async fn downloads_once_then_reads_from_disk() {
    let hits = Arc::new(AtomicUsize::new(0));
    let base = server(hits.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let c = cache(dir, &base);
    let url = format!("{base}/img/ok.jpg?w=640&h=349");
    assert_eq!(c.get(&url).await.unwrap(), JPEG);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let name = file_name(&url);
    assert_eq!(name.len(), 40);
    assert!(name.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(std::fs::read(dir.join(&name)).unwrap(), JPEG);
    assert!(!dir.join(&name).with_extension("part").exists());
    // Deuxième appel : pas de requête.
    assert_eq!(c.get(&url).await.unwrap(), JPEG);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // Un autre cache sur le même dossier (l'app redémarrée) : toujours pas de requête.
    let c2 = cache(dir, &base);
    assert_eq!(c2.get(&url).await.unwrap(), JPEG);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refuses_urls_outside_the_allowed_prefix() {
    let hits = Arc::new(AtomicUsize::new(0));
    let base = server(hits.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let c = cache(dir.path(), &base);
    for url in [
        format!("{base}/other/ok.jpg"),
        "https://evil.example/img/ok.jpg".to_string(),
        "file:///etc/passwd".to_string(),
    ] {
        assert!(
            matches!(c.get(&url).await, Err(CoreError::Download(_))),
            "{url}"
        );
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn http_errors_are_not_cached() {
    let hits = Arc::new(AtomicUsize::new(0));
    let base = server(hits.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let c = cache(dir.path(), &base);
    let url = format!("{base}/img/missing.jpg");
    assert!(matches!(c.get(&url).await, Err(CoreError::Http(_))));
    assert!(!dir.path().join(file_name(&url)).exists());
    assert!(c.get(&url).await.is_err());
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn real_prefixes_only_let_fab_and_epic_media_through() {
    let c = ThumbCache::new().unwrap();
    assert_eq!(
        c.allowed,
        ["https://media.fab.com/", "https://cdn1.epicgames.com/"]
    );
    for bad in [
        "https://media.fab.com.evil.com/x.jpg",
        "https://media.fab.com@evil.com/x.jpg",
        "http://media.fab.com/x.jpg",
        "https://cdn1.epicgames.com.evil.com/x.jpg",
        "http://cdn1.epicgames.com/x.jpg",
    ] {
        assert!(
            !c.allowed.iter().any(|prefix| bad.starts_with(prefix)),
            "{bad}"
        );
    }
}
