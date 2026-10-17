//! Persistance de la session Epic.
//! Premier choix : le trousseau système (Secret Service : GNOME Keyring, KWallet…).
//! Repli : un fichier JSON lisible par toi seul (droits 600) si aucun trousseau ne répond.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use directories::ProjectDirs;
use egs_api::api::types::account::UserData;
use keyring_core::Entry;

use crate::{CoreError, Result};

const SERVICE: &str = "uehub";
const ACCOUNT: &str = "epic-session";

pub(crate) struct SessionStore {
    file: PathBuf,
    keyring: Option<Entry>,
}

impl SessionStore {
    pub(crate) fn new() -> Result<Self> {
        // ~/.local/share/uehub/session.json
        let dirs = ProjectDirs::from("", "", "uehub").ok_or(CoreError::NoDataDir)?;
        let store = Self {
            file: dirs.data_dir().join("session.json"),
            keyring: open_keyring(),
        };
        store.migrate_file_to_keyring()?;
        Ok(store)
    }

    /// Où la session est rangée, pour l'afficher à l'utilisateur.
    pub(crate) fn backend(&self) -> &'static str {
        if self.keyring.is_some() {
            "trousseau système"
        } else {
            "fichier (aucun trousseau disponible)"
        }
    }

    pub(crate) fn load(&self) -> Result<Option<UserData>> {
        let json = match &self.keyring {
            Some(entry) => match entry.get_password() {
                Ok(json) => json,
                Err(keyring_core::Error::NoEntry) => return Ok(None),
                Err(e) => return Err(e.into()),
            },
            None => {
                if !self.file.exists() {
                    return Ok(None);
                }
                fs::read_to_string(&self.file)?
            }
        };
        Ok(Some(serde_json::from_str(&json)?))
    }

    pub(crate) fn save(&self, data: &UserData) -> Result<()> {
        let json = serde_json::to_string(data)?;
        match &self.keyring {
            Some(entry) => entry.set_password(&json)?,
            None => write_private_file(&self.file, &json)?,
        }
        log::debug!("session enregistrée ({})", self.backend());
        Ok(())
    }

    pub(crate) fn clear(&self) -> Result<()> {
        if let Some(entry) = &self.keyring {
            match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        }
        if self.file.exists() {
            fs::remove_file(&self.file)?;
        }
        Ok(())
    }

    /// Si une session de la partie 1 traîne dans le fichier, on la passe dans le trousseau.
    fn migrate_file_to_keyring(&self) -> Result<()> {
        let Some(entry) = &self.keyring else {
            return Ok(());
        };
        if !self.file.exists() {
            return Ok(());
        }
        entry.set_password(&fs::read_to_string(&self.file)?)?;
        fs::remove_file(&self.file)?;
        log::info!("session déplacée du fichier vers le trousseau");
        Ok(())
    }
}

/// Ouvre l'entrée du trousseau, ou `None` s'il n'y a pas de Secret Service joignable.
fn open_keyring() -> Option<Entry> {
    match zbus_secret_service_keyring_store::Store::new() {
        Ok(store) => {
            keyring_core::set_default_store(store);
            Entry::new(SERVICE, ACCOUNT).ok()
        }
        Err(e) => {
            log::warn!("trousseau indisponible ({e}), repli sur fichier");
            None
        }
    }
}

fn write_private_file(path: &PathBuf, content: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}