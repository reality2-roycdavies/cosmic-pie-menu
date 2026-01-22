//! Application information module
//!
//! Parses desktop files and looks up icons for applications.

use std::fs;
use std::path::{Path, PathBuf};

/// Information about an application
#[derive(Debug, Clone)]
pub struct AppInfo {
    /// Application ID (desktop file name without .desktop)
    pub id: String,
    /// Display name
    pub name: String,
    /// Icon name or path
    pub icon: Option<String>,
    /// Executable command
    pub exec: Option<String>,
    /// Path to the desktop file
    pub desktop_path: PathBuf,
}

/// Get all standard locations for desktop files
fn desktop_file_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // System applications
    dirs.push(PathBuf::from("/usr/share/applications"));
    dirs.push(PathBuf::from("/usr/local/share/applications"));

    // User applications
    if let Some(data_dir) = dirs::data_local_dir() {
        dirs.push(data_dir.join("applications"));
    }

    // Flatpak applications
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }

    // Snap applications
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    dirs
}

/// Find the desktop file for an app ID
fn find_desktop_file(app_id: &str) -> Option<PathBuf> {
    let filename = format!("{}.desktop", app_id);

    for dir in desktop_file_dirs() {
        let path = dir.join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Parse a simple desktop file to extract key fields
/// This is a basic parser - for complex cases use freedesktop-desktop-entry crate
fn parse_desktop_file(path: &Path) -> Option<(String, Option<String>, Option<String>)> {
    let content = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut icon = None;
    let mut exec = None;
    let mut in_desktop_entry = false;

    for line in content.lines() {
        let line = line.trim();

        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }

        if line.starts_with('[') {
            in_desktop_entry = false;
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        if let Some(value) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("Icon=") {
            icon = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Exec=") {
            // Remove field codes like %u, %f, etc.
            let cleaned = value
                .replace("%u", "")
                .replace("%U", "")
                .replace("%f", "")
                .replace("%F", "")
                .replace("%i", "")
                .replace("%c", "")
                .replace("%k", "")
                .trim()
                .to_string();
            exec = Some(cleaned);
        }
    }

    Some((name?, icon, exec))
}

/// Load information for a single app by ID
pub fn load_app_info(app_id: &str) -> Option<AppInfo> {
    let desktop_path = find_desktop_file(app_id)?;
    let (name, icon, exec) = parse_desktop_file(&desktop_path)?;

    Some(AppInfo {
        id: app_id.to_string(),
        name,
        icon,
        exec,
        desktop_path,
    })
}

/// Load information for multiple apps
pub fn load_apps(app_ids: &[String]) -> Vec<AppInfo> {
    app_ids
        .iter()
        .filter_map(|id| load_app_info(id))
        .collect()
}

/// Find icon path for an icon name
/// Returns the path to the icon file, preferring SVG, then PNG
pub fn find_icon_path(icon_name: &str, size: u16) -> Option<PathBuf> {
    // If it's already a path, return it
    if icon_name.starts_with('/') {
        let path = PathBuf::from(icon_name);
        if path.exists() {
            return Some(path);
        }
    }

    // Try the exact name first
    if let Some(path) = freedesktop_icons::lookup(icon_name)
        .with_size(size)
        .with_scale(1)
        .find()
    {
        return Some(path);
    }

    // Try common alternate names
    let alternates: Vec<String> = vec![
        // app-name -> app-name-desktop (common for browsers)
        format!("{}-desktop", icon_name),
        // Remove -browser suffix and try -desktop
        icon_name.replace("-browser", "-desktop"),
        // Try lowercase
        icon_name.to_lowercase(),
    ];

    for alt in alternates {
        if alt != icon_name {
            if let Some(path) = freedesktop_icons::lookup(&alt)
                .with_size(size)
                .with_scale(1)
                .find()
            {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_app_info() {
        // Test with a common app
        if let Some(info) = load_app_info("com.system76.CosmicFiles") {
            println!("App: {} ({:?})", info.name, info.icon);
        }
    }

    #[test]
    fn test_find_icon() {
        // Test COSMIC app icon
        let result = find_icon_path("com.system76.CosmicFiles", 48);
        println!("CosmicFiles icon: {:?}", result);

        // Test standard app icon
        let result2 = find_icon_path("firefox", 48);
        println!("Firefox icon: {:?}", result2);

        // Test brave
        let result3 = find_icon_path("brave-browser", 48);
        println!("Brave icon: {:?}", result3);

        // Test Flatpak app (Betterbird)
        let result4 = find_icon_path("eu.betterbird.Betterbird", 48);
        println!("Betterbird icon: {:?}", result4);
    }
}
