use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("connexion refusée par Epic (code expiré ou invalide ?)")]
    AuthFailed,
    #[error("pas de session valide : connecte-toi d'abord")]
    NotLoggedIn,
    #[error("erreur de l'API Epic : {0}")]
    Api(String),
    #[error("impossible de trouver le dossier de données utilisateur")]
    NoDataDir,
    #[error("téléchargement : {0}")]
    Download(String),
    #[error("fichier corrompu (SHA-1 différent) : {0}")]
    HashMismatch(String),
    #[error("projet : {0}")]
    Project(String),
    #[error("moteur : {0}")]
    Engine(String),
    #[error("{0} fichier(s) existent déjà dans le projet, dont {1} (autorise l'écrasement)")]
    Conflict(usize, String),
    #[error("trousseau : {0}")]
    Keyring(#[from] keyring_core::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
