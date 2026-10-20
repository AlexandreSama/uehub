//! Un téléchargement de bout en bout, par l'API publique du core : un faux CDN sur
//! 127.0.0.1, et de vrais morceaux au format d'Epic.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use egs_api::api::types::download_manifest::{DownloadManifest, FileChunkPart, FileManifestList};
use egs_api::api::types::fab_library::{FabAsset, ProjectVersion};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uehub_core::{CoreError, DownloadPlan, Progress, find_assets, pick_version};

/// Un morceau tel que le CDN d'Epic le sert : un en-tête, puis les données, compressées
/// ou non.
fn chunk_bytes(data: &[u8], compressed: bool) -> Vec<u8> {
    let payload = if compressed {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    } else {
        data.to_vec()
    };
    let mut b = Vec::new();
    b.extend_from_slice(&2986228386u32.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&41u32.to_le_bytes());
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    for _ in 0..4 {
        b.extend_from_slice(&0u32.to_le_bytes());
    }
    b.extend_from_slice(&0u64.to_le_bytes());
    b.push(compressed as u8);
    b.extend_from_slice(&payload);
    b
}

fn sha1_hex(data: &[u8]) -> String {
    Sha1::digest(data)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// Le faux CDN : un fichier par chemin, un compteur de requêtes, et un délai pour avoir
/// le temps d'annuler.
async fn serve(routes: HashMap<String, Vec<u8>>, hits: Arc<Mutex<usize>>, delay_ms: u64) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let routes = Arc::new(routes);
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = listener.accept().await.unwrap();
            let routes = routes.clone();
            let hits = hits.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                *hits.lock().unwrap() += 1;
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                match routes.get(&path) {
                    Some(body) => {
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = sock.write_all(head.as_bytes()).await;
                        let _ = sock.write_all(body).await;
                    }
                    None => {
                        let _ = sock
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                }
            });
        }
    });
    format!("http://{addr}")
}

/// Un asset de trois fichiers, en deux morceaux, et ce qu'on doit retrouver sur le disque.
struct Fixture {
    manifest: DownloadManifest,
    expected: HashMap<String, Vec<u8>>,
    hits: Arc<Mutex<usize>>,
    chunk_bytes: u64,
}

async fn fixture(extra_file: Option<FileManifestList>, delay_ms: u64) -> Fixture {
    let c1: Vec<u8> = (0..400u32).map(|i| (i % 251) as u8).collect();
    let c2: Vec<u8> = (0..300u32).map(|i| (i * 7 % 256) as u8).collect();
    let g1 = "aaaaaaaabbbbbbbbccccccccdddddddd".to_string();
    let g2 = "11111111222222223333333344444444".to_string();

    let mut a = c1[0..100].to_vec();
    a.extend_from_slice(&c2[50..150]);
    let b = c1[100..300].to_vec();

    let mut manifest = DownloadManifest {
        manifest_file_version: 18,
        ..Default::default()
    };
    manifest.chunk_hash_list.insert(g1.clone(), 0xABCD);
    manifest.chunk_hash_list.insert(g2.clone(), 0x1234);
    manifest.data_group_list.insert(g1.clone(), 7);
    manifest.data_group_list.insert(g2.clone(), 42);
    let part = |g: &str, offset: u128, size: u128| FileChunkPart {
        guid: g.to_string(),
        link: None,
        offset,
        size,
    };
    manifest.file_manifest_list = vec![
        FileManifestList {
            filename: "Content/Maps/A.umap".into(),
            file_hash: sha1_hex(&a),
            file_chunk_parts: vec![part(&g1, 0, 100), part(&g2, 50, 100)],
        },
        FileManifestList {
            filename: "Content/B.uasset".into(),
            file_hash: sha1_hex(&b),
            file_chunk_parts: vec![part(&g1, 100, 200)],
        },
        FileManifestList {
            filename: "Content/Empty.txt".into(),
            file_hash: sha1_hex(b""),
            file_chunk_parts: vec![],
        },
    ];
    if let Some(f) = extra_file {
        manifest.file_manifest_list.push(f);
    }

    let mut routes = HashMap::new();
    routes.insert(
        format!(
            "/base/ChunksV4/07/{:016X}_{}.chunk",
            0xABCDu128,
            g1.to_uppercase()
        ),
        chunk_bytes(&c1, true),
    );
    routes.insert(
        format!(
            "/base/ChunksV4/42/{:016X}_{}.chunk",
            0x1234u128,
            g2.to_uppercase()
        ),
        chunk_bytes(&c2, false),
    );
    let chunk_total: u64 = routes.values().map(|v| v.len() as u64).sum();
    let hits = Arc::new(Mutex::new(0));
    let base = serve(routes, hits.clone(), delay_ms).await;
    manifest.custom_fields = Some(HashMap::from([(
        "SourceURL".to_string(),
        format!("{base}/base"),
    )]));

    let mut expected = HashMap::new();
    expected.insert("Content/Maps/A.umap".to_string(), a);
    expected.insert("Content/B.uasset".to_string(), b);
    expected.insert("Content/Empty.txt".to_string(), vec![]);
    Fixture {
        manifest,
        expected,
        hits,
        chunk_bytes: chunk_total,
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

/// Le téléchargement doit être Send : Tauri lance ses commandes sur plusieurs fils.
#[test]
fn download_future_is_send() {
    fn assert_send<T: Send>(_: &T) {}
    let plan = DownloadPlan::from_manifest(&DownloadManifest::default());
    let http = http();
    let noop = |_: Progress| {};
    let fut = plan.download_chunks(&http, std::path::Path::new("/tmp/x"), 2, &noop);
    assert_send(&fut);
}

#[tokio::test]
async fn full_download_and_resume() {
    let fx = fixture(None, 0).await;
    let plan = DownloadPlan::from_manifest(&fx.manifest);
    assert_eq!(plan.file_count(), 3);
    assert_eq!(plan.install_size(), 400);

    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let cache = root.join("cache");
    let out = root.join("out");
    let events = Mutex::new(Vec::new());
    let on_progress = |p: Progress| events.lock().unwrap().push(p);

    plan.download_chunks(&http(), &cache, 4, &on_progress)
        .await
        .unwrap();
    assert_eq!(
        *fx.hits.lock().unwrap(),
        2,
        "chaque morceau est demandé une fois"
    );
    plan.assemble_files(&cache, &out, &on_progress)
        .await
        .unwrap();

    for (name, data) in &fx.expected {
        assert_eq!(&std::fs::read(out.join(name)).unwrap(), data, "{name}");
    }
    {
        let ev = events.lock().unwrap();
        assert!(matches!(
            ev.first(),
            Some(Progress::Chunks {
                done: 0,
                total: 2,
                ..
            })
        ));
        // Les octets reçus des deux morceaux sont comptés à leur arrivée.
        let bytes: Vec<u64> = ev
            .iter()
            .filter_map(|p| match p {
                Progress::Chunks { bytes, .. } => Some(*bytes),
                _ => None,
            })
            .collect();
        assert_eq!(bytes[0], 0);
        assert!(bytes.windows(2).all(|w| w[0] < w[1]), "{bytes:?}");
        assert_eq!(*bytes.last().unwrap(), fx.chunk_bytes, "{bytes:?}");
        assert!(matches!(
            ev.last(),
            Some(Progress::Files { done: 3, total: 3 })
        ));
    }
    plan.download_chunks(&http(), &cache, 4, &on_progress)
        .await
        .unwrap();
    assert_eq!(
        *fx.hits.lock().unwrap(),
        2,
        "les morceaux en cache ne sont pas redemandés"
    );
}

#[tokio::test]
async fn corrupted_cache_is_detected_and_purged() {
    let fx = fixture(None, 0).await;
    let plan = DownloadPlan::from_manifest(&fx.manifest);
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let cache = root.join("cache");
    let noop = |_: Progress| {};
    plan.download_chunks(&http(), &cache, 2, &noop)
        .await
        .unwrap();
    let p = cache.join("aaaaaaaabbbbbbbbccccccccdddddddd.bin");
    let mut d = std::fs::read(&p).unwrap();
    d[10] ^= 0xFF;
    std::fs::write(&p, d).unwrap();
    let err = plan
        .assemble_files(&cache, &root.join("out"), &noop)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::HashMismatch(_)), "{err}");
    assert!(!p.exists(), "le morceau abîmé quitte le cache");
    plan.download_chunks(&http(), &cache, 2, &noop)
        .await
        .unwrap();
    plan.assemble_files(&cache, &root.join("out"), &noop)
        .await
        .unwrap();
}

#[tokio::test]
async fn path_traversal_is_refused() {
    let evil = FileManifestList {
        filename: "../evil.txt".into(),
        file_hash: sha1_hex(b""),
        file_chunk_parts: vec![],
    };
    let fx = fixture(Some(evil), 0).await;
    let plan = DownloadPlan::from_manifest(&fx.manifest);
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let noop = |_: Progress| {};
    plan.download_chunks(&http(), &root.join("cache"), 2, &noop)
        .await
        .unwrap();
    let err = plan
        .assemble_files(&root.join("cache"), &root.join("out"), &noop)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Download(_)), "{err}");
    assert!(!root.join("evil.txt").exists());
}

#[tokio::test]
async fn missing_chunk_fails_after_retries() {
    let mut fx = fixture(None, 0).await;
    let url = fx.manifest.custom_fields.as_ref().unwrap()["SourceURL"].replace("/base", "/nope");
    fx.manifest.custom_fields = Some(HashMap::from([("SourceURL".to_string(), url)]));
    let plan = DownloadPlan::from_manifest(&fx.manifest);
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let noop = |_: Progress| {};
    let t = std::time::Instant::now();
    let err = plan
        .download_chunks(&http(), &root.join("cache"), 2, &noop)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Net(_)), "{err}");
    assert!(
        t.elapsed().as_secs() >= 6,
        "trois essais, après 1, 2 puis 3 secondes"
    );
}

/// Abandonner le téléchargement en route (ce que fait une annulation) laisse un cache
/// dont on peut repartir.
#[tokio::test(flavor = "multi_thread")]
async fn cancel_then_resume() {
    let fx = fixture(None, 300).await;
    let plan = DownloadPlan::from_manifest(&fx.manifest);
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let cache = root.join("cache");
    let noop = |_: Progress| {};
    let res = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        plan.download_chunks(&http(), &cache, 1, &noop),
    )
    .await;
    assert!(res.is_err(), "annulé pendant le premier morceau");
    // Aucun morceau à moitié écrit ne passe pour complet.
    if cache.exists() {
        for e in std::fs::read_dir(&cache).unwrap() {
            let p = e.unwrap().path();
            assert!(
                p.extension().is_none_or(|x| x != "bin"),
                "pas encore de .bin : {}",
                p.display()
            );
        }
    }
    plan.download_chunks(&http(), &cache, 1, &noop)
        .await
        .unwrap();
    plan.assemble_files(&cache, &root.join("out"), &noop)
        .await
        .unwrap();
    for (name, data) in &fx.expected {
        assert_eq!(&std::fs::read(root.join("out").join(name)).unwrap(), data);
    }
}

fn version(id: &str, engines: &[&str]) -> ProjectVersion {
    ProjectVersion {
        artifact_id: id.into(),
        engine_versions: engines.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn selection_helpers() {
    let als = FabAsset {
        asset_id: "289da315".into(),
        title: "Advanced Locomotion System V4".into(),
        project_versions: vec![
            version("V1", &["UE_4.18"]),
            version("V7", &["UE_4.27", "UE_5.0", "UE_5.4"]),
            version("V4", &["UE_4.20", "UE_4.23"]),
        ],
        ..Default::default()
    };
    let grass = FabAsset {
        asset_id: "x1".into(),
        title: "Megascans: Grass".into(),
        ..Default::default()
    };
    let grass2 = FabAsset {
        asset_id: "x2".into(),
        title: "Megascans: Grass Vol. 2".into(),
        ..Default::default()
    };
    let lib = vec![als.clone(), grass, grass2];
    assert_eq!(find_assets(&lib, "locomotion").len(), 1);
    assert_eq!(
        find_assets(&lib, "megascans: grass")[0].asset_id,
        "x1",
        "le titre exact passe devant"
    );
    assert_eq!(find_assets(&lib, "grass").len(), 2);
    assert_eq!(find_assets(&lib, "x2")[0].title, "Megascans: Grass Vol. 2");
    assert!(find_assets(&lib, "nothing").is_empty());
    assert_eq!(pick_version(&als, None).unwrap().artifact_id, "V7");
    assert_eq!(pick_version(&als, Some("4.20")).unwrap().artifact_id, "V4");
    assert_eq!(
        pick_version(&als, Some("UE_4.18")).unwrap().artifact_id,
        "V1"
    );
    assert!(pick_version(&als, Some("5.6")).is_none());
}
