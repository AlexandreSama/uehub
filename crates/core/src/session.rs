//! Persistance de la session Epic.
//! Version 1 : un fichier JSON lisible par toi seul (droits 600).
//! La partie 3 la remplace par le trousseau système.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use directories::ProjectDirs;
use egs_api::api::types::account::UserData;

use crate::{CoreError, Result};

pub(crate) struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub(crate) fn new() -> Result<Self> {
        // ~/.local/share/uehub/session.json
        let dirs = ProjectDirs::from("", "", "uehub").ok_or(CoreError::NoDataDir)?;
        Ok(Self {
            path: dirs.data_dir().join("session.json"),
        })
    }

    pub(crate) fn load(&self) -> Result<Option<UserData>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let json = fs::read_to_string(&self.path)?;
        Ok(Some(serde_json::from_str(&json)?))
    }

    pub(crate) fn save(&self, data: &UserData) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&self.path)?;
        file.write_all(serde_json::to_string_pretty(data)?.as_bytes())?;
        log::debug!("session enregistrée dans {}", self.path.display());
        Ok(())
    }

    pub(crate) fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}