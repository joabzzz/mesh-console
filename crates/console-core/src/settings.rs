//! App settings persisted next to `chats.json` in the local data dir.
//! Currently: the last node configuration so a relaunch can offer (or later
//! auto-run) the same setup.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const APP_DIR: &str = "mesh-console";
const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LastNodeConfig {
    /// "serve" or "client".
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub mesh_name: Option<String>,
    #[serde(default)]
    pub publish: bool,
    #[serde(default)]
    pub join_tokens: Vec<String>,
    #[serde(default)]
    pub max_vram_gb: Option<f64>,
}

/// User-selectable appearance style for the app shell.
///
/// `Light` and `Dark` are monochrome palettes (Dark is true black); `Mesh`
/// layers a rainbow tech-mesh background behind frosted glass surfaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
    Mesh,
}

impl Appearance {
    /// Value used for the `data-appearance` attribute on the document root.
    pub fn as_data_value(self) -> &'static str {
        match self {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
            Appearance::Mesh => "mesh",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Set after the user starts a node for the first time; the onboarding
    /// flow is shown while this is `None`.
    #[serde(default)]
    pub last_node: Option<LastNodeConfig>,
    /// Selected appearance style. Defaults to `Light` for existing installs.
    #[serde(default)]
    pub appearance: Appearance,
}

fn storage_path() -> Result<PathBuf, String> {
    let base =
        dirs::data_local_dir().ok_or_else(|| "could not resolve data_local_dir".to_string())?;
    Ok(base.join(APP_DIR).join(SETTINGS_FILE))
}

pub fn load() -> Settings {
    let Ok(path) = storage_path() else {
        return Settings::default();
    };
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = storage_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}
