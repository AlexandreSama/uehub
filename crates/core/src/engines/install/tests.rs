use super::*;
use std::io::{Seek, Write};
use std::sync::Mutex;

use tokio::io::AsyncWriteExt;
use zip::write::SimpleFileOptions;

use crate::test_support::{dead_address, read_request};

/// Un petit zip de moteur : l'éditeur (0755), Build.version, un fichier compressé de
/// 1 Mo, deux liens symboliques, et des entrées à ignorer. `root` range tout dans un
/// dossier de tête, comme certains zips d'Epic.
fn engine_zip(root: Option<&str>) -> Vec<u8> {
    engine_zip_with(root, 0, false)
}

/// Le même, plus `noise` octets incompressibles dans Engine/Content/noise.bin (au-delà de
/// 64 Ko, le zip dépasse la première requête de l'index). `streamed` : écrit comme par un
/// outil qui ne peut pas revenir en arrière, avec les tailles après chaque fichier
/// (descripteurs de données), et un fichier en ZIP64.
fn engine_zip_with(root: Option<&str>, noise: usize, streamed: bool) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    if streamed {
        let mut zip = zip::ZipWriter::new_stream(&mut out);
        fill_zip(&mut zip, root, noise, true);
        zip.finish().unwrap();
    } else {
        let mut zip = zip::ZipWriter::new(&mut out);
        fill_zip(&mut zip, root, noise, false);
        zip.finish().unwrap();
    }
    out.into_inner()
}

/// Des octets qui ne se compressent pas, toujours les mêmes.
fn noise_bytes(len: usize) -> Vec<u8> {
    let mut x: u32 = 0x9E37_79B9;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}

fn fill_zip<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    root: Option<&str>,
    noise: usize,
    large: bool,
) {
    let p = |name: &str| match root {
        Some(r) => format!("{r}/{name}"),
        None => name.to_string(),
    };
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.add_directory(p("Engine/Binaries/Linux/"), stored.unix_permissions(0o755))
        .unwrap();
    zip.start_file(
        p("Engine/Binaries/Linux/UnrealEditor"),
        stored.unix_permissions(0o755),
    )
    .unwrap();
    zip.write_all(b"#!/bin/sh\necho editor\n").unwrap();
    zip.start_file(
        p("Engine/Binaries/Linux/libfoo.so.1"),
        stored.unix_permissions(0o644),
    )
    .unwrap();
    zip.write_all(b"lib").unwrap();
    zip.add_symlink(p("Engine/Binaries/Linux/libfoo.so"), "libfoo.so.1", stored)
        .unwrap();
    zip.add_symlink(p("Engine/Binaries/Linux/evil"), "/etc/passwd", stored)
        .unwrap();
    zip.add_symlink(p("Engine/escape"), "../../outside", stored)
        .unwrap();
    zip.start_file(
        p("Engine/Build/Build.version"),
        stored.unix_permissions(0o444),
    )
    .unwrap();
    zip.write_all(
        br#"{"MajorVersion":5,"MinorVersion":8,"PatchVersion":3,"BranchName":"++UE5+Release-5.8"}"#,
    )
    .unwrap();
    zip.start_file(
        p("Engine/Content/big.bin"),
        deflated.unix_permissions(0o644),
    )
    .unwrap();
    let big: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
    zip.write_all(&big).unwrap();
    if noise > 0 {
        zip.start_file(
            p("Engine/Content/noise.bin"),
            deflated.unix_permissions(0o644).large_file(large),
        )
        .unwrap();
        zip.write_all(&noise_bytes(noise)).unwrap();
    }
    zip.start_file("../evil.txt", stored).unwrap();
    zip.write_all(b"no").unwrap();
}

/// Comment le faux CDN se comporte.
#[derive(Clone, Copy)]
enum Mode {
    /// Répond normalement
    Plain,
    /// Coupe la première connexion après tant d'octets
    CutFirstAt(usize),
    /// Coupe, une fois, la première réponse qui passe par cet octet du fichier
    CutOnceAt(usize),
    /// Abîme, une fois, cet octet du fichier
    CorruptOnce(usize),
    /// Ignore l'en-tête Range et renvoie tout le fichier
    IgnoreRange,
    /// Envoie tant d'octets avec la première connexion, puis se tait sans fermer
    StallAfter(usize),
}

/// Un faux CDN pour un seul fichier. Le jeton « old » a expiré (403). Garde l'en-tête
/// Range de chaque requête.
async fn cdn(data: Vec<u8>, mode: Mode) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        let mut count = 0;
        let mut applied = false;
        while let Ok((mut sock, _)) = listener.accept().await {
            count += 1;
            let Some(request) = read_request(&mut sock).await else {
                continue;
            };
            let range = request.header("range").map(|r| {
                let start = r.trim_start_matches("bytes=").trim_end_matches('-');
                start.parse::<usize>().unwrap()
            });
            log.lock().unwrap().push(format!("{range:?}"));
            if request.query.get("token").map(String::as_str) == Some("old") {
                let _ = sock
                    .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 15\r\nConnection: close\r\n\r\nRequest expired")
                    .await;
                continue;
            }
            let start = match mode {
                Mode::IgnoreRange => 0,
                _ => range.unwrap_or(0),
            };
            let body = &data[start..];
            let head = if start > 0 {
                format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    data.len() - 1,
                    data.len(),
                    body.len()
                )
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
            };
            let _ = sock.write_all(head.as_bytes()).await;
            match mode {
                Mode::CutFirstAt(cut) if count == 1 => {
                    let _ = sock.write_all(&body[..cut]).await;
                }
                Mode::CutOnceAt(at) if !applied && start < at => {
                    applied = true;
                    let _ = sock.write_all(&body[..at - start]).await;
                }
                Mode::CorruptOnce(at) if !applied && start <= at => {
                    applied = true;
                    let mut damaged = body.to_vec();
                    damaged[at - start] ^= 0xFF;
                    let _ = sock.write_all(&damaged).await;
                }
                Mode::StallAfter(n) if count == 1 => {
                    let _ = sock.write_all(&body[..n]).await;
                    // La connexion reste ouverte et muette, sans bloquer la suivante.
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        drop(sock);
                    });
                }
                _ => {
                    let _ = sock.write_all(body).await;
                }
            }
        }
    });
    (base, seen)
}

fn engine(base: &str, token: &str, size: u64) -> EngineDownload {
    EngineDownload {
        version: "5.8.3".into(),
        file_name: "Linux_Unreal_Engine_5.8.3.zip".into(),
        size,
        created_at: None,
        url: format!("{base}/Linux_Unreal_Engine_5.8.3.zip?token={token}&X-Amz-Signature=s"),
    }
}

/// Garde chaque étape d'avancement.
type Events = Arc<Mutex<Vec<InstallProgress>>>;

fn recorder() -> (Events, impl Fn(InstallProgress) + Send + Sync) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    (events, move |p| e.lock().unwrap().push(p))
}

fn mode_of(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Une installation dont le zip est déjà téléchargé dans un dossier temporaire.
fn installed_from(zip_bytes: Vec<u8>) -> (EngineInstall, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let size = zip_bytes.len() as u64;
    let install = EngineInstall::new(
        &engine("http://unused", "a", size),
        Some(root.path().join("5.8.3")),
    )
    .unwrap();
    fs::write(&install.zip, &zip_bytes).unwrap();
    (install, root)
}

/// Un faux moteur sur le disque, marqué comme installé par uehub ou non.
fn fake_engine(dir: &Path, minor: u32, patch: u32, marked: bool) {
    fs::create_dir_all(dir.join("Engine/Build")).unwrap();
    fs::create_dir_all(dir.join("Engine/Binaries/Linux")).unwrap();
    fs::write(dir.join(EDITOR), "#!/bin/sh\n").unwrap();
    fs::write(
        dir.join("Engine/Build/Build.version"),
        format!(r#"{{"MajorVersion":5,"MinorVersion":{minor},"PatchVersion":{patch}}}"#),
    )
    .unwrap();
    for i in 0..50 {
        fs::write(dir.join(format!("Engine/file{i}.bin")), "x").unwrap();
    }
    if marked {
        fs::write(dir.join(MARKER), format!("5.{minor}.{patch}")).unwrap();
    }
}

#[tokio::test]
async fn download_resumes_after_a_cut_and_after_expiry() {
    let data = engine_zip(None);
    let size = data.len() as u64;
    let (base, seen) = cdn(data.clone(), Mode::CutFirstAt(1000)).await;
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let install = EngineInstall::new(&engine(&base, "a", size), Some(root.join("5.8.3"))).unwrap();
    assert_eq!(install.zip, root.join("Linux_Unreal_Engine_5.8.3.zip"));
    // Une installation commencée avec uehub 0.2 : le zip est sur le disque, l'ancien chemin
    // continue.
    fs::write(&install.zip, b"").unwrap();
    assert!(install.zip_on_disk());
    let (events, on) = recorder();

    let first = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(first.unwrap(), Downloaded::Interrupted);
    assert_eq!(install.downloaded(), 1000);
    let expired = install.download(&engine(&base, "old", size), &on).await;
    assert_eq!(expired.unwrap(), Downloaded::Expired);
    assert_eq!(
        install.downloaded(),
        1000,
        "une adresse expirée n'écrit rien"
    );
    let fresh = install.download(&engine(&base, "fresh", size), &on).await;
    assert_eq!(fresh.unwrap(), Downloaded::Complete);
    assert_eq!(fs::read(&install.zip).unwrap(), data);
    assert_eq!(
        *seen.lock().unwrap(),
        ["Some(0)", "Some(1000)", "Some(1000)"]
    );
    // Déjà complet : rien n'est redemandé.
    let again = install.download(&engine(&base, "fresh", size), &on).await;
    assert_eq!(again.unwrap(), Downloaded::Complete);
    assert_eq!(seen.lock().unwrap().len(), 3);
    let last = *events.lock().unwrap().last().unwrap();
    assert_eq!(
        last,
        InstallProgress::Download {
            done: size,
            total: size
        }
    );
}

#[tokio::test]
async fn server_ignoring_ranges_and_oversized_files_restart_from_zero() {
    let data = engine_zip(None);
    let size = data.len() as u64;
    let (base, _) = cdn(data.clone(), Mode::IgnoreRange).await;
    let root = tempfile::tempdir().unwrap();
    let install =
        EngineInstall::new(&engine(&base, "a", size), Some(root.path().join("5.8.3"))).unwrap();
    fs::write(&install.zip, b"partial").unwrap();
    let (_, on) = recorder();
    let done = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(done.unwrap(), Downloaded::Complete);
    assert_eq!(fs::read(&install.zip).unwrap(), data);
    fs::write(&install.zip, vec![0u8; size as usize + 10]).unwrap();
    let done = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(done.unwrap(), Downloaded::Complete);
    assert_eq!(fs::read(&install.zip).unwrap(), data);
}

#[tokio::test]
async fn extraction_keeps_modes_links_and_skips_dangerous_entries() {
    for root_folder in [None, Some("Linux_Unreal_Engine_5.8.3")] {
        let (install, root) = installed_from(engine_zip(root_folder));
        let root = root.path();
        let (events, on) = recorder();
        let files = install.extract(&on).await.unwrap();
        assert_eq!(
            files, 9,
            "chaque entrée du zip compte, même celles qu'on saute"
        );
        let dir = &install.dir;
        let editor = dir.join(EDITOR);
        assert_eq!(
            fs::read_to_string(&editor).unwrap(),
            "#!/bin/sh\necho editor\n"
        );
        assert_eq!(mode_of(&editor), 0o755);
        assert_eq!(
            mode_of(&dir.join("Engine/Build/Build.version")),
            0o644,
            "un fichier en lecture seule devient modifiable par son propriétaire"
        );
        assert_eq!(
            fs::read_link(dir.join("Engine/Binaries/Linux/libfoo.so")).unwrap(),
            Path::new("libfoo.so.1")
        );
        assert!(fs::symlink_metadata(dir.join("Engine/Binaries/Linux/evil")).is_err());
        assert!(fs::symlink_metadata(dir.join("Engine/escape")).is_err());
        assert!(!root.join("evil.txt").exists() && !root.join("5.8.3/evil.txt").exists());
        let big = fs::metadata(dir.join("Engine/Content/big.bin")).unwrap();
        assert_eq!(big.len(), 1_000_000);
        assert!(
            !install.zip.exists(),
            "le zip part une fois l'extraction réussie"
        );
        assert_eq!(
            fs::read_to_string(dir.join(MARKER)).unwrap(),
            "5.8.3",
            "uehub marque ce qu'il installe"
        );
        assert!(install.is_installed());
        let last = *events.lock().unwrap().last().unwrap();
        assert!(
            matches!(last, InstallProgress::Extract { done, total } if done == total),
            "{last:?}"
        );
        // Enregistré, et listé avec sa version.
        let ini = root.join("Install.ini");
        let id = register_engine(&ini, dir).unwrap();
        let listed = installed_in(&ini);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
        assert_eq!(listed[0].version.as_deref(), Some("5.8.3"));
        assert!(listed[0].by_uehub);
    }
}

#[tokio::test]
async fn extraction_resumes_and_repairs_modes() {
    let (install, _root) = installed_from(engine_zip(None));
    let dir = install.dir.clone();
    fs::create_dir_all(dir.join("Engine/Content")).unwrap();
    fs::create_dir_all(dir.join("Engine/Binaries/Linux")).unwrap();
    fs::write(dir.join("Engine/Content/big.bin"), b"truncated").unwrap();
    fs::write(dir.join(EDITOR), b"#!/bin/sh\necho editor\n").unwrap();
    fs::set_permissions(dir.join(EDITOR), fs::Permissions::from_mode(0o644)).unwrap();
    let (_, on) = recorder();
    install.extract(&on).await.unwrap();
    let big = fs::metadata(dir.join("Engine/Content/big.bin")).unwrap();
    assert_eq!(big.len(), 1_000_000);
    assert_eq!(mode_of(&dir.join(EDITOR)), 0o755);
}

#[tokio::test]
async fn stop_flag_and_corruption() {
    let (install, _root) = installed_from(engine_zip(None));
    let stop = AtomicBool::new(true);
    let err = extract_zip(&install.zip, &install.dir, &stop, &|_| {})
        .unwrap_err()
        .to_string();
    assert_eq!(err, "Extraction interrupted.");
    // Le garde lève le drapeau quand le future est abandonné.
    let flag = Arc::new(AtomicBool::new(false));
    drop(StopOnDrop(flag.clone()));
    assert!(flag.load(Ordering::Relaxed));

    // Un octet changé dans l'éditeur, stocké sans compression : le CRC ne correspond plus,
    // et le fichier est supprimé.
    let mut bytes = engine_zip(None);
    let at = bytes.windows(4).position(|w| w == b"echo").unwrap();
    bytes[at] = b'X';
    let (install, _root) = installed_from(bytes);
    let (_, on) = recorder();
    let err = install.extract(&on).await.unwrap_err().to_string();
    assert!(err.contains("UnrealEditor"), "{err}");
    assert!(
        !install.dir.join(EDITOR).exists(),
        "un fichier abîmé ne doit pas rester"
    );
    assert!(install.zip.exists(), "le zip reste pour un nouvel essai");
}

#[test]
fn space_check_and_modes() {
    let err = check_space(Path::new("/tmp/does/not/exist/yet"), u64::MAX / 2)
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("Not enough space: about "), "{err}");
    assert!(err.contains(" GB free are needed in /tmp,"), "{err}");
    assert!(check_space(Path::new("/tmp/x"), 1).is_ok());
    assert_eq!(file_mode(Some(0o100755), Path::new("a")), 0o755);
    assert_eq!(file_mode(Some(0o100444), Path::new("a")), 0o644);
    let mode = |path: &str| file_mode(None, Path::new(path));
    assert_eq!(mode("Engine/Binaries/Linux/UnrealEditor"), 0o755);
    assert_eq!(mode("Engine/Build/BatchFiles/Linux/Setup.sh"), 0o755);
    assert_eq!(mode("Engine/Binaries/Linux/libx.so"), 0o644);
    assert_eq!(mode("Engine/Content/a.uasset"), 0o644);
    let inside = |from: &str, to: &str| stays_inside(Path::new(from), Path::new(to));
    assert!(inside("Engine/Binaries/Linux/libfoo.so", "libfoo.so.1"));
    assert!(inside("Engine/a/b", "../../c"));
    assert!(!inside("Engine/a", "../../c"));
    assert!(!inside("a", "/etc/passwd"));
}

#[test]
fn install_ini_is_merged_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let ini = root.join("Epic/UnrealEngine/Install.ini");
    let dir_a = root.join("UE/5.8.3");
    let dir_b = root.join("UE/5.7.4");
    // Un fichier neuf.
    let a = register_engine(&ini, &dir_a).unwrap();
    assert_eq!(
        fs::read_to_string(&ini).unwrap(),
        format!("[Installations]\n{a}={}\n", dir_a.display())
    );
    assert_eq!(a.len(), 36);
    assert_eq!(a, a.to_uppercase());
    // Le même dossier : le même identifiant, le fichier ne change pas.
    assert_eq!(register_engine(&ini, &dir_a).unwrap(), a);
    // Un fichier qui contient autre chose.
    fs::write(
        &ini,
        "[Other]\nkey=value\n\n[Installations]\nOLD-ID=/somewhere/else\n",
    )
    .unwrap();
    let b = register_engine(&ini, &dir_b).unwrap();
    assert_eq!(
        fs::read_to_string(&ini).unwrap(),
        format!(
            "[Other]\nkey=value\n\n[Installations]\n{b}={}\nOLD-ID=/somewhere/else\n",
            dir_b.display()
        )
    );
    // Sans la section, elle s'ajoute après une ligne vide.
    fs::write(&ini, "[Other]\nkey=value").unwrap();
    let c = register_engine(&ini, &dir_a).unwrap();
    assert_eq!(
        fs::read_to_string(&ini).unwrap(),
        format!(
            "[Other]\nkey=value\n\n[Installations]\n{c}={}\n",
            dir_a.display()
        )
    );
    // La liste : seuls les dossiers qui contiennent un moteur, du plus récent au plus ancien.
    for (dir, minor) in [(&dir_a, 8), (&dir_b, 7)] {
        fs::create_dir_all(dir.join("Engine/Build")).unwrap();
        fs::write(
            dir.join("Engine/Build/Build.version"),
            format!(r#"{{"MajorVersion":5,"MinorVersion":{minor},"PatchVersion":4}}"#),
        )
        .unwrap();
    }
    register_engine(&ini, &dir_b).unwrap();
    let listed: Vec<Option<String>> = installed_in(&ini).into_iter().map(|e| e.version).collect();
    assert_eq!(listed, [Some("5.8.4".into()), Some("5.7.4".into())]);
    assert!(installed_in(&root.join("missing.ini")).is_empty());
}

#[tokio::test]
async fn a_silent_connection_counts_as_interrupted() {
    let data = engine_zip(None);
    let size = data.len() as u64;
    let (base, seen) = cdn(data.clone(), Mode::StallAfter(500)).await;
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let mut install =
        EngineInstall::new(&engine(&base, "a", size), Some(root.join("5.8.3"))).unwrap();
    // Le même client que `new`, avec un délai court pour que le test reste rapide.
    install.http = reqwest::Client::builder()
        .read_timeout(std::time::Duration::from_millis(300))
        .build()
        .unwrap();
    fs::write(&install.zip, b"").unwrap();
    let (_, on) = recorder();
    let t0 = std::time::Instant::now();
    let first = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(first.unwrap(), Downloaded::Interrupted);
    assert!(t0.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(install.downloaded(), 500);
    let second = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(second.unwrap(), Downloaded::Complete);
    assert_eq!(fs::read(&install.zip).unwrap(), data);
    assert_eq!(*seen.lock().unwrap(), ["Some(0)", "Some(500)"]);
    assert!(crate::READ_TIMEOUT >= std::time::Duration::from_secs(30));
    // Personne n'écoute : une interruption aussi (c'est l'appelant qui compte les essais).
    let dead = dead_address();
    let closed =
        EngineInstall::new(&engine(&dead, "a", size + 1), Some(root.join("5.8.4"))).unwrap();
    let nobody = closed.download(&engine(&dead, "a", size + 1), &on).await;
    assert_eq!(nobody.unwrap(), Downloaded::Interrupted);
}

#[tokio::test]
async fn folder_checks_and_discard() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let e = engine("http://unused", "a", 10);
    // Absent ou vide : d'accord.
    assert!(EngineInstall::new(&e, Some(root.join("5.8.3"))).is_ok());
    fs::create_dir_all(root.join("empty")).unwrap();
    assert!(EngineInstall::new(&e, Some(root.join("empty"))).is_ok());
    // Les fichiers de quelqu'un d'autre : refusé, avec le dossier dans le message.
    fs::create_dir_all(root.join("mine")).unwrap();
    fs::write(root.join("mine/notes.txt"), "keep me").unwrap();
    let err = EngineInstall::new(&e, Some(root.join("mine")))
        .err()
        .unwrap()
        .to_string();
    assert!(
        err.contains("mine already exists and doesn't contain this engine"),
        "{err}"
    );
    // Une installation à nous, inachevée (le zip est à côté) : acceptée, pour reprendre.
    fs::create_dir_all(root.join("ours/Engine")).unwrap();
    fs::write(root.join("ours/Engine/a.bin"), "x").unwrap();
    let zip = EngineInstall::zip_path("5.8.3", &root.join("ours")).unwrap();
    assert_eq!(zip, root.join("Linux_Unreal_Engine_5.8.3.zip"));
    fs::write(&zip, "partial").unwrap();
    assert!(EngineInstall::check_dir("5.8.3", &root.join("ours")).is_ok());
    assert!(!EngineInstall::is_installed_in("5.8.3", &root.join("ours")));
    // discard efface le dossier inachevé et le zip…
    EngineInstall::discard("5.8.3", &root.join("ours"))
        .await
        .unwrap();
    assert!(!root.join("ours").exists() && !zip.exists());
    // … mais jamais un moteur fini (sans zip), ni rien quand rien n'a commencé.
    fs::create_dir_all(root.join("done/Engine/Binaries/Linux")).unwrap();
    fs::write(root.join("done").join(EDITOR), "editor").unwrap();
    assert!(EngineInstall::is_installed_in("5.8.3", &root.join("done")));
    assert!(EngineInstall::check_dir("5.8.3", &root.join("done")).is_ok());
    EngineInstall::discard("5.8.3", &root.join("done"))
        .await
        .unwrap();
    assert!(root.join("done").join(EDITOR).exists());
    EngineInstall::discard("5.8.3", &root.join("mine"))
        .await
        .unwrap();
    assert!(root.join("mine/notes.txt").exists());
}

#[tokio::test]
async fn uninstall_only_what_uehub_installed() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let ini = root.join("Install.ini");
    let ours = root.join("UE/5.8.3");
    let theirs = root.join("src/UnrealEngine");
    fake_engine(&ours, 8, 3, true);
    fake_engine(&theirs, 7, 4, false);
    fs::write(&ini, "[Other]\nkey=value\n\n[Installations]\n").unwrap();
    let a = register_engine(&ini, &ours).unwrap();
    let b = register_engine(&ini, &theirs).unwrap();

    let flags: Vec<(Option<String>, bool)> = installed_in(&ini)
        .iter()
        .map(|e| (e.version.clone(), e.by_uehub))
        .collect();
    assert_eq!(
        flags,
        [(Some("5.8.3".into()), true), (Some("5.7.4".into()), false)]
    );

    // Pas à nous : refusé, rien n'est touché.
    let err = uninstall_from(&ini, &theirs).await.unwrap_err().to_string();
    assert!(err.contains("wasn't installed by uehub"), "{err}");
    assert!(theirs.join(EDITOR).exists());
    assert!(fs::read_to_string(&ini).unwrap().contains(&b));

    // Un reste de désinstallation interrompue ne bloque pas la suivante.
    fs::create_dir_all(root.join("UE/.5.8.3.suppression/old")).unwrap();
    uninstall_from(&ini, &ours).await.unwrap();
    assert!(!ours.exists());
    assert!(!root.join("UE/.5.8.3.suppression").exists());
    let text = fs::read_to_string(&ini).unwrap();
    assert_eq!(
        text,
        format!(
            "[Other]\nkey=value\n\n[Installations]\n{b}={}\n",
            theirs.display()
        )
    );
    assert!(!text.contains(&a));
    assert_eq!(installed_in(&ini).len(), 1);

    // Deux fois : la marque est partie avec le dossier.
    assert!(uninstall_from(&ini, &ours).await.is_err());
    // Une ligne hors de [Installations] avec le même chemin reste.
    fs::write(
        &ini,
        format!(
            "[Recent]\nX={}\n[Installations]\nY={}\n",
            theirs.display(),
            theirs.display()
        ),
    )
    .unwrap();
    unregister_engine(&ini, &theirs).unwrap();
    assert_eq!(
        fs::read_to_string(&ini).unwrap(),
        format!("[Recent]\nX={}\n[Installations]\n", theirs.display())
    );
    // Pas d'Install.ini : rien à faire.
    unregister_engine(&root.join("none.ini"), &theirs).unwrap();
}

#[test]
fn free_space_measures_the_first_existing_parent() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let missing = root.join("UnrealEngine/5.8.3");
    assert!(!missing.exists());
    let free = free_space(&missing).unwrap();
    assert!(free > 0);
    let parent = fs4::available_space(root).unwrap();
    // Le même disque : à quelques Mo près (d'autres programmes écrivent pendant ce temps).
    assert!(free.abs_diff(parent) < 64 << 20, "{free} vs {parent}");
    assert_eq!(existing_parent(&missing), root);
    assert_eq!(
        existing_parent(Path::new("/nonexistent-uehub/a/b")),
        Path::new("/")
    );
}

#[test]
fn extracted_size_uses_the_ratio() {
    assert_eq!(extracted_size(100), 180);
    assert_eq!(extracted_size(39_800_000_000), 71_640_000_000);
}

// ---------------------------------------------------------------------------
// L'extraction pendant le téléchargement
// ---------------------------------------------------------------------------

/// Où commence une entrée du zip (son en-tête local), et où commencent ses données.
fn offsets(data: &[u8], name: &str) -> (usize, usize) {
    let mut archive = ZipArchive::new(std::io::Cursor::new(data)).unwrap();
    let entry = archive.by_name(name).unwrap();
    (
        entry.header_start() as usize,
        entry.data_start().unwrap() as usize,
    )
}

/// Le début de l'index du zip.
fn index_of(data: &[u8]) -> u64 {
    ZipArchive::new(std::io::Cursor::new(data))
        .unwrap()
        .central_directory_start()
}

/// Une installation neuve, vers un dossier temporaire.
fn fresh(base: &str, size: u64) -> (EngineInstall, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let install =
        EngineInstall::new(&engine(base, "a", size), Some(root.path().join("5.8.3"))).unwrap();
    (install, root)
}

/// Le moteur extrait est complet et sûr : contenu, droits, liens, entrées dangereuses.
fn check_engine(dir: &Path, root: &Path, noise: usize) {
    let editor = dir.join(EDITOR);
    assert_eq!(
        fs::read_to_string(&editor).unwrap(),
        "#!/bin/sh\necho editor\n"
    );
    assert_eq!(mode_of(&editor), 0o755);
    assert_eq!(mode_of(&dir.join("Engine/Build/Build.version")), 0o644);
    assert_eq!(
        fs::read_link(dir.join("Engine/Binaries/Linux/libfoo.so")).unwrap(),
        Path::new("libfoo.so.1")
    );
    assert!(fs::symlink_metadata(dir.join("Engine/Binaries/Linux/evil")).is_err());
    assert!(fs::symlink_metadata(dir.join("Engine/escape")).is_err());
    assert!(!root.join("evil.txt").exists() && !dir.join("evil.txt").exists());
    let big: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
    assert_eq!(fs::read(dir.join("Engine/Content/big.bin")).unwrap(), big);
    if noise > 0 {
        assert_eq!(
            fs::read(dir.join("Engine/Content/noise.bin")).unwrap(),
            noise_bytes(noise)
        );
    }
}

#[tokio::test]
async fn streams_the_engine_without_a_zip_on_disk() {
    for root_folder in [None, Some("Linux_Unreal_Engine_5.8.3")] {
        let data = engine_zip_with(root_folder, 200_000, false);
        let size = data.len() as u64;
        let (base, seen) = cdn(data.clone(), Mode::Plain).await;
        let (install, tmp) = fresh(&base, size);
        let (events, on) = recorder();

        let done = install.download(&engine(&base, "a", size), &on).await;
        assert_eq!(done.unwrap(), Downloaded::Complete);
        // Le zip n'a jamais été écrit : seulement le moteur, son journal et son index.
        assert!(!install.zip_on_disk());
        let names: Vec<String> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["5.8.3"]);
        assert_eq!(install.downloaded(), index_of(&data));
        // L'index tient dans la première requête (les 64 derniers Ko), puis le zip arrive
        // d'un bloc depuis le début.
        assert_eq!(
            *seen.lock().unwrap(),
            [format!("Some({})", size - 65_557), "Some(0)".into()]
        );
        assert_eq!(
            *events.lock().unwrap().last().unwrap(),
            InstallProgress::Download {
                done: size,
                total: size
            }
        );
        assert!(
            !install.is_installed(),
            "pas fini tant que le journal est là"
        );

        // La fin : l'éditeur, la marque, et plus de journal ni d'index.
        let files = install.extract(&on).await.unwrap();
        assert_eq!(files, 10);
        check_engine(&install.dir, tmp.path(), 200_000);
        assert_eq!(
            fs::read_to_string(install.dir.join(MARKER)).unwrap(),
            "5.8.3"
        );
        assert!(!install.dir.join(stream::JOURNAL).exists());
        assert!(!install.dir.join(stream::INDEX).exists());
        assert!(install.is_installed());
    }
}

#[tokio::test]
async fn a_long_index_takes_a_second_request() {
    // 2 000 petits fichiers : l'index dépasse les 64 Ko de la première requête.
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = SimpleFileOptions::default().unix_permissions(0o755);
    zip.start_file(EDITOR, options).unwrap();
    zip.write_all(b"editor").unwrap();
    for i in 0..2000 {
        zip.start_file(format!("Engine/Content/Pack/Asset{i:04}.uasset"), options)
            .unwrap();
        zip.write_all(format!("asset {i}").as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let data = out.into_inner();
    let size = data.len() as u64;
    let start = index_of(&data);
    assert!(size - start > 65_557);
    let (base, seen) = cdn(data, Mode::Plain).await;
    let (install, _tmp) = fresh(&base, size);
    let (_, on) = recorder();
    let done = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(done.unwrap(), Downloaded::Complete);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            format!("Some({})", size - 65_557),
            format!("Some({start})"),
            "Some(0)".into()
        ]
    );
    assert_eq!(install.extract(&on).await.unwrap(), 2001);
    let asset = install.dir.join("Engine/Content/Pack/Asset1999.uasset");
    assert_eq!(fs::read_to_string(asset).unwrap(), "asset 1999");
}

#[tokio::test]
async fn a_cut_resumes_at_the_entry_it_stopped_in() {
    let data = engine_zip_with(None, 200_000, false);
    let size = data.len() as u64;
    let (noise_start, noise_data) = offsets(&data, "Engine/Content/noise.bin");
    // La connexion se coupe au milieu de noise.bin.
    let (base, seen) = cdn(data.clone(), Mode::CutOnceAt(noise_data + 50_000)).await;
    let (install, tmp) = fresh(&base, size);
    let (_, on) = recorder();

    let first = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(first.unwrap(), Downloaded::Interrupted);
    // Tout ce qui précède est écrit et enregistré ; noise.bin, à moitié reçu, est effacé.
    assert_eq!(install.downloaded(), noise_start as u64);
    assert!(install.dir.join("Engine/Content/big.bin").is_file());
    assert!(!install.dir.join("Engine/Content/noise.bin").exists());
    let journal = stream::read_journal(&install.dir).unwrap();
    assert_eq!(journal.offset, noise_start as u64);
    assert!(journal.written >= 1_000_000);

    // La reprise redemande le zip à partir de noise.bin, et l'index n'est pas redemandé.
    let second = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(second.unwrap(), Downloaded::Complete);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.last().unwrap(), &format!("Some({noise_start})"));
    assert_eq!(seen.len(), 3);
    install.extract(&on).await.unwrap();
    check_engine(&install.dir, tmp.path(), 200_000);
}

#[tokio::test]
async fn a_silent_stream_counts_as_interrupted() {
    let data = engine_zip_with(None, 200_000, false);
    let size = data.len() as u64;
    let (noise_start, noise_data) = offsets(&data, "Engine/Content/noise.bin");
    // La première connexion (l'index) se tait : interrompu, rien d'écrit.
    let (base, _) = cdn(data.clone(), Mode::StallAfter(100)).await;
    let (mut install, _tmp) = fresh(&base, size);
    install.http = reqwest::Client::builder()
        .read_timeout(std::time::Duration::from_millis(300))
        .build()
        .unwrap();
    let (_, on) = recorder();
    let first = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(first.unwrap(), Downloaded::Interrupted);
    assert!(!install.dir.join(stream::JOURNAL).exists());
    assert_eq!(install.downloaded(), 0);
    // Puis tout passe.
    let second = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(second.unwrap(), Downloaded::Complete);
    assert!(noise_data > noise_start);
}

#[tokio::test]
async fn expired_address_and_a_server_without_ranges() {
    let data = engine_zip_with(None, 200_000, false);
    let size = data.len() as u64;
    let (base, _) = cdn(data.clone(), Mode::Plain).await;
    let (install, _tmp) = fresh(&base, size);
    let (_, on) = recorder();
    // Une adresse expirée dès l'index : rien n'est écrit, on en redemande une.
    let expired = install.download(&engine(&base, "old", size), &on).await;
    assert_eq!(expired.unwrap(), Downloaded::Expired);
    assert!(!install.dir.join(stream::JOURNAL).exists());

    // Un serveur qui ignore Range : le zip passe par le disque, comme avant.
    let (base, _) = cdn(data.clone(), Mode::IgnoreRange).await;
    let (install, tmp) = fresh(&base, size);
    let done = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(done.unwrap(), Downloaded::Complete);
    assert!(install.zip_on_disk());
    assert_eq!(fs::read(&install.zip).unwrap(), data);
    assert_eq!(install.extract(&on).await.unwrap(), 10);
    check_engine(&install.dir, tmp.path(), 200_000);
    assert!(!install.zip_on_disk() && install.is_installed());
}

#[tokio::test]
async fn a_damaged_file_is_removed_and_downloaded_again() {
    let data = engine_zip_with(None, 200_000, false);
    let size = data.len() as u64;
    // Un octet de l'éditeur (stocké tel quel) arrive abîmé, une fois.
    let at = data.windows(4).position(|w| w == b"echo").unwrap();
    let (base, seen) = cdn(data.clone(), Mode::CorruptOnce(at)).await;
    let (install, tmp) = fresh(&base, size);
    let (_, on) = recorder();
    let err = install
        .download(&engine(&base, "a", size), &on)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("UnrealEditor") && err.contains("damaged"),
        "{err}"
    );
    assert!(
        !install.dir.join(EDITOR).exists(),
        "un fichier abîmé ne reste pas"
    );
    // La reprise repart de l'éditeur.
    let (editor_start, _) = offsets(&data, EDITOR);
    assert_eq!(install.downloaded(), editor_start as u64);
    let again = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(again.unwrap(), Downloaded::Complete);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &format!("Some({editor_start})")
    );
    install.extract(&on).await.unwrap();
    check_engine(&install.dir, tmp.path(), 200_000);
}

#[tokio::test]
async fn data_descriptors_and_zip64_records() {
    // Écrit sans revenir en arrière : les tailles et les CRC ne sont qu'après chaque
    // fichier, et noise.bin a ses tailles en ZIP64. Seul l'index les donne d'avance.
    let data = engine_zip_with(Some("Linux_Unreal_Engine_5.8.3"), 200_000, true);
    let size = data.len() as u64;
    let (base, _) = cdn(data, Mode::Plain).await;
    let (install, tmp) = fresh(&base, size);
    let (_, on) = recorder();
    let done = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(done.unwrap(), Downloaded::Complete);
    install.extract(&on).await.unwrap();
    check_engine(&install.dir, tmp.path(), 200_000);
}

#[tokio::test]
async fn a_crash_redoes_the_entries_after_the_last_checkpoint() {
    let data = engine_zip_with(None, 200_000, false);
    let size = data.len() as u64;
    let (base, seen) = cdn(data.clone(), Mode::Plain).await;
    let (install, tmp) = fresh(&base, size);
    let (_, on) = recorder();
    let done = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(done.unwrap(), Downloaded::Complete);

    // Un plantage après big.bin : le dernier point d'enregistrement était là, et ce qui
    // suit est à moitié écrit (ou pas du tout).
    let (big_start, _) = offsets(&data, "Engine/Content/big.bin");
    let mut journal = stream::read_journal(&install.dir).unwrap();
    let total = journal.files;
    journal.next = total - 3;
    journal.offset = big_start as u64;
    stream::write_journal(&install.dir, &journal).unwrap();
    fs::write(install.dir.join("Engine/Content/big.bin"), b"half").unwrap();
    fs::remove_file(install.dir.join("Engine/Content/noise.bin")).unwrap();
    assert!(!install.is_installed());
    assert_eq!(install.downloaded(), big_start as u64);

    let again = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(again.unwrap(), Downloaded::Complete);
    assert_eq!(
        seen.lock().unwrap().last().unwrap(),
        &format!("Some({big_start})")
    );
    assert_eq!(install.extract(&on).await.unwrap(), total);
    check_engine(&install.dir, tmp.path(), 200_000);
}

/// Le zip avec une entrée de l'index modifiée : `field` est le décalage du champ dans
/// l'en-tête central, `value` sa nouvelle valeur.
fn patch_central(data: &[u8], name: &str, field: usize, value: &[u8]) -> Vec<u8> {
    let start = index_of(data) as usize;
    let mut out = data.to_vec();
    let mut at = start;
    loop {
        assert_eq!(&out[at..at + 4], b"PK\x01\x02");
        let name_len = u16::from_le_bytes([out[at + 28], out[at + 29]]) as usize;
        let extra = u16::from_le_bytes([out[at + 30], out[at + 31]]) as usize;
        let comment = u16::from_le_bytes([out[at + 32], out[at + 33]]) as usize;
        if &out[at + 46..at + 46 + name_len] == name.as_bytes() {
            out[at + field..at + field + value.len()].copy_from_slice(value);
            return out;
        }
        at += 46 + name_len + extra + comment;
    }
}

#[test]
fn the_index_is_checked_before_anything_is_written() {
    let data = engine_zip(None);
    let size = data.len() as u64;
    let start = index_of(&data);
    let tail = |d: &[u8]| d[start as usize..].to_vec();

    // Lu depuis sa seule fin : toutes les entrées, et la place qu'elles prendront.
    let index = stream::parse(tail(&data), size, start).unwrap();
    assert_eq!(index.len(), 9);
    // Les fichiers (4 Ko au moins chacun) ; ni les liens, ni les dossiers.
    let blocks = |n: u64| n.div_ceil(4096) * 4096;
    let expected = blocks(22) + blocks(3) + blocks(84) + blocks(1_000_000);
    assert_eq!(index.remaining_space(0), expected);
    assert_eq!(index.remaining_space(9), 0);

    // Une compression que uehub ne sait pas lire (bzip2) : refusé d'avance.
    let bzip2 = patch_central(&data, "Engine/Content/big.bin", 10, &12u16.to_le_bytes());
    let err = stream::parse(tail(&bzip2), size, start)
        .err()
        .unwrap()
        .to_string();
    assert!(
        err.contains("Engine/Content/big.bin") && err.contains("unsupported"),
        "{err}"
    );

    // Deux entrées sur les mêmes octets : refusé aussi.
    let (editor_start, _) = offsets(&data, EDITOR);
    let overlap = patch_central(
        &data,
        "Engine/Build/Build.version",
        42,
        &(editor_start as u32).to_le_bytes(),
    );
    let err = stream::parse(tail(&overlap), size, start)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("overlap"), "{err}");
}

#[test]
fn the_index_is_read_without_the_data() {
    // 3 Mo de données et un petit index : le crate zip ne doit lire que l'index. Sinon,
    // pour le zip d'Epic, il lirait 40 Go de zéros avant le premier fichier.
    let data = engine_zip_with(None, 3 << 20, false);
    let size = data.len() as u64;
    let start = index_of(&data);
    stream::SPARSE_READ.with(|read| read.set(0));
    stream::parse(data[start as usize..].to_vec(), size, start).unwrap();
    let read = stream::SPARSE_READ.with(|read| read.get());
    assert!(read < 200_000, "{read} octets lus");
}

#[test]
fn a_zip64_index_is_found_from_the_end() {
    // Plus de 65 535 entrées : la fin de l'index passe en ZIP64.
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..70_000 {
        zip.start_file(format!("f{i}"), options).unwrap();
    }
    zip.finish().unwrap();
    let data = out.into_inner();
    let size = data.len() as u64;
    let start = index_of(&data);
    let from = size - 65_557;
    let probe = &data[from as usize..];
    assert!(probe.windows(4).any(|w| w == b"PK\x06\x06"), "un zip ZIP64");
    let found = stream::index_start(probe, from).unwrap();
    assert_eq!(found, start);
    let index = stream::parse(data[start as usize..].to_vec(), size, start).unwrap();
    assert_eq!(index.len(), 70_000);
}

#[tokio::test]
async fn folder_checks_with_a_journal() {
    let data = engine_zip_with(None, 200_000, false);
    let size = data.len() as u64;
    let (base, _) = cdn(data.clone(), Mode::CutOnceAt(data.len() / 2)).await;
    let (install, _tmp) = fresh(&base, size);
    let (_, on) = recorder();
    let first = install.download(&engine(&base, "a", size), &on).await;
    assert_eq!(first.unwrap(), Downloaded::Interrupted);
    // Une installation à nous, en cours (son journal est là) : acceptée pour reprendre,
    // pas encore installée, et le reste à écrire est compté.
    assert!(EngineInstall::check_dir("5.8.3", &install.dir).is_ok());
    assert!(!EngineInstall::is_installed_in("5.8.3", &install.dir));
    assert!(install.check_space().is_ok());
    // L'abandonner efface le dossier, sans zip à côté.
    EngineInstall::discard("5.8.3", &install.dir).await.unwrap();
    assert!(!install.dir.exists());
}
