//! Cœur de l'application : tout ce qui parle à Epic et touche au disque.
//! Aucune dépendance d'interface ici : la CLI et l'app Tauri partagent ce code.

mod error;
mod session;

pub use egs_api::api::types::fab_library::FabAsset;
pub use error::{CoreError, Result};

use egs_api::EpicGames;
use session::SessionStore;

/// Page de connexion Epic. Après connexion, elle affiche un JSON
/// dont il faut copier la valeur `authorizationCode`.
pub const LOGIN_URL: &str = "https://www.epicgames.com/id/login?redirectUrl=https%3A%2F%2Fwww.epicgames.com%2Fid%2Fapi%2Fredirect%3FclientId%3D34a02cf8f4414e29b15921876da36f9a%26responseType%3Dcode";

/// Point d'entrée unique vers les services Epic.
pub struct EpicClient {
    egs: EpicGames,
    store: SessionStore,
}

impl EpicClient {
    /// Crée le client et recharge la session enregistrée si elle existe.
    pub fn new() -> Result<Self> {
        let store = SessionStore::new()?;
        let mut egs = EpicGames::new();
        if let Some(saved) = store.load()? {
            egs.set_user_details(saved);
        }
        Ok(Self { egs, store })
    }

    /// Première connexion avec le code copié depuis `LOGIN_URL`.
    pub async fn login_with_code(&mut self, code: &str) -> Result<()> {
        let code = code.trim().trim_matches('"').to_string();
        if !self.egs.auth_code(None, Some(code)).await {
            return Err(CoreError::AuthFailed);
        }
        self.store.save(&self.egs.user_details())
    }

    /// Reprend la session enregistrée (rafraîchit le token si besoin).
    pub async fn resume(&mut self) -> Result<()> {
        if self.egs.is_logged_in() {
            return Ok(());
        }
        if !self.egs.login().await {
            return Err(CoreError::NotLoggedIn);
        }
        self.store.save(&self.egs.user_details())
    }

    /// Nom affiché du compte connecté.
    pub fn display_name(&self) -> Option<String> {
        self.egs.user_details().display_name
    }

    /// Toute la bibliothèque Fab du compte (pagination gérée par egs-api).
    pub async fn fab_library(&mut self) -> Result<Vec<FabAsset>> {
        self.resume().await?;
        let account_id = self
            .egs
            .user_details()
            .account_id
            .ok_or(CoreError::NotLoggedIn)?;
        let library = self
            .egs
            .try_fab_library_items(account_id)
            .await
            .map_err(|e| CoreError::Api(format!("{e:?}")))?;
        Ok(library.results)
    }

    /// Déconnexion côté Epic + suppression de la session locale.
    pub async fn logout(&mut self) -> Result<()> {
        let _ = self.egs.logout().await;
        self.store.clear()
    }
}