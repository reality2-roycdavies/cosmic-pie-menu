//! Configuration module for cosmic-pie-menu
//!
//! Reads the COSMIC dock favorites and applets from the system config.
//! Also manages pie menu settings (gesture config).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Configuration for pie menu gesture detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieMenuConfig {
    /// Number of fingers for tap gesture (3 or 4)
    pub finger_count: u8,
    /// Maximum duration for tap in milliseconds
    pub tap_duration_ms: u64,
    /// Maximum movement threshold in touchpad units
    pub tap_movement: i32,
}

impl Default for PieMenuConfig {
    fn default() -> Self {
        Self {
            finger_count: 4,
            tap_duration_ms: 200,
            tap_movement: 500,
        }
    }
}

impl PieMenuConfig {
    /// Get the path to the config file
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cosmic-pie-menu")
            .join("config.json")
    }

    /// Load config from disk, or return defaults if not found
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Save config to disk
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)
    }
}

/// Runtime gesture configuration (derived from PieMenuConfig)
#[derive(Debug, Clone)]
pub struct GestureConfig {
    /// Number of fingers for tap gesture (3 or 4)
    pub finger_count: u8,
    /// Maximum duration for tap gesture
    pub tap_max_duration: Duration,
    /// Maximum movement threshold in touchpad units
    pub tap_max_movement: i32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self::from(&PieMenuConfig::default())
    }
}

impl From<&PieMenuConfig> for GestureConfig {
    fn from(config: &PieMenuConfig) -> Self {
        Self {
            finger_count: config.finger_count,
            tap_max_duration: Duration::from_millis(config.tap_duration_ms),
            tap_max_movement: config.tap_movement,
        }
    }
}

/// Thread-safe shared gesture configuration
pub type SharedConfig = Arc<RwLock<GestureConfig>>;

/// Get the path to COSMIC's app list favorites config
fn favorites_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(config_dir.join("cosmic/com.system76.CosmicAppList/v1/favorites"))
}

/// Get the path to COSMIC's dock plugins config
fn dock_plugins_path() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?;
    Some(config_dir.join("cosmic/com.system76.CosmicPanel.Dock/v1/plugins_center"))
}

/// Read the list of favorite app IDs from COSMIC dock config
///
/// Returns a list of app IDs (desktop file names without .desktop extension)
pub fn read_favorites() -> Vec<String> {
    let path = match favorites_path() {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config directory");
            return Vec::new();
        }
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Could not read favorites file {:?}: {}", path, e);
            return Vec::new();
        }
    };

    // Parse RON format - it's a simple array like ["app1", "app2", ...]
    match ron::from_str::<Vec<String>>(&content) {
        Ok(favorites) => favorites,
        Err(e) => {
            eprintln!("Could not parse favorites: {}", e);
            Vec::new()
        }
    }
}

/// Read the list of dock applets from COSMIC dock config
///
/// Returns a list of applet IDs that are enabled in the dock center
pub fn read_dock_applets() -> Vec<String> {
    let path = match dock_plugins_path() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    // Parse RON format - it's Some(["applet1", "applet2", ...]) or None
    // Try parsing as Option<Vec<String>>
    if let Ok(Some(applets)) = ron::from_str::<Option<Vec<String>>>(&content) {
        return applets;
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_favorites() {
        let favorites = read_favorites();
        println!("Favorites: {:?}", favorites);
        // Just check it doesn't panic
    }

    #[test]
    fn test_read_dock_applets() {
        let applets = read_dock_applets();
        println!("Dock applets: {:?}", applets);
    }
}
