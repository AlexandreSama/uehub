use super::*;
use serde_json::{Value, json};
use std::path::Path;

fn download(asset: &str, folder: &str) -> Request {
    Request::Download {
        asset_id: asset.into(),
        folder: folder.into(),
    }
}

fn add_to(asset: &str, uproject: &str) -> Request {
    Request::AddToProject {
        asset_id: asset.into(),
        uproject: uproject.into(),
        overwrite: false,
    }
}

fn engine(version: &str, folder: &str) -> Request {
    Request::InstallEngine {
        version: version.into(),
        folder: folder.into(),
    }
}

fn job(request: Request) -> Job {
    Job {
        id: 0,
        request,
        title: "Megascans: Grass".into(),
        thumbnail: None,
        target: "ForestShooter".into(),
        status: Status::Failed {
            error: "ignored by add".into(),
        },
        artifact_id: None,
        finished_at: None,
        progress: None,
    }
}

fn summary() -> Summary {
    serde_json::from_value(json!({"files": 61, "bytes": 1234, "dir": "/p", "note": null})).unwrap()
}

fn states(jobs: &Jobs) -> Vec<(u64, String)> {
    jobs.list
        .iter()
        .map(|j| {
            let v = serde_json::to_value(&j.status).unwrap();
            (j.id, v["state"].as_str().unwrap().to_string())
        })
        .collect()
}

#[test]
fn json_contract_matches_typescript() {
    let mut j = job(add_to("abc", "/p/P.uproject"));
    j.id = 7;
    j.status = Status::Running;
    j.artifact_id = Some("art".into());
    j.progress = Some(ProgressEvent::Chunks {
        done: 3,
        total: 40,
        bytes: 3_145_728,
    });
    let v = serde_json::to_value(&j).unwrap();
    assert_eq!(
        v["request"],
        json!({"kind": "addToProject", "assetId": "abc", "uproject": "/p/P.uproject", "overwrite": false})
    );
    assert_eq!(v["status"], json!({"state": "running"}));
    assert_eq!(v["artifactId"], json!("art"));
    assert_eq!(v["finishedAt"], json!(null));
    assert_eq!(
        v["progress"],
        json!({"phase": "chunks", "done": 3, "total": 40, "bytes": 3_145_728})
    );
    assert_eq!(v["id"], json!(7));
    for key in ["title", "thumbnail", "target"] {
        assert!(v.get(key).is_some(), "{key}");
    }

    j.status = Status::Done { summary: summary() };
    let v = serde_json::to_value(&j).unwrap();
    assert_eq!(v["status"]["state"], json!("done"));
    assert_eq!(v["status"]["summary"]["files"], json!(61));
    j.status = Status::Failed {
        error: "boom".into(),
    };
    assert_eq!(
        serde_json::to_value(&j).unwrap()["status"],
        json!({"state": "failed", "error": "boom"})
    );

    // Ce que l'interface envoie à `enqueue`
    let r: Request =
        serde_json::from_value(json!({"kind": "download", "assetId": "a", "folder": "/tmp/x"}))
            .unwrap();
    assert!(r == download("a", "/tmp/x"));
    let r: Request = serde_json::from_value(
        json!({"kind": "addToProject", "assetId": "a", "uproject": "/p.uproject", "overwrite": true}),
    )
    .unwrap();
    assert!(matches!(
        r,
        Request::AddToProject {
            overwrite: true,
            ..
        }
    ));
    let r: Request = serde_json::from_value(
        json!({"kind": "installEngine", "version": "5.8.3", "folder": "/home/a/UnrealEngine"}),
    )
    .unwrap();
    assert!(r == engine("5.8.3", "/home/a/UnrealEngine"));
    assert_eq!(
        serde_json::to_value(ProgressEvent::from(uehub_core::InstallProgress::Download {
            done: 5,
            total: 39_818_576_005
        }))
        .unwrap(),
        json!({"phase": "bytes", "done": 5, "total": 39_818_576_005u64})
    );
    assert_eq!(
        serde_json::to_value(ProgressEvent::from(uehub_core::InstallProgress::Extract {
            done: 200,
            total: 3206
        }))
        .unwrap(),
        json!({"phase": "files", "done": 200, "total": 3206})
    );

    // Aller-retour par le fichier : l'avancement n'est jamais relu
    j.progress = Some(ProgressEvent::Files { done: 1, total: 2 });
    let text = serde_json::to_string(&vec![j.clone()]).unwrap();
    let back: Vec<Job> = serde_json::from_str(&text).unwrap();
    assert!(back[0].progress.is_none());
    assert_eq!(back[0].artifact_id.as_deref(), Some("art"));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v[0]["status"]["state"], json!("failed"));
}

#[test]
fn restore_pauses_unfinished_and_continues_ids() {
    let mut list = Vec::new();
    for (id, status) in [
        (3, Status::Waiting),
        (5, Status::Running),
        (8, Status::Done { summary: summary() }),
        (9, Status::Failed { error: "x".into() }),
        (4, Status::Paused),
    ] {
        let mut j = job(download("a", &format!("/f{id}")));
        j.id = id;
        j.status = status;
        j.progress = Some(ProgressEvent::Chunks {
            done: 1,
            total: 2,
            bytes: 10,
        });
        list.push(j);
    }
    let mut jobs = Jobs::restore(list);
    assert_eq!(
        states(&jobs),
        [
            (3, "paused"),
            (5, "paused"),
            (8, "done"),
            (9, "failed"),
            (4, "paused")
        ]
        .map(|(i, s)| (i, s.to_string()))
    );
    assert!(jobs.list.iter().all(|j| j.progress.is_none()));
    assert!(
        jobs.start_next().is_none(),
        "rien ne repart tout seul après un redémarrage"
    );
    jobs.add(job(download("b", "/new"))).unwrap();
    assert_eq!(jobs.list.last().unwrap().id, 10);
    assert_eq!(Jobs::restore(Vec::new()).next_id, 1);
}

#[test]
fn add_assigns_ids_and_refuses_duplicates_until_done() {
    let mut jobs = Jobs::restore(Vec::new());
    jobs.add(job(add_to("a", "/p/P.uproject"))).unwrap();
    jobs.add(job(download("a", "/f"))).unwrap(); // même asset, autre destination : d'accord
    let err = jobs.add(job(add_to("a", "/p/P.uproject"))).unwrap_err();
    assert!(err.contains("déjà dans la file"), "{err}");
    assert_eq!(
        states(&jobs),
        [(1, "waiting".into()), (2, "waiting".into())]
    );

    // Un doublon en échec ou en pause bloque encore ; un doublon terminé, non.
    jobs.finish(1, Status::Failed { error: "x".into() });
    assert!(jobs.add(job(add_to("a", "/p/P.uproject"))).is_err());
    jobs.finish(1, Status::Done { summary: summary() });
    jobs.add(job(add_to("a", "/p/P.uproject"))).unwrap();
    assert_eq!(jobs.list.last().unwrap().id, 3);
}

#[test]
fn worker_takes_jobs_in_order_and_pause_resume_work() {
    let mut jobs = Jobs::restore(Vec::new());
    for i in 0..3 {
        jobs.add(job(download("a", &format!("/f{i}")))).unwrap();
    }
    let first = jobs.start_next().unwrap();
    assert_eq!(first.id, 1);
    assert!(matches!(first.status, Status::Running));

    // On met en pause le job en cours et le suivant : le worker passe au 3.
    jobs.get_mut(1).unwrap().progress = Some(ProgressEvent::Chunks {
        done: 1,
        total: 9,
        bytes: 10,
    });
    jobs.pause(1);
    jobs.pause(2);
    assert!(jobs.get_mut(1).unwrap().progress.is_none());
    assert_eq!(jobs.start_next().unwrap().id, 3);
    assert!(jobs.start_next().is_none());

    // resume remet le 2 dans la file ; sur un job en cours ou terminé, il ne fait rien.
    jobs.resume(2);
    jobs.resume(3);
    jobs.finish(3, Status::Done { summary: summary() });
    jobs.resume(3);
    assert_eq!(
        states(&jobs),
        [
            (1, "paused".into()),
            (2, "waiting".into()),
            (3, "done".into())
        ]
    );
    assert_eq!(jobs.start_next().unwrap().id, 2);
    jobs.finish(
        2,
        Status::Failed {
            error: "réseau".into(),
        },
    );

    // Un job en échec revient avec resume (« Réessayer »), pas avec resume_all.
    jobs.resume_all();
    assert_eq!(
        states(&jobs),
        [
            (1, "waiting".into()),
            (2, "failed".into()),
            (3, "done".into())
        ]
    );
    jobs.resume(2);
    assert_eq!(states(&jobs)[1].1, "waiting");

    jobs.pause_all();
    assert_eq!(
        states(&jobs),
        [
            (1, "paused".into()),
            (2, "paused".into()),
            (3, "done".into())
        ]
    );
}

#[test]
fn remove_clear_and_shared_caches() {
    let mut jobs = Jobs::restore(Vec::new());
    jobs.add(job(add_to("a", "/p/P.uproject"))).unwrap();
    jobs.add(job(download("a", "/f"))).unwrap();
    jobs.add(job(download("b", "/f"))).unwrap();
    for (id, art) in [(1, "artA"), (2, "artA"), (3, "artB")] {
        jobs.get_mut(id).unwrap().artifact_id = Some(art.into());
    }
    let removed = jobs.remove(1).unwrap();
    assert_eq!(removed.artifact_id.as_deref(), Some("artA"));
    assert!(jobs.remove(1).is_none());
    assert!(
        jobs.still_needs(&Leftover::Chunks("artA".into())),
        "le job 2 a encore besoin d'artA"
    );
    jobs.finish(2, Status::Done { summary: summary() });
    assert!(jobs.get_mut(2).unwrap().finished_at.unwrap() > 1_700_000_000_000);
    assert!(jobs.get_mut(3).unwrap().finished_at.is_none());
    assert!(
        !jobs.still_needs(&Leftover::Chunks("artA".into())),
        "un job terminé n'a plus besoin de son cache"
    );
    jobs.clear_finished();
    assert_eq!(jobs.list.iter().map(|j| j.id).collect::<Vec<_>>(), [3]);
    assert!(jobs.still_needs(&Leftover::Chunks("artB".into())));
}

#[test]
fn engine_leftovers() {
    let mut jobs = Jobs::restore(Vec::new());
    jobs.add(job(engine("5.8.3", "/home/a/UE"))).unwrap();
    assert!(
        jobs.add(job(engine("5.8.3", "/home/a/UE"))).is_err(),
        "le même moteur deux fois"
    );
    jobs.add(job(engine("5.7.4", "/home/a/UE"))).unwrap();
    let removed = jobs.remove(1).unwrap();
    let leftover = Leftover::of(removed).unwrap();
    assert!(
        matches!(&leftover, Leftover::Engine { version, dir } if version == "5.8.3" && dir == Path::new("/home/a/UE/5.8.3"))
    );
    assert!(!jobs.still_needs(&leftover), "il ne reste que la 5.7.4");
    // Remis dans la file avant que la corbeille soit vidée : son zip doit rester.
    jobs.add(job(engine("5.8.3", "/home/a/UE"))).unwrap();
    assert!(jobs.still_needs(&leftover));
    assert!(!jobs.still_needs(&Leftover::Chunks("art".into())));
    // Un job d'asset sans version connue ne laisse rien.
    jobs.add(job(download("a", "/f"))).unwrap();
    let id = jobs.list.last().unwrap().id;
    assert!(Leftover::of(jobs.remove(id).unwrap()).is_none());
}
