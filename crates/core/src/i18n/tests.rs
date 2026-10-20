use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }
}

#[test]
fn the_system_language_comes_from_the_first_variable_set() {
    assert_eq!(Lang::from_env(env(&[("LANG", "fr_FR.UTF-8")])), Lang::Fr);
    assert_eq!(Lang::from_env(env(&[("LANG", "en_GB.UTF-8")])), Lang::En);
    // Une langue que uehub ne parle pas : l'anglais.
    assert_eq!(Lang::from_env(env(&[("LANG", "de_DE.UTF-8")])), Lang::En);
    assert_eq!(Lang::from_env(env(&[("LANG", "C.UTF-8")])), Lang::En);
    assert_eq!(Lang::from_env(env(&[])), Lang::En);
    // LC_ALL passe avant LANG, LC_MESSAGES aussi.
    let both = [("LC_ALL", "en_US.UTF-8"), ("LANG", "fr_FR.UTF-8")];
    assert_eq!(Lang::from_env(env(&both)), Lang::En);
    let messages = [("LC_MESSAGES", "fr_CA.UTF-8"), ("LANG", "en_US.UTF-8")];
    assert_eq!(Lang::from_env(env(&messages)), Lang::Fr);
    // LANGUAGE est une liste : la première langue connue gagne.
    let list = [("LANGUAGE", "de:fr:en"), ("LANG", "en_US.UTF-8")];
    assert_eq!(Lang::from_env(env(&list)), Lang::Fr);
    // Une variable vide ne compte pas.
    let empty = [("LANGUAGE", ""), ("LANG", "fr_FR.UTF-8")];
    assert_eq!(Lang::from_env(env(&empty)), Lang::Fr);
}

#[test]
fn texts_fill_their_variables_and_pick_the_plural() {
    let n = 1;
    let first = "Content/A.uasset";
    let one = text(Lang::Fr, "errors.conflict", &[("n", &n), ("first", &first)]);
    assert!(one.starts_with("1 fichier existe déjà"), "{one}");
    assert!(one.contains("Content/A.uasset"), "{one}");
    // Le français met 0 au singulier, l'anglais au pluriel.
    let zero = 0;
    let fr = text(
        Lang::Fr,
        "errors.conflict",
        &[("n", &zero), ("first", &first)],
    );
    let en = text(
        Lang::En,
        "errors.conflict",
        &[("n", &zero), ("first", &first)],
    );
    assert!(fr.starts_with("0 fichier existe"), "{fr}");
    assert!(en.starts_with("0 files already exist"), "{en}");
    // Une valeur qui contient des accolades n'est pas remplacée à son tour.
    let host = "{status}";
    let status = 503;
    let tricky = text(
        Lang::En,
        "errors.http_status",
        &[("status", &status), ("host", &host)],
    );
    assert_eq!(tricky, "Error 503 returned by {status}.");
    // Une clé absente se voit, sans planter.
    assert_eq!(text(Lang::Fr, "nope.missing", &[]), "nope.missing");
}

#[test]
fn the_macro_speaks_english_until_init() {
    // Les tests n'appellent pas init : tout est en anglais, quelle que soit la machine.
    assert_eq!(lang(), Lang::En);
    let host = "www.fab.com";
    assert_eq!(
        crate::t!("errors.offline", host = host),
        "No connection: can't reach www.fab.com. Check your Internet access."
    );
}

#[test]
fn sizes_are_written_like_the_interface_does() {
    assert_eq!(bytes_in(Lang::Fr, 1_234_000_000), "1,2 Go");
    assert_eq!(bytes_in(Lang::Fr, 1_000_000_000), "1 Go");
    assert_eq!(bytes_in(Lang::En, 39_818_576_005), "39.8 GB");
    assert_eq!(bytes_in(Lang::En, 18_400_000), "18.4 MB");
    assert_eq!(bytes_in(Lang::Fr, 2_000), "2 Ko");
}

// ---------------------------------------------------------------------------
// Les deux fichiers de langue
// ---------------------------------------------------------------------------

/// Chaque texte, par clé : "errors.offline" → ses variables.
fn leaves(node: &Value, prefix: &str, out: &mut BTreeMap<String, BTreeSet<String>>) {
    let Value::Object(map) = node else { return };
    let plural = map.contains_key("other");
    if plural {
        let names = map.values().filter_map(Value::as_str).flat_map(variables);
        out.insert(prefix.to_string(), names.collect());
        return;
    }
    for (key, value) in map {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::String(text) => {
                out.insert(path, variables(text).collect());
            }
            _ => leaves(value, &path, out),
        }
    }
}

/// Les {nom} d'un texte. Les accolades d'indicatif ({bar:40}) n'en sont pas.
fn variables(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split('{').skip(1).filter_map(|rest| {
        let name = &rest[..rest.find('}')?];
        let simple =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        simple.then(|| name.to_string())
    })
}

fn keys(lang: Lang) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    leaves(table(lang), "", &mut out);
    out
}

#[test]
fn both_languages_have_the_same_texts_and_variables() {
    let (en, fr) = (keys(Lang::En), keys(Lang::Fr));
    let only_en: Vec<_> = en.keys().filter(|k| !fr.contains_key(*k)).collect();
    let only_fr: Vec<_> = fr.keys().filter(|k| !en.contains_key(*k)).collect();
    assert!(only_en.is_empty(), "absentes de fr.json : {only_en:?}");
    assert!(only_fr.is_empty(), "absentes de en.json : {only_fr:?}");
    for (key, names) in &en {
        assert_eq!(names, &fr[key], "variables différentes pour {key}");
    }
}

/// Les fichiers de code du dépôt : Rust, et TypeScript de l'interface.
fn sources() -> Vec<(PathBuf, String)> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    let mut dirs = vec![
        repo.join("crates"),
        repo.join("desktop/src"),
        repo.join("desktop/src-tauri/src"),
    ];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy();
            if path.is_dir() {
                if name != "target" && name != "node_modules" {
                    dirs.push(path);
                }
            } else if [".rs", ".ts", ".tsx"].iter().any(|ext| name.ends_with(ext)) {
                let text = std::fs::read_to_string(&path).unwrap();
                found.push((path, text));
            }
        }
    }
    found
}

#[test]
fn every_key_used_exists_and_every_text_is_used() {
    let en = keys(Lang::En);
    let sources = sources();
    // Les clés que cite la macro t! dans le code Rust (format! ne compte pas).
    for (path, text) in &sources {
        for (at, _) in text.match_indices("t!(\"") {
            let before = text[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = &text[at + 4..];
            let key = &rest[..rest.find('"').unwrap()];
            assert!(en.contains_key(key), "{key} manque ({})", path.display());
        }
    }
    // Chaque texte est cité quelque part, entre guillemets, hors des tests : pas de
    // texte oublié.
    let code: Vec<&String> = sources
        .iter()
        .filter(|(path, _)| {
            let name = path.to_string_lossy();
            !name.ends_with("tests.rs") && !name.contains(".test.") && !name.contains("/tests/")
        })
        .map(|(_, text)| text)
        .collect();
    let unused: Vec<_> = en
        .keys()
        .filter(|key| {
            let quoted = format!("\"{key}\"");
            !code.iter().any(|text| text.contains(&quoted))
        })
        .collect();
    assert!(unused.is_empty(), "textes jamais utilisés : {unused:?}");
}
