//! System tray module for cosmic-pie-menu
//!
//! Provides a tray icon that:
//! - Shows the pie menu icon in the system tray
//! - Provides menu options for settings, about, and quit
//! - Will eventually trigger the pie menu on click or hotkey

use ksni::{self, menu::StandardItem, Icon, MenuItem, Tray};
use ksni::blocking::TrayMethods as BlockingTrayMethods;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// Messages that can be sent from the tray to the main application
#[derive(Debug, Clone)]
pub enum TrayMessage {
    /// User clicked "Show Pie Menu" - includes cursor position
    ShowPieMenu { x: i32, y: i32 },
    /// User clicked "Settings"
    OpenSettings,
    /// User clicked "About"
    ShowAbout,
    /// User clicked "Quit"
    Quit,
}

/// Reason for tray exit - used for suspend/resume detection
#[derive(Debug)]
enum TrayExitReason {
    Quit,
    SuspendResume,
}

/// The tray icon state
struct PieMenuTray {
    /// Channel to send messages to the main app
    tx: Sender<TrayMessage>,
}

impl Tray for PieMenuTray {
    fn id(&self) -> String {
        "cosmic-pie-menu".to_string()
    }

    fn title(&self) -> String {
        "COSMIC Pie Menu".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        // Create a simple pie-chart style icon
        create_pie_icon()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            MenuItem::Standard(StandardItem {
                label: "Show Pie Menu".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    // Menu click doesn't have cursor pos, use 0,0 (will center)
                    let _ = tray.tx.send(TrayMessage::ShowPieMenu { x: 0, y: 0 });
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Settings...".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayMessage::OpenSettings);
                }),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: "About".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayMessage::ShowAbout);
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Quit".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayMessage::Quit);
                }),
                ..Default::default()
            }),
        ]
    }

    fn activate(&mut self, x: i32, y: i32) {
        // Left click shows the pie menu at cursor position
        let _ = self.tx.send(TrayMessage::ShowPieMenu { x, y });
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "COSMIC Pie Menu".to_string(),
            description: "Click to show pie menu launcher".to_string(),
            ..Default::default()
        }
    }
}

/// Create a simple pie-chart icon (32x32 ARGB)
fn create_pie_icon() -> Vec<Icon> {
    let size = 32i32;
    let mut pixels = vec![0u8; (size * size * 4) as usize];

    let center = size as f32 / 2.0;
    let radius = center - 2.0;

    // Colors for pie segments (ARGB format for ksni)
    let colors: [(u8, u8, u8); 6] = [
        (255, 100, 100), // Red
        (100, 255, 100), // Green
        (100, 100, 255), // Blue
        (255, 255, 100), // Yellow
        (255, 100, 255), // Magenta
        (100, 255, 255), // Cyan
    ];

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            let idx = ((y * size + x) * 4) as usize;

            if dist <= radius && dist > radius * 0.3 {
                // Calculate angle and determine segment
                let angle = dy.atan2(dx) + std::f32::consts::PI;
                let segment = ((angle / (2.0 * std::f32::consts::PI)) * 6.0) as usize;
                let (r, g, b) = colors[segment % 6];

                pixels[idx] = 255;     // A
                pixels[idx + 1] = r;   // R
                pixels[idx + 2] = g;   // G
                pixels[idx + 3] = b;   // B
            } else if dist <= radius * 0.3 {
                // Center dot
                pixels[idx] = 255;     // A
                pixels[idx + 1] = 200; // R
                pixels[idx + 2] = 200; // G
                pixels[idx + 3] = 200; // B
            }
        }
    }

    vec![Icon {
        width: size,
        height: size,
        data: pixels,
    }]
}

/// Inner tray run loop - returns reason for exit
fn run_tray_inner(tx: Sender<TrayMessage>) -> Result<TrayExitReason, String> {
    let tray = PieMenuTray { tx: tx.clone() };

    // Spawn the tray - not sandboxed (native app)
    let handle = BlockingTrayMethods::disable_dbus_name(tray, false)
        .spawn()
        .map_err(|e| format!("Failed to spawn tray: {}", e))?;

    // Main event loop
    let mut last_loop_time = Instant::now();

    loop {
        let loop_start = Instant::now();

        // Check for time jump (suspend/resume detection)
        let elapsed = loop_start.duration_since(last_loop_time);
        if elapsed > Duration::from_secs(5) {
            println!("Time jump detected ({:?}), likely suspend/resume", elapsed);
            handle.shutdown();
            return Ok(TrayExitReason::SuspendResume);
        }
        last_loop_time = loop_start;

        // Sleep briefly
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Run the tray icon service
/// Returns a receiver for tray messages
pub fn run_tray() -> Result<Receiver<TrayMessage>, String> {
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        // Small delay to let the panel initialize
        std::thread::sleep(Duration::from_secs(2));

        // Retry loop for suspend/resume
        loop {
            match run_tray_inner(tx.clone()) {
                Ok(TrayExitReason::Quit) => break,
                Ok(TrayExitReason::SuspendResume) => {
                    println!("Detected suspend/resume, restarting tray...");
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                Err(e) => {
                    eprintln!("Tray error: {}", e);
                    break;
                }
            }
        }
    });

    Ok(rx)
}
