//! Configuration module for cosmic-pie-menu
//!
//! Reads the COSMIC dock favorites and applets from the system config.

use std::fs;
use std::path::PathBuf;

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
