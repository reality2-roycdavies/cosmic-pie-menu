//! COSMIC Pie Menu
//!
//! A radial app launcher for COSMIC desktop that mirrors the dock's favorites.
//!
//! Features:
//! - Reads favorites from COSMIC dock config
//! - Displays apps in a radial/pie layout
//! - Size scales with number of apps
//! - Tray icon for quick access and settings

mod apps;
mod config;
mod pie_menu;
mod tray;
mod windows;

use std::collections::HashSet;
use std::fs;
use std::process::Command;
use tray::TrayMessage;

/// Ensure autostart desktop file exists so tray starts on login
fn ensure_autostart() {
    let autostart_dir = match dirs::config_dir() {
        Some(config) => config.join("autostart"),
        None => return,
    };

    // Create autostart directory if needed
    if !autostart_dir.exists() {
        let _ = fs::create_dir_all(&autostart_dir);
    }

    let desktop_file = autostart_dir.join("cosmic-pie-menu.desktop");

    // Don't overwrite if user has modified it
    if desktop_file.exists() {
        return;
    }

    let content = r#"[Desktop Entry]
Type=Application
Name=COSMIC Pie Menu
Comment=Radial app launcher system tray
Exec=cosmic-pie-menu
Icon=cosmic-pie-menu
Terminal=false
Categories=Utility;
X-GNOME-Autostart-enabled=true
"#;

    if let Err(e) = fs::write(&desktop_file, content) {
        eprintln!("Failed to create autostart file: {}", e);
    } else {
        println!("Created autostart file at {:?}", desktop_file);
    }
}

/// Query running apps via subprocess to avoid Wayland connection conflicts
fn query_running_via_subprocess() -> HashSet<String> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
    match Command::new(&exe).arg("--query-running").output() {
        Ok(output) => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
        Err(e) => {
            eprintln!("Failed to query running apps: {}", e);
            HashSet::new()
        }
    }
}

/// Load all apps for the pie menu: dock applets first, then favorites, then running
fn load_all_pie_apps() -> Vec<apps::AppInfo> {
    let favorites = config::read_favorites();
    let running = query_running_via_subprocess();
    let dock_applets = config::read_dock_applets();

    // Start with dock applets (App Library, Launcher, Workspaces)
    let mut all_apps = apps::load_dock_applets(&dock_applets);
    let applet_count = all_apps.len();

    // Add favorites and running apps
    let favorite_apps = apps::load_apps_with_running(&favorites, &running);
    let app_count = favorite_apps.len();
    all_apps.extend(favorite_apps);

    println!("Loaded {} dock applets + {} apps", applet_count, app_count);

    all_apps
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // If --pie flag, show the pie menu directly (centered)
    if args.contains(&"--pie".to_string()) {
        let apps = load_all_pie_apps();
        println!("Total apps to show: {}", apps.len());
        pie_menu::show_pie_menu(apps);
        return;
    }

    // If --pie-at X Y, show the pie menu at a specific position
    if let Some(pos) = args.iter().position(|a| a == "--pie-at") {
        if args.len() > pos + 2 {
            let x: f32 = args[pos + 1].parse().unwrap_or(0.0);
            let y: f32 = args[pos + 2].parse().unwrap_or(0.0);
            let apps = load_all_pie_apps();
            pie_menu::show_pie_menu_at(apps, Some((x, y)));
            return;
        }
    }

    // If --track flag, use cursor tracking to position the menu
    if args.contains(&"--track".to_string()) {
        let apps = load_all_pie_apps();
        pie_menu::show_pie_menu_with_tracking(apps);
        return;
    }

    // Internal: --query-running just prints running apps and exits (for subprocess use)
    if args.contains(&"--query-running".to_string()) {
        let running = windows::get_running_apps();
        for app_id in running {
            println!("{}", app_id);
        }
        return;
    }

    println!("COSMIC Pie Menu starting...");

    // Ensure autostart file exists for next login
    ensure_autostart();

    // Load favorites from COSMIC dock config
    let favorites = config::read_favorites();
    let apps_list = apps::load_apps(&favorites);
    println!("Loaded {} apps from dock favorites", apps_list.len());

    // Start the tray icon
    let rx = match tray::run_tray() {
        Ok(rx) => rx,
        Err(e) => {
            eprintln!("Failed to start tray: {}", e);
            return;
        }
    };

    println!("Tray icon started. Click it or use the menu.");

    // Main event loop - handle tray messages
    loop {
        match rx.recv() {
            Ok(TrayMessage::ShowPieMenu { .. }) => {
                println!("Launching pie menu overlay...");
                // Spawn a new instance with --pie flag (centered on screen)
                let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
                let _ = Command::new(exe).arg("--pie").spawn();
            }
            Ok(TrayMessage::OpenSettings) => {
                println!("Settings requested!");
                // TODO: Open settings window
            }
            Ok(TrayMessage::ShowAbout) => {
                println!("About:");
                println!("  COSMIC Pie Menu v{}", env!("CARGO_PKG_VERSION"));
                println!("  A radial app launcher for COSMIC desktop");
            }
            Ok(TrayMessage::Quit) => {
                println!("Quit requested, exiting...");
                break;
            }
            Err(e) => {
                eprintln!("Channel error: {}", e);
                break;
            }
        }
    }
}
