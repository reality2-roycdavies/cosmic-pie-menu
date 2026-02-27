//! COSMIC Pie Menu
//!
//! A radial app launcher for COSMIC desktop that mirrors the dock's favorites.
//!
//! Features:
//! - Reads favorites from COSMIC dock config
//! - Displays apps in a radial/pie layout
//! - Size scales with number of apps
//! - COSMIC panel applet for quick access and settings

mod applet;
mod apps;
mod config;
mod gesture;
mod pie_menu;
mod settings;
mod settings_page;
mod windows;

use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::process::Command;

/// Query running apps via subprocess to avoid Wayland connection conflicts
/// Returns a map of app_id -> window count
fn query_running_via_subprocess() -> HashMap<String, u32> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
    match Command::new(&exe).arg("--query-running").output() {
        Ok(output) => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|s| !s.is_empty())
                .filter_map(|line| {
                    // Parse "app_id:count" format
                    let parts: Vec<&str> = line.rsplitn(2, ':').collect();
                    if parts.len() == 2 {
                        let count = parts[0].parse().unwrap_or(1);
                        let app_id = parts[1].to_string();
                        Some((app_id, count))
                    } else {
                        // Fallback for old format (just app_id)
                        Some((line.to_string(), 1))
                    }
                })
                .collect()
        }
        Err(e) => {
            eprintln!("Failed to query running apps: {}", e);
            HashMap::new()
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

/// Try to acquire an exclusive lock for a subprocess mode (track/pie).
/// Prevents duplicate subprocesses on multi-monitor setups.
fn try_subprocess_lock(name: &str) -> Option<std::fs::File> {
    let run_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let lock_path = format!("{}/cosmic-pie-menu-{}.lock", run_dir, name);
    let file = std::fs::File::create(&lock_path).ok()?;
    let fd = file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        Some(file)
    } else {
        None
    }
}

fn main() -> cosmic::iced::Result {
    let args: Vec<String> = std::env::args().collect();

    // Internal: --pie-at X Y, show the pie menu at a specific position (used by gesture system)
    if let Some(pos) = args.iter().position(|a| a == "--pie-at") {
        if args.len() > pos + 2 {
            // Singleton: only one pie menu at a time
            let _lock = match try_subprocess_lock("pie") {
                Some(f) => f,
                None => return Ok(()),
            };
            let x: f32 = args[pos + 1].parse().unwrap_or(0.0);
            let y: f32 = args[pos + 2].parse().unwrap_or(0.0);
            let apps = load_all_pie_apps();
            pie_menu::show_pie_menu_at(apps, Some((x, y)));
            return Ok(());
        }
    }

    // Internal: --track flag, use cursor tracking to position the menu (used by gesture system)
    if args.contains(&"--track".to_string()) {
        // Singleton: only one tracker at a time (prevents duplicates on multi-monitor)
        let _lock = match try_subprocess_lock("track") {
            Some(f) => f,
            None => return Ok(()),
        };
        let apps = load_all_pie_apps();
        pie_menu::show_pie_menu_with_tracking(apps);
        return Ok(());
    }

    // --settings: try the unified settings hub first, fall back to standalone
    if args.contains(&"--settings".to_string()) {
        use std::process::Command;
        if Command::new("cosmic-applet-settings")
            .arg("io.github.reality2_roycdavies.cosmic-pie-menu")
            .spawn()
            .is_ok()
        {
            return Ok(());
        }
        settings::run_settings(None);
        return Ok(());
    }

    // --settings-standalone: always open standalone settings window
    if args.contains(&"--settings-standalone".to_string()) {
        settings::run_settings(None);
        return Ok(());
    }

    // Internal: --query-running just prints running apps and exits (for subprocess use)
    // Output format: app_id:count (one per line)
    if args.contains(&"--query-running".to_string()) {
        let running = windows::get_running_apps();
        for (app_id, count) in running {
            println!("{}:{}", app_id, count);
        }
        return Ok(());
    }

    // Default: run as COSMIC panel applet
    println!("COSMIC Pie Menu starting as panel applet...");
    applet::run_applet()
}
