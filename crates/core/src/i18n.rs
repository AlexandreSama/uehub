//! Les textes de uehub, dans la langue du système.
//!
//! Ils sont rangés dans un fichier par langue, à la racine du dépôt : locales/en.json et
//! locales/fr.json. La CLI, l'app Tauri et l'interface React lisent les mêmes fichiers.
//! Une clé désigne un texte par son chemin : "errors.offline". Un texte peut contenir
//! des variables ({host}), et avoir deux formes, "one" et "other", choisies par {n}.

use std::fmt::Display;
use std::sync::OnceLock;

use serde_json::Value;

const EN: &str = include_str!("../../../locales/en.json");
const FR: &str = include_str!("../../../locales/fr.json");

/// Les langues de uehub. L'anglais sert aussi pour toutes celles qu'il ne parle pas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    /// "en" ou "fr", comme les fichiers de langue.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    /// La langue du système, comme la choisissent les programmes traduits avec gettext.
    pub fn from_system() -> Self {
        Self::from_env(|name| std::env::var(name).ok())
    }

    /// LANGUAGE (une liste : "fr:en"), puis LC_ALL, LC_MESSAGES et LANG : la première
    /// variable remplie décide. Une langue que uehub ne parle pas donne l'anglais.
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        for name in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Some(value) = var(name).filter(|v| !v.is_empty()) {
                return value
                    .split(':')
                    .find_map(Self::from_tag)
                    .unwrap_or(Lang::En);
            }
        }
        Lang::En
    }

    /// "fr_FR.UTF-8", "fr-CA", "fr" → le français. "C" et "POSIX" ne disent rien.
    fn from_tag(tag: &str) -> Option<Self> {
        match tag.split(['_', '-', '.', '@']).next()? {
            "fr" => Some(Lang::Fr),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

static LANG: OnceLock<Lang> = OnceLock::new();

/// Fixe la langue de tout le programme, une fois, au démarrage. Sans cet appel (dans les
/// tests par exemple), uehub parle anglais.
pub fn init(lang: Lang) {
    let _ = LANG.set(lang);
}

/// La langue choisie au démarrage.
pub fn lang() -> Lang {
    LANG.get().copied().unwrap_or(Lang::En)
}

/// Le fichier d'une langue, lu une seule fois.
fn table(lang: Lang) -> &'static Value {
    static TABLES: [OnceLock<Value>; 2] = [OnceLock::new(), OnceLock::new()];
    let (cell, text) = match lang {
        Lang::En => (&TABLES[0], EN),
        Lang::Fr => (&TABLES[1], FR),
    };
    cell.get_or_init(|| serde_json::from_str(text).expect("fichier de langue invalide"))
}

/// Le texte de `key` dans `lang`, avec ses variables remplacées. Une clé qui n'existe pas
/// donne la clé elle-même : l'oubli se voit à l'écran, sans arrêter le programme.
pub fn text(lang: Lang, key: &str, args: &[(&str, &dyn Display)]) -> String {
    let template = match key
        .split('.')
        .try_fold(table(lang), |node, part| node.get(part))
    {
        Some(Value::String(text)) => text.as_str(),
        Some(Value::Object(forms)) => forms
            .get(plural(lang, count(args)))
            .and_then(Value::as_str)
            .unwrap_or(key),
        _ => key,
    };
    fill(template, args)
}

/// La variable `n`, qui choisit entre singulier et pluriel.
fn count(args: &[(&str, &dyn Display)]) -> f64 {
    args.iter()
        .find(|(name, _)| *name == "n")
        .and_then(|(_, value)| value.to_string().parse().ok())
        .unwrap_or(0.0)
}

/// Le français met 0 et 1 au singulier ; l'anglais, seulement 1.
fn plural(lang: Lang, n: f64) -> &'static str {
    let one = match lang {
        Lang::Fr => n < 2.0,
        Lang::En => n == 1.0,
    };
    if one { "one" } else { "other" }
}

/// Remplace chaque {nom} connu par sa valeur, en une passe : une valeur qui contient
/// des accolades reste telle quelle. Les autres accolades ({bar:40} d'indicatif) aussi.
fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let value = after.find('}').and_then(|end| {
            let name = &after[..end];
            let (_, value) = args.iter().find(|(n, _)| *n == name)?;
            Some((value.to_string(), end))
        });
        match value {
            Some((value, end)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Une taille en octets, comme l'interface React l'écrit : "1,2 Go", "1.2 GB".
pub fn bytes(bytes: u64) -> String {
    bytes_in(lang(), bytes)
}

pub fn bytes_in(lang: Lang, bytes: u64) -> String {
    let (value, unit) = match bytes {
        b if b >= 1_000_000_000 => (b as f64 / 1e9, "units.gb"),
        b if b >= 1_000_000 => (b as f64 / 1e6, "units.mb"),
        b => (b as f64 / 1e3, "units.kb"),
    };
    let number = format!("{value:.1}");
    let number = number.strip_suffix(".0").unwrap_or(&number);
    let number = match lang {
        Lang::Fr => number.replace('.', ","),
        Lang::En => number.to_string(),
    };
    text(lang, unit, &[("value", &number)])
}

/// `t!("errors.offline", host = host)` : le texte de la clé dans la langue de uehub.
#[macro_export]
macro_rules! t {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::text(
            $crate::i18n::lang(),
            $key,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),*],
        )
    };
}

#[cfg(test)]
mod tests;
