use super::*;

fn project(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name).join(format!("{name}.uproject"));
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "{}").unwrap();
    p
}

#[test]
fn remember_orders_dedupes_and_persists() {
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    let file = d.join("data/sub/recent.json");
    let a = project(d, "A");
    let b = project(d, "B");
    let mut r = RecentProjects::load_from(file.clone());
    assert!(r.paths().is_empty());
    r.remember(&a).unwrap();
    r.remember(&b).unwrap();
    // Le même projet, par un chemin avec « .. » : une seule entrée, remise en tête.
    r.remember(&d.join("B/../A/A.uproject")).unwrap();
    let ca = a.canonicalize().unwrap();
    let cb = b.canonicalize().unwrap();
    assert_eq!(r.paths(), [ca.clone(), cb.clone()]);
    let again = RecentProjects::load_from(file.clone());
    assert_eq!(again.paths(), [ca, cb]);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("A.uproject"), "{text}");
}

#[test]
fn truncates_to_max_most_recent_first() {
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    let mut r = RecentProjects::load_from(d.join("recent.json"));
    let all: Vec<PathBuf> = (0..11).map(|i| project(d, &format!("P{i}"))).collect();
    for p in &all {
        r.remember(p).unwrap();
    }
    assert_eq!(r.paths().len(), MAX);
    assert_eq!(r.paths()[0], all[10].canonicalize().unwrap());
    assert_eq!(r.paths()[MAX - 1], all[3].canonicalize().unwrap());
}

#[test]
fn missing_projects_are_skipped_and_corrupt_file_is_empty() {
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    let file = d.join("recent.json");
    let a = project(d, "A");
    let b = project(d, "B");
    let mut r = RecentProjects::load_from(file.clone());
    r.remember(&a).unwrap();
    r.remember(&b).unwrap();
    std::fs::remove_file(&b).unwrap();
    let r = RecentProjects::load_from(file.clone());
    assert_eq!(r.paths(), [a.canonicalize().unwrap()]);

    std::fs::write(&file, "not json {").unwrap();
    assert!(RecentProjects::load_from(file).paths().is_empty());
}

#[test]
fn forget_removes_and_persists() {
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    let file = d.join("recent.json");
    let a = project(d, "A");
    let b = project(d, "B");
    let mut r = RecentProjects::load_from(file.clone());
    r.remember(&a).unwrap();
    r.remember(&b).unwrap();
    r.forget(&b.canonicalize().unwrap()).unwrap();
    assert_eq!(
        RecentProjects::load_from(file).paths(),
        [a.canonicalize().unwrap()]
    );
}

#[test]
fn remember_nonexistent_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    let d = d.path();
    let mut r = RecentProjects::load_from(d.join("recent.json"));
    assert!(matches!(
        r.remember(&d.join("nope.uproject")),
        Err(CoreError::Io(_))
    ));
    assert!(r.paths().is_empty());
}
