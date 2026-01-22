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

use std::process::Command;
use tray::TrayMessage;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // If --pie flag, show the pie menu directly
    if args.contains(&"--pie".to_string()) {
        let favorites = config::read_favorites();
        let apps = apps::load_apps(&favorites);
        pie_menu::show_pie_menu(apps);
        return;
    }

    println!("COSMIC Pie Menu starting...");

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
