//! System tray module for cosmic-pie-menu
//!
//! Provides a tray icon that:
//! - Shows the pie menu icon in the system tray
//! - Provides menu options for settings, about, and quit
//! - Will eventually trigger the pie menu on click or hotkey

use ksni::{self, menu::StandardItem, Icon, MenuItem, Tray};
use ksni::blocking::TrayMethods as BlockingTrayMethods;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Get the path to COSMIC's theme mode config file
fn cosmic_theme_mode_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("cosmic/com.system76.CosmicTheme.Mode/v1/is_dark"))
}

/// Get the path to the active theme directory
fn cosmic_theme_dir() -> Option<PathBuf> {
    let is_dark = is_dark_mode();
    let theme_name = if is_dark { "Dark" } else { "Light" };
    dirs::config_dir().map(|d| d.join(format!("cosmic/com.system76.CosmicTheme.{}/v1", theme_name)))
}

/// Detect if the system is in dark mode
fn is_dark_mode() -> bool {
    if let Some(path) = cosmic_theme_mode_path() {
        if let Ok(content) = fs::read_to_string(&path) {
            let trimmed = content.trim();
            // COSMIC stores "true" or "false"
            return trimmed == "true";
        }
    }
    // Default to dark mode
    true
}

/// Parse a color from COSMIC theme RON format
/// Looks for pattern like: red: 0.5, green: 0.3, blue: 0.2,
fn parse_color_from_ron(content: &str, color_name: &str) -> Option<(u8, u8, u8)> {
    // Find the color block by name (e.g., "base:" or "on:")
    let search_pattern = format!("{}:", color_name);
    let start_idx = content.find(&search_pattern)?;
    let block_start = content[start_idx..].find('(')?;
    let block_end = content[start_idx + block_start..].find(')')?;
    let block = &content[start_idx + block_start..start_idx + block_start + block_end + 1];

    // Extract red, green, blue values
    let extract_float = |name: &str| -> Option<f32> {
        let pattern = format!("{}: ", name);
        let idx = block.find(&pattern)?;
        let start = idx + pattern.len();
        let end = block[start..].find(',')?;
        block[start..start + end].trim().parse().ok()
    };

    let red = extract_float("red")?;
    let green = extract_float("green")?;
    let blue = extract_float("blue")?;

    Some((
        (red.clamp(0.0, 1.0) * 255.0) as u8,
        (green.clamp(0.0, 1.0) * 255.0) as u8,
        (blue.clamp(0.0, 1.0) * 255.0) as u8,
    ))
}

/// Get theme colors for the tray icon by reading directly from config files
/// This avoids potential caching issues with the cosmic::theme API
fn get_theme_colors() -> ((u8, u8, u8), (u8, u8, u8)) {
    // Default colors (light gray for normal, cyan for triggered)
    let default_normal = (200, 200, 200);
    let default_triggered = (0, 200, 200);

    let theme_dir = match cosmic_theme_dir() {
        Some(dir) => dir,
        None => return (default_normal, default_triggered),
    };

    // Read accent color (for triggered state)
    let accent_path = theme_dir.join("accent");
    let triggered = if let Ok(content) = fs::read_to_string(&accent_path) {
        parse_color_from_ron(&content, "base").unwrap_or(default_triggered)
    } else {
        default_triggered
    };

    // Read background on color (for normal state)
    let bg_path = theme_dir.join("background");
    let normal = if let Ok(content) = fs::read_to_string(&bg_path) {
        // The "on" color is the foreground color for text/icons
        parse_color_from_ron(&content, "on").unwrap_or(default_normal)
    } else {
        default_normal
    };

    (normal, triggered)
}

/// Messages that can be sent from the tray to the main application
#[derive(Debug, Clone)]
pub enum TrayMessage {
    /// User clicked "Show Pie Menu" - includes cursor position
    ShowPieMenu { x: i32, y: i32 },
    /// User clicked "Settings"
    OpenSettings,
    /// User clicked "Quit"
    Quit,
}

/// Reason for tray exit - used for suspend/resume and theme change detection
#[derive(Debug)]
enum TrayExitReason {
    Quit,
    SuspendResume,
    ThemeChanged,
}

/// Shared state for gesture feedback
#[derive(Clone)]
pub struct GestureFeedback {
    triggered: Arc<AtomicBool>,
    reset_requested: Arc<AtomicBool>,
}

impl GestureFeedback {
    pub fn new() -> Self {
        Self {
            triggered: Arc::new(AtomicBool::new(false)),
            reset_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal that a gesture was detected (turns icon cyan)
    pub fn trigger(&self) {
        self.triggered.store(true, Ordering::SeqCst);
    }

    /// Signal that the menu has closed (turns icon back to normal)
    pub fn reset(&self) {
        self.reset_requested.store(true, Ordering::SeqCst);
    }

    /// Check if triggered and clear the flag
    fn check_and_reset_trigger(&self) -> bool {
        self.triggered.swap(false, Ordering::SeqCst)
    }

    /// Check if reset was requested and clear the flag
    fn check_and_reset_reset(&self) -> bool {
        self.reset_requested.swap(false, Ordering::SeqCst)
    }
}

/// The tray icon state
struct PieMenuTray {
    /// Channel to send messages to the main app
    tx: Sender<TrayMessage>,
    /// Whether system is in dark mode
    dark_mode: bool,
    /// Whether gesture was just triggered (for visual feedback)
    gesture_triggered: bool,
}

impl Tray for PieMenuTray {
    // Show menu on left-click instead of calling activate
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "cosmic-pie-menu".to_string()
    }

    fn title(&self) -> String {
        "COSMIC Pie Menu".to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        // Create a styled icon that adapts to theme and gesture state
        create_pie_icon(self.dark_mode, self.gesture_triggered)
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            MenuItem::Standard(StandardItem {
                label: "Show Pie Menu".to_string(),
                icon_name: "view-app-grid-symbolic".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    // Menu click doesn't have cursor pos, use 0,0 (will center)
                    let _ = tray.tx.send(TrayMessage::ShowPieMenu { x: 0, y: 0 });
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Settings...".to_string(),
                icon_name: "preferences-system-symbolic".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayMessage::OpenSettings);
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit-symbolic".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayMessage::Quit);
                }),
                ..Default::default()
            }),
        ]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        // Left-click does nothing - use the dropdown menu or gestures to show pie menu
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "COSMIC Pie Menu".to_string(),
            description: "Click for menu, use touchpad gesture to show pie menu".to_string(),
            ..Default::default()
        }
    }
}

/// Create a styled icon with dots in a circle + center dot (32x32 ARGB)
/// Adapts to COSMIC theme colors and shows highlight when gesture triggered
fn create_pie_icon(_dark_mode: bool, triggered: bool) -> Vec<Icon> {
    let size = 32i32;
    let mut pixels = vec![0u8; (size * size * 4) as usize];

    let center = size as f32 / 2.0;
    let outer_radius = center - 3.0;
    let dot_radius = 2.5;
    let center_dot_radius = 4.0;
    let num_dots = 8;

    // Get colors from COSMIC theme
    let (normal_color, triggered_color) = get_theme_colors();
    let (r, g, b) = if triggered {
        triggered_color
    } else {
        normal_color
    };

    // Draw outer dots in a circle
    for i in 0..num_dots {
        let angle = (i as f32 / num_dots as f32) * 2.0 * std::f32::consts::PI - std::f32::consts::FRAC_PI_2;
        let dot_x = center + outer_radius * angle.cos();
        let dot_y = center + outer_radius * angle.sin();

        // Fill pixels within dot radius
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - dot_x;
                let dy = y as f32 - dot_y;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist <= dot_radius {
                    let idx = ((y * size + x) * 4) as usize;
                    // Anti-aliasing at edges
                    let alpha = if dist > dot_radius - 1.0 {
                        ((dot_radius - dist) * 255.0) as u8
                    } else {
                        255
                    };
                    // Blend if there's already a pixel
                    if pixels[idx] < alpha {
                        pixels[idx] = alpha;
                        pixels[idx + 1] = r;
                        pixels[idx + 2] = g;
                        pixels[idx + 3] = b;
                    }
                }
            }
        }
    }

    // Draw center dot
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= center_dot_radius {
                let idx = ((y * size + x) * 4) as usize;
                let alpha = if dist > center_dot_radius - 1.0 {
                    ((center_dot_radius - dist) * 255.0) as u8
                } else {
                    255
                };
                if pixels[idx] < alpha {
                    pixels[idx] = alpha;
                    pixels[idx + 1] = r;
                    pixels[idx + 2] = g;
                    pixels[idx + 3] = b;
                }
            }
        }
    }

    vec![Icon {
        width: size,
        height: size,
        data: pixels,
    }]
}

/// Get modification time of theme color files for change detection
fn get_theme_files_mtime() -> Option<std::time::SystemTime> {
    let theme_dir = cosmic_theme_dir()?;
    let accent_path = theme_dir.join("accent");
    let bg_path = theme_dir.join("background");

    // Return the most recent modification time of either file
    let accent_mtime = fs::metadata(&accent_path).ok()?.modified().ok()?;
    let bg_mtime = fs::metadata(&bg_path).ok()?.modified().ok()?;

    Some(accent_mtime.max(bg_mtime))
}

/// Inner tray run loop - returns reason for exit
fn run_tray_inner(tx: Sender<TrayMessage>, feedback: GestureFeedback) -> Result<TrayExitReason, String> {
    let current_dark_mode = is_dark_mode();
    let initial_mtime = get_theme_files_mtime();

    let tray = PieMenuTray {
        tx: tx.clone(),
        dark_mode: current_dark_mode,
        gesture_triggered: false,
    };

    // Spawn the tray - not sandboxed (native app)
    let handle = BlockingTrayMethods::disable_dbus_name(tray, false)
        .spawn()
        .map_err(|e| format!("Failed to spawn tray: {}", e))?;

    // Main event loop
    let mut last_loop_time = Instant::now();
    let mut last_theme_check = Instant::now();
    let tracked_dark_mode = current_dark_mode;
    let mut tracked_mtime = initial_mtime;
    let mut icon_highlighted = false;

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

        // Check for gesture trigger - highlight the icon
        if feedback.check_and_reset_trigger() && !icon_highlighted {
            icon_highlighted = true;
            // Update tray with highlighted icon
            handle.update(|tray| {
                tray.gesture_triggered = true;
            });
        }

        // Check for reset request - unhighlight the icon when menu closes
        if feedback.check_and_reset_reset() && icon_highlighted {
            icon_highlighted = false;
            handle.update(|tray| {
                tray.gesture_triggered = false;
            });
        }

        // Check for theme changes every second (both dark/light mode AND color file changes)
        if loop_start.duration_since(last_theme_check) > Duration::from_secs(1) {
            last_theme_check = loop_start;

            // Check dark/light mode change
            let new_dark_mode = is_dark_mode();
            if new_dark_mode != tracked_dark_mode {
                println!("Theme mode changed (dark_mode: {} -> {}), restarting tray...", tracked_dark_mode, new_dark_mode);
                handle.shutdown();
                return Ok(TrayExitReason::ThemeChanged);
            }

            // Check if theme color files have been modified
            let new_mtime = get_theme_files_mtime();
            if new_mtime != tracked_mtime {
                println!("Theme colors changed, restarting tray...");
                handle.shutdown();
                return Ok(TrayExitReason::ThemeChanged);
            }
            tracked_mtime = new_mtime;
        }

        // Sleep briefly
        std::thread::sleep(Duration::from_millis(50)); // Faster polling for responsive feedback
    }
}

/// Run the tray icon service with an externally provided sender
/// This allows sharing the channel with other components (like gesture detection)
pub fn run_tray_with_sender(tx: Sender<TrayMessage>, feedback: GestureFeedback) {
    // Small delay to let the panel initialize
    std::thread::sleep(Duration::from_secs(2));

    // Retry loop for suspend/resume and theme changes
    loop {
        match run_tray_inner(tx.clone(), feedback.clone()) {
            Ok(TrayExitReason::Quit) => break,
            Ok(TrayExitReason::SuspendResume) => {
                println!("Detected suspend/resume, restarting tray...");
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            Ok(TrayExitReason::ThemeChanged) => {
                // Wait for theme files to be fully written before restarting
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            Err(e) => {
                eprintln!("Tray error: {}", e);
                break;
            }
        }
    }
}

/// Run the tray icon service (without gesture feedback)
/// Returns a receiver for tray messages
#[allow(dead_code)]
pub fn run_tray() -> Result<Receiver<TrayMessage>, String> {
    let (tx, rx) = mpsc::channel();
    let feedback = GestureFeedback::new();

    std::thread::spawn(move || {
        run_tray_with_sender(tx, feedback);
    });

    Ok(rx)
}
