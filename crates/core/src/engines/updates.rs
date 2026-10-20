//! Les mises à jour des moteurs installés.
//!
//! Epic publie des correctifs dans chaque branche : 5.8.3, puis 5.8.4. Une branche plus
//! récente (5.9) n'est pas une mise à jour : un projet 5.8 ne s'ouvre avec elle qu'après
//! conversion, et c'est à toi de choisir quand.

#[cfg(test)]
mod tests;

use super::cache::{EngineList, ListedEngine};
use super::install::InstalledEngine;
use super::version_key;

/// Deux versions de la même branche : 5.8.3 et 5.8.4, mais pas 5.8.3 et 5.9.0.
fn same_branch(a: &str, b: &str) -> bool {
    let (a, b) = (version_key(a), version_key(b));
    a.len() >= 2 && b.len() >= 2 && a[..2] == b[..2]
}

impl EngineList {
    /// La mise à jour d'un moteur installé : la version la plus récente de sa branche
    /// chez Epic, si elle est plus récente que lui. Un moteur déjà remplacé par un plus
    /// récent de sa branche n'en a pas : c'est le plus récent qui la porte.
    pub fn update_for(
        &self,
        version: &str,
        installed: &[InstalledEngine],
    ) -> Option<&ListedEngine> {
        if replaced_by(version, installed).is_some() {
            return None;
        }
        self.engines
            .iter()
            .filter(|e| same_branch(&e.version, version))
            .max_by_key(|e| version_key(&e.version))
            .filter(|newest| version_key(&newest.version) > version_key(version))
    }
}

/// Le moteur installé qui en remplace un autre : le plus récent de sa branche, s'il est
/// plus récent que lui. uehub ouvre maintenant avec lui les projets de cette branche,
/// sauf ceux qui citent l'ancien par son identifiant.
pub fn replaced_by<'a>(version: &str, installed: &'a [InstalledEngine]) -> Option<&'a str> {
    installed
        .iter()
        .filter_map(|e| e.version.as_deref())
        .filter(|v| same_branch(v, version))
        .max_by_key(|v| version_key(v))
        .filter(|newest| version_key(newest) > version_key(version))
}
