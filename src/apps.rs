//! Application information module
//!
//! Parses desktop files and looks up icons for applications.

use std::fs;
use std::path::{Path, PathBuf};

use std::collections::{HashMap, HashSet};

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
    /// Path to the desktop file (for future use)
    #[allow(dead_code)]
    pub desktop_path: PathBuf,
    /// Number of running windows for this app (0 = not running)
    pub running_count: u32,
    /// Whether this app is a dock favorite (vs just running)
    pub is_favorite: bool,
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

    // Flatpak-installed applications
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

    // First, try exact match
    for dir in desktop_file_dirs() {
        let path = dir.join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    // If no exact match, search for desktop files ending with the app_id
    // This handles cases like app_id="Slack" matching "com.slack.Slack.desktop"
    let app_id_lower = app_id.to_lowercase();
    for dir in desktop_file_dirs() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".desktop") {
                    // Check if the last component before .desktop matches
                    let base = name_str.trim_end_matches(".desktop");
                    if let Some(last_part) = base.rsplit('.').next() {
                        if last_part.to_lowercase() == app_id_lower {
                            return Some(entry.path());
                        }
                    }
                    // Also try case-insensitive full match
                    if base.to_lowercase() == app_id_lower {
                        return Some(entry.path());
                    }
                }
            }
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
        running_count: 0,
        is_favorite: false,
    })
}

/// Load information for multiple apps (favorites)
pub fn load_apps(app_ids: &[String]) -> Vec<AppInfo> {
    app_ids
        .iter()
        .filter_map(|id| {
            let mut app = load_app_info(id)?;
            app.is_favorite = true;
            Some(app)
        })
        .collect()
}

/// Load apps with running status
/// Returns favorites first, then running non-favorites
pub fn load_apps_with_running(favorites: &[String], running_apps: &HashMap<String, u32>) -> Vec<AppInfo> {
    let mut apps = Vec::new();
    let mut seen_ids = HashSet::new();

    // First, add all favorites and mark if running
    for id in favorites {
        if let Some(mut app) = load_app_info(id) {
            app.is_favorite = true;
            app.running_count = get_running_count(id, running_apps);
            seen_ids.insert(id.clone());
            apps.push(app);
        }
    }

    // Then, add running apps that aren't favorites
    for (running_id, count) in running_apps {
        if !seen_ids.contains(running_id) && !is_id_in_set(running_id, &seen_ids) {
            if let Some(mut app) = load_app_info(running_id) {
                app.is_favorite = false;
                app.running_count = *count;
                seen_ids.insert(running_id.clone());
                apps.push(app);
            }
        }
    }

    apps
}

/// Get the running window count for an app ID (case-insensitive, handles variations)
fn get_running_count(app_id: &str, running_apps: &HashMap<String, u32>) -> u32 {
    // Direct match
    if let Some(&count) = running_apps.get(app_id) {
        return count;
    }

    let app_id_lower = app_id.to_lowercase();
    for (running, &count) in running_apps {
        // Case-insensitive match
        if running.to_lowercase() == app_id_lower {
            return count;
        }
        // Match the last part after dots (e.g., org.gnome.Nautilus -> Nautilus)
        if let Some(name) = running.rsplit('.').next() {
            if name.to_lowercase() == app_id_lower {
                return count;
            }
        }
        // Reverse: if app_id has dots, match its last part
        if let Some(name) = app_id.rsplit('.').next() {
            if running.to_lowercase() == name.to_lowercase() {
                return count;
            }
        }
    }

    0
}

/// Check if an ID is already in the seen set (handles case variations)
fn is_id_in_set(id: &str, seen: &HashSet<String>) -> bool {
    let id_lower = id.to_lowercase();
    seen.iter().any(|s| s.to_lowercase() == id_lower)
}

/// Dock applet definition
struct DockApplet {
    id: &'static str,
    name: &'static str,
    exec: &'static str,
    icon: &'static str,
}

/// Known dock applets that can be included in the pie menu
const DOCK_APPLETS: &[DockApplet] = &[
    DockApplet {
        id: "com.system76.CosmicPanelAppButton",
        name: "App Library",
        exec: "cosmic-app-library",
        icon: "com.system76.CosmicPanelAppButton",
    },
    DockApplet {
        id: "com.system76.CosmicPanelLauncherButton",
        name: "Launcher",
        exec: "cosmic-launcher",
        icon: "com.system76.CosmicPanelLauncherButton",
    },
    DockApplet {
        id: "com.system76.CosmicPanelWorkspacesButton",
        name: "Workspaces",
        exec: "cosmic-workspaces",
        icon: "com.system76.CosmicPanelWorkspacesButton",
    },
];

/// Create AppInfo entries for enabled dock applets
pub fn load_dock_applets(enabled_applets: &[String]) -> Vec<AppInfo> {
    let mut apps = Vec::new();

    for applet in DOCK_APPLETS {
        if enabled_applets.iter().any(|a| a == applet.id) {
            apps.push(AppInfo {
                id: applet.id.to_string(),
                name: applet.name.to_string(),
                icon: Some(applet.icon.to_string()),
                exec: Some(applet.exec.to_string()),
                desktop_path: PathBuf::new(), // No desktop file for applets
                running_count: 0,
                is_favorite: true, // Treat as favorites since they're in the dock
            });
        }
    }

    apps
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

    // Try direct paths in common icon themes (including Pop which has good COSMIC icons)
    let icon_themes = ["Pop", "Adwaita", "hicolor", "Papirus"];
    let categories = ["apps", "actions", "places", "status"];
    let sizes = [&format!("{}x{}", size, size), "scalable", "symbolic"];

    for theme in icon_themes {
        for sz in sizes {
            for category in categories {
                // Try with .svg extension
                let path = PathBuf::from(format!(
                    "/usr/share/icons/{}/{}/{}/{}.svg",
                    theme, sz, category, icon_name
                ));
                if path.exists() {
                    return Some(path);
                }
                // Try with .png extension
                let path = PathBuf::from(format!(
                    "/usr/share/icons/{}/{}/{}/{}.png",
                    theme, sz, category, icon_name
                ));
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }

    // For symbolic icons, try additional lookups
    if icon_name.ends_with("-symbolic") {
        // Try smaller sizes that symbolic icons typically come in
        for sym_size in [24, 16, 32, 48] {
            if let Some(path) = freedesktop_icons::lookup(icon_name)
                .with_size(sym_size)
                .with_scale(1)
                .find()
            {
                return Some(path);
            }
        }
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

    #[test]
    fn test_find_symbolic_icon() {
        // Test symbolic icons used by dock applets
        let result1 = find_icon_path("view-app-grid-symbolic", 48);
        println!("view-app-grid-symbolic: {:?}", result1);
        assert!(result1.is_some(), "Should find view-app-grid-symbolic");

        let result2 = find_icon_path("system-search-symbolic", 48);
        println!("system-search-symbolic: {:?}", result2);
        assert!(result2.is_some(), "Should find system-search-symbolic");

        let result3 = find_icon_path("view-paged-symbolic", 48);
        println!("view-paged-symbolic: {:?}", result3);
        assert!(result3.is_some(), "Should find view-paged-symbolic");
    }

    #[test]
    fn test_applet_icons() {
        // Test the actual icons we use for dock applets
        let result1 = find_icon_path("appgrid", 48);
        println!("appgrid: {:?}", result1);

        let result2 = find_icon_path("edit-find-symbolic", 48);
        println!("edit-find-symbolic: {:?}", result2);

        let result3 = find_icon_path("focus-windows-symbolic", 48);
        println!("focus-windows-symbolic: {:?}", result3);
    }
}
