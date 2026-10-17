//! Lecture d'un projet Unreal (.uproject) et de la version du moteur associé.

use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use egs_api::api::types::fab_library::FabAsset;

use crate::download::DownloadPlan;
use crate::{CoreError, Result};

/// Un projet Unreal ouvert depuis son .uproject.
#[derive(Debug, Clone)]
pub struct UnrealProject {
    /// Chemin complet du fichier .uproject
    pub uproject: PathBuf,
    /// Dossier qui contient le .uproject
    pub dir: PathBuf,
    pub name: String,
    /// Version du moteur, ex. (5, 4). `None` si on n'a pas pu la déterminer.
    pub engine: Option<(u32, u32)>,
}

impl UnrealProject {
    pub fn open(uproject: &Path) -> Result<Self> {
        let path = fs::canonicalize(uproject)?;
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
        let association = json["EngineAssociation"].as_str().unwrap_or("").trim();
        let dir = path
            .parent()
            .ok_or_else(|| CoreError::Project("chemin de projet invalide".into()))?
            .to_path_buf();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        // 1. "5.4" : moteur installé « officiellement »
        // 2. "{GUID}" : moteur compilé ou précompilé, enregistré dans Install.ini
        // 3. "" : projet rangé à l'intérieur de l'arborescence du moteur
        let engine = parse_version(association)
            .or_else(|| {
                engine_root_from_install_ini(association).and_then(|d| read_build_version(&d))
            })
            .or_else(|| {
                if association.is_empty() {
                    dir.ancestors().find_map(read_build_version)
                } else {
                    None
                }
            });

        Ok(Self {
            uproject: path,
            dir,
            name,
            engine,
        })
    }

    /// Version du moteur au format "5.4".
    pub fn engine_version(&self) -> Option<String> {
        self.engine.map(|(major, minor)| format!("{major}.{minor}"))
    }
}

/// "5.4" ou "5.4.2" → (5, 4)
pub fn parse_version(v: &str) -> Option<(u32, u32)> {
    let mut parts = v.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Cherche le dossier du moteur dans ~/.config/Epic/UnrealEngine/Install.ini.
fn engine_root_from_install_ini(guid: &str) -> Option<PathBuf> {
    let wanted = guid.trim_matches(['{', '}']).to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    let ini = BaseDirs::new()?
        .config_dir()
        .join("Epic/UnrealEngine/Install.ini");
    let text = fs::read_to_string(ini).ok()?;

    let mut in_section = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line.eq_ignore_ascii_case("[Installations]");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if in_section && key.trim().trim_matches(['{', '}']).to_lowercase() == wanted {
            return Some(PathBuf::from(value.trim()));
        }
    }
    None
}

/// Lit Engine/Build/Build.version sous la racine d'un moteur.
fn read_build_version(engine_root: &Path) -> Option<(u32, u32)> {
    let text = fs::read_to_string(engine_root.join("Engine/Build/Build.version")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some((
        v["MajorVersion"].as_u64()? as u32,
        v["MinorVersion"].as_u64()? as u32,
    ))
}

/// Ce qu'est un asset, d'après son `distribution_method`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// Contenu (Content/…) à copier dans le projet
    AssetPack,
    /// Plugin à ranger dans Plugins/
    Plugin,
    /// Projet complet : ne s'ajoute pas à un projet existant
    CompleteProject,
}

impl AssetKind {
    pub fn of(asset: &FabAsset) -> Self {
        let method = asset.distribution_method.to_uppercase();
        if method.contains("PLUGIN") {
            Self::Plugin
        } else if method.contains("PROJECT") {
            Self::CompleteProject
        } else {
            Self::AssetPack
        }
    }
}

/// Dossier où écrire les fichiers d'un plugin, selon la façon dont le manifest les range.
pub(crate) fn plugin_target(project_dir: &Path, plan: &DownloadPlan) -> Result<PathBuf> {
    let uplugin = plan
        .file_names()
        .find(|f| f.ends_with(".uplugin"))
        .ok_or_else(|| CoreError::Project("aucun fichier .uplugin dans ce plugin".into()))?;
    let uplugin = Path::new(uplugin);
    let plugins = project_dir.join("Plugins");

    Ok(if uplugin.starts_with("Plugins") {
        // "Plugins/Nom/Nom.uplugin" : déjà rangé depuis la racine du projet
        project_dir.to_path_buf()
    } else if uplugin.parent().is_some_and(|p| !p.as_os_str().is_empty()) {
        // "Nom/Nom.uplugin" : le dossier du plugin est déjà là
        plugins
    } else {
        // "Nom.uplugin" à la racine : on crée le dossier du plugin
        let stem = uplugin.file_stem().unwrap_or_default();
        plugins.join(stem)
    })
}