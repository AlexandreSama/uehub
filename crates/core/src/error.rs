//! Les erreurs du core. Chacune s'affiche en une phrase, dans la langue de uehub : c'est
//! ce que voit l'interface. `detail()` y ajoute la cause technique (la réponse d'Epic,
//! l'erreur du système) : c'est ce qu'on écrit dans le journal.

use std::error::Error as StdError;
use std::fmt;

use egs_api::api::error::EpicAPIError;

use crate::t;

#[derive(Debug)]
pub enum CoreError {
    /// Epic refuse le code de connexion
    AuthFailed,
    /// Pas de session, ou elle a expiré
    NotLoggedIn,
    /// Une étape chez Epic a échoué : laquelle, et le statut HTTP s'il y en a un. La
    /// réponse d'Epic ne va que dans le journal.
    Epic {
        step: String,
        status: Option<String>,
        detail: String,
    },
    /// Le réseau : pas de connexion, délai dépassé, réponse coupée…
    Net(NetError),
    NoDataDir,
    // Les quatre suivantes portent un message déjà écrit dans la langue de uehub.
    Api(String),
    Download(String),
    Project(String),
    Engine(String),
    HashMismatch(String),
    /// Le nombre de fichiers déjà présents dans le projet, et le premier d'entre eux
    Conflict(usize, String),
    Keyring(keyring_core::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let text = match self {
            Self::AuthFailed => t!("errors.auth_failed"),
            Self::NotLoggedIn => t!("errors.not_logged_in"),
            Self::Epic {
                step,
                status: Some(status),
                ..
            } => t!("errors.epic_status", step = step, status = status),
            Self::Epic { step, .. } => t!("errors.epic", step = step),
            Self::Net(e) => e.to_string(),
            Self::NoDataDir => t!("errors.no_data_dir"),
            Self::Api(text) | Self::Download(text) | Self::Project(text) | Self::Engine(text) => {
                text.clone()
            }
            Self::HashMismatch(file) => t!("errors.hash_mismatch", file = file),
            Self::Conflict(n, first) => t!("errors.conflict", n = n, first = first),
            Self::Keyring(e) => t!("errors.keyring", detail = e),
            Self::Io(e) => e.to_string(),
            Self::Json(e) => e.to_string(),
        };
        f.write_str(&text)
    }
}

impl StdError for CoreError {
    /// La cause qui suit ce que la phrase dit déjà.
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Net(e) => e.source(),
            Self::Keyring(e) => e.source(),
            Self::Io(e) => e.source(),
            Self::Json(e) => e.source(),
            _ => None,
        }
    }
}

impl CoreError {
    /// La phrase, puis tout ce qui l'explique : pour le journal, jamais pour l'interface.
    pub fn detail(&self) -> String {
        match self {
            Self::Epic { detail, .. } => format!("{self} [{detail}]"),
            Self::Net(e) => e.detail(),
            _ => causes(self),
        }
    }

    /// Une erreur d'egs-api, pendant l'étape `step` (déjà traduite).
    pub(crate) fn epic(step: String, e: EpicAPIError) -> Self {
        match e {
            EpicAPIError::NetworkError(e) => Self::Net(e.into()),
            EpicAPIError::InvalidCredentials => Self::NotLoggedIn,
            EpicAPIError::HttpError { status, body } => Self::Epic {
                step,
                status: Some(status.to_string()),
                detail: body.chars().take(300).collect(),
            },
            other => Self::Epic {
                step,
                status: None,
                detail: other.to_string(),
            },
        }
    }
}

/// Une erreur et ses causes, sur une ligne : "error sending request: client error
/// (Connect): tcp connect error: Connection refused (os error 111)".
pub fn causes(e: &dyn StdError) -> String {
    let mut text = e.to_string();
    let mut next = e.source();
    while let Some(cause) = next {
        text = format!("{text}: {cause}");
        next = cause.source();
    }
    text
}

// ---------------------------------------------------------------------------
// Le réseau
// ---------------------------------------------------------------------------

/// Ce qu'on peut dire d'une erreur réseau, sans le détail technique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetKind {
    /// Pas de connexion : pas de réseau, nom inconnu, serveur qui refuse
    Offline,
    /// Le serveur n'a pas répondu à temps
    Timeout,
    /// Le serveur a répondu par une erreur
    Status(u16),
    /// Une réponse qu'on ne sait pas lire
    Unreadable,
    /// La connexion s'est coupée en route
    Interrupted,
}

/// Une erreur de reqwest ou de wreq, rangée par type. Son adresse est retirée : elle peut
/// contenir un code de session ou une signature de téléchargement. Seul le nom du site reste.
#[derive(Debug)]
pub struct NetError {
    pub kind: NetKind,
    /// "www.unrealengine.com", sans chemin ni paramètres
    pub host: Option<String>,
    source: Box<dyn StdError + Send + Sync>,
}

impl From<reqwest::Error> for NetError {
    fn from(e: reqwest::Error) -> Self {
        let kind = if e.is_timeout() {
            NetKind::Timeout
        } else if e.is_connect() {
            NetKind::Offline
        } else if let Some(status) = e.status() {
            NetKind::Status(status.as_u16())
        } else if e.is_decode() {
            NetKind::Unreadable
        } else {
            NetKind::Interrupted
        };
        Self {
            kind,
            host: e.url().and_then(|url| url.host_str()).map(String::from),
            source: Box::new(e.without_url()),
        }
    }
}

/// La même chose pour wreq, le client de la session unrealengine.com.
impl From<wreq::Error> for NetError {
    fn from(e: wreq::Error) -> Self {
        let kind = if e.is_timeout() {
            NetKind::Timeout
        } else if e.is_connect() {
            NetKind::Offline
        } else if let Some(status) = e.status() {
            NetKind::Status(status.as_u16())
        } else if e.is_decode() {
            NetKind::Unreadable
        } else {
            NetKind::Interrupted
        };
        Self {
            kind,
            host: e.uri().and_then(|uri| uri.host()).map(String::from),
            source: Box::new(e.without_uri()),
        }
    }
}

impl From<reqwest::Error> for CoreError {
    fn from(e: reqwest::Error) -> Self {
        Self::Net(e.into())
    }
}

impl From<wreq::Error> for CoreError {
    fn from(e: wreq::Error) -> Self {
        Self::Net(e.into())
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let host = self.host.clone().unwrap_or_else(|| t!("errors.the_server"));
        let text = match self.kind {
            NetKind::Offline => t!("errors.offline", host = host),
            NetKind::Timeout => t!("errors.timeout", host = host),
            NetKind::Status(status) => t!("errors.http_status", status = status, host = host),
            NetKind::Unreadable => t!("errors.unreadable", host = host),
            NetKind::Interrupted => t!("errors.interrupted", host = host),
        };
        f.write_str(&text)
    }
}

impl StdError for NetError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref())
    }
}

impl NetError {
    /// Pour le journal : le type, le site, et l'erreur du client HTTP avec ses causes.
    pub fn detail(&self) -> String {
        let host = self.host.as_deref().unwrap_or("?");
        format!("{:?} {host}: {}", self.kind, causes(self.source.as_ref()))
    }
}

impl From<keyring_core::Error> for CoreError {
    fn from(e: keyring_core::Error) -> Self {
        Self::Keyring(e)
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[cfg(test)]
mod tests;
