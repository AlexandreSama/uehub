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
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;