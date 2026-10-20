//! Le journal : ce que uehub fait et ce qui casse, dans ~/.local/state/uehub/uehub.log.
//!
//! On le joint à un rapport de bug. Chaque ligne passe donc par `scrub` avant d'être
//! écrite : jetons, cookies, codes et adresses signées sont masqués, et le dossier
//! personnel devient « ~ ». Le fichier ne garde qu'environ 2 Mo : au-delà d'1 Mo, il
//! devient uehub.log.1 (l'ancien est effacé) et un nouveau commence.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::{BaseDirs, ProjectDirs};
use log::{Level, LevelFilter, Log, Metadata, Record};
use regex::{Captures, Regex};

/// Au-delà, le fichier passe en uehub.log.1.
const MAX_BYTES: u64 = 1_000_000;

/// Ce qui remplace un secret.
const HIDDEN: &str = "[hidden]";

/// ~/.local/state/uehub/uehub.log : le dossier que la norme XDG réserve aux journaux.
pub fn path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "uehub")?;
    Some(dirs.state_dir()?.join("uehub.log"))
}

/// Branche le journal sur la crate `log` : les messages de uehub à partir d'`info`, ceux
/// des bibliothèques (reqwest, Tauri…) à partir de `warn`. Renvoie le chemin du fichier,
/// ou `None` si le dossier ne s'écrit pas : uehub tourne alors sans journal.
pub fn start() -> Option<PathBuf> {
    let path = path()?;
    let home = BaseDirs::new().map(|d| d.home_dir().display().to_string());
    let journal = Journal::open(&path, home).ok()?;
    // Le journal vit jusqu'à la fin du programme : on le « fuit » volontairement.
    log::set_logger(Box::leak(Box::new(journal))).ok()?;
    log::set_max_level(LevelFilter::Info);
    // Un panic finit dans le journal, avec l'endroit du code où il a eu lieu.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!(target: "uehub::panic", "{info}");
        default(info);
    }));
    Some(path)
}

struct Journal {
    path: PathBuf,
    home: Option<String>,
    /// Le fichier ouvert, et sa taille
    file: Mutex<(File, u64)>,
}

impl Journal {
    fn open(path: &Path, home: Option<String>) -> std::io::Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = append(path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path: path.to_path_buf(),
            home,
            file: Mutex::new((file, size)),
        })
    }

    /// Écrit une ligne déjà masquée, après avoir changé de fichier s'il est trop gros.
    fn write(&self, line: &str) -> std::io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(|e| e.into_inner());
        let (file, size) = &mut *guard;
        if *size > MAX_BYTES {
            let mut old = self.path.clone().into_os_string();
            old.push(".1");
            fs::rename(&self.path, old)?;
            *file = append(&self.path)?;
            *size = 0;
        }
        file.write_all(line.as_bytes())?;
        *size += line.len() as u64;
        Ok(())
    }
}

/// Le fichier en ajout, lisible par toi seul.
fn append(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

impl Log for Journal {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let ours = metadata.target().starts_with("uehub");
        metadata.level() <= if ours { Level::Info } else { Level::Warn }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {}: {}\n",
            utc_now(),
            record.level(),
            record.target(),
            record.args()
        );
        // Le journal ne doit jamais faire échouer ce qu'il raconte.
        let _ = self.write(&scrub(&line, self.home.as_deref()));
    }

    fn flush(&self) {}
}

// ---------------------------------------------------------------------------
// Le masquage
// ---------------------------------------------------------------------------

/// Une adresse web : on garde le site et le chemin, pas les paramètres.
static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s"'<>?#]+[?#][^\s"'<>)]*"#).unwrap());

/// Un en-tête Cookie, Set-Cookie ou Authorization : tout le reste de la ligne.
static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(set-cookie|cookie|authorization)(\s*:\s*)[^\r\n]+").unwrap()
});

/// Un nom qui annonce un secret : jeton, code, SID, cookie de session, signature…
const SECRET: &str = r"[\w-]*(?:token|code|sid|sso|session|auth|bearer|password|secret|signature|policy|key-pair-id|credential)[\w-]*";

/// `nom=valeur`, comme dans un cookie ou des paramètres.
static PAIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"(?i)\b({SECRET})(=)([^\s"'&;,]+)"#)).unwrap());

/// `"nom":"valeur"` (JSON) ou `nom: "valeur"` (Debug de Rust). La valeur entre
/// guillemets : une phrase comme « session: Epic a répondu » reste lisible.
static QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r#"(?i)\b({SECRET})("?\s*:\s*")([^"]*)"#)).unwrap());

/// Masque la valeur d'une paire, sauf les codes d'erreur d'Epic : « errorCode » en
/// contient, et ils servent à comprendre la panne.
fn hide_value(c: &Captures) -> String {
    let name = &c[1];
    if name.to_ascii_lowercase().ends_with("errorcode") {
        return c[0].to_string();
    }
    format!("{name}{}{HIDDEN}", &c[2])
}

/// « Bearer xxx » seul, hors d'un en-tête.
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bbearer\s+[\w.~+/=-]+").unwrap());

/// Un JWT (les jetons d'Epic en sont : « eg1~eyJ… »).
static JWT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"eyJ[\w-]+\.[\w-]+(?:\.[\w-]*)?").unwrap());

/// 32 caractères hexadécimaux ou plus : la forme des codes d'échange et des SID d'Epic.
static HEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[0-9a-fA-F]{32,}\b").unwrap());

/// Masque ce qui pourrait servir à quelqu'un d'autre, et remplace le dossier personnel
/// par « ~ ».
pub fn scrub(line: &str, home: Option<&str>) -> String {
    let line = URL.replace_all(line, |c: &Captures| {
        let url = &c[0];
        let end = url.find(['?', '#']).unwrap_or(url.len());
        format!("{}?{HIDDEN}", &url[..end])
    });
    let line = HEADER.replace_all(&line, format!("${{1}}${{2}}{HIDDEN}"));
    let line = PAIR.replace_all(&line, hide_value);
    let line = QUOTED.replace_all(&line, hide_value);
    let line = BEARER.replace_all(&line, format!("Bearer {HIDDEN}"));
    let line = JWT.replace_all(&line, HIDDEN);
    let line = HEX.replace_all(&line, HIDDEN);
    match home {
        Some(home) if home.len() > 1 => line.replace(home, "~"),
        _ => line.into_owned(),
    }
}

// ---------------------------------------------------------------------------
// L'heure, en UTC, sans dépendance de plus
// ---------------------------------------------------------------------------

fn utc_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    utc(seconds)
}

/// 1792440000 → "2026-10-19 20:00:00Z"
fn utc(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// Le jour du calendrier à partir du nombre de jours depuis 1970 (algorithme de
/// Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests;
