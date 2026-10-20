use super::*;
use std::io::Write;
use std::sync::Mutex;

use tokio::io::AsyncWriteExt;
use zip::write::SimpleFileOptions;

use crate::test_support::{dead_address, read_request};

/// Un petit zip de moteur : l'éditeur (0755), Build.version, un fichier compressé de
/// 1 Mo, deux liens symboliques, et des entrées à ignorer. `root` range tout dans un
/// dossier de tête, comme certains zips d'Epic.
fn engine_zip(root: Option<&str>) -> Vec<u8> {
    let p = |name: &str| match root {
        Some(r) => format!("{r}/{name}"),
        None => name.to_string(),
    };
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
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
    zip.start_file("../evil.txt", stored).unwrap();
    zip.write_all(b"no").unwrap();
    zip.finish().unwrap();
    out.into_inner()
}

/// Comment le faux CDN se comporte avec la première connexion.
#[derive(Clone, Copy)]
enum Mode {
    /// Coupe après tant d'octets
    CutFirstAt(usize),
    /// Ignore l'en-tête Range et renvoie tout le fichier
    IgnoreRange,
    /// Envoie tant d'octets, puis se tait sans fermer
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
    assert!(err.contains("extraction interrompue"), "{err}");
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
    assert!(
        err.starts_with("téléchargement : pas assez de place : il faut environ"),
        "{err}"
    );
    assert!(err.contains("dans /tmp"), "{err}");
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
    assert!(STALLED >= std::time::Duration::from_secs(30));
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
        err.contains("mine existe déjà et ne contient pas ce moteur"),
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
    assert!(err.contains("n'a pas été installé par uehub"), "{err}");
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
