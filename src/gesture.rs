//! Touchpad gesture detection module
//!
//! Detects multi-finger press/release on touchpads using Linux evdev and triggers
//! the pie menu. Requires user to be in the 'input' group.
//!
//! Supports configurable finger count (3 or 4), tap duration, and movement threshold.

use evdev::{AbsoluteAxisType, Device, InputEventKind, Key};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::config::{GestureConfig, PieMenuConfig, SharedConfig};
use crate::tray::{GestureFeedback, TrayMessage};

/// Errors that can occur during gesture detection setup
#[derive(Debug)]
pub enum GestureError {
    /// No touchpad devices found with multi-finger tap support
    NoTouchpadFound,
    /// Permission denied - user not in input group
    PermissionDenied(String),
    /// Device open failed for other reason
    DeviceError(String),
    /// Thread spawn failed
    ThreadError(String),
}

impl std::fmt::Display for GestureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTouchpadFound => {
                write!(f, "No touchpad with multi-finger tap support found")
            }
            Self::PermissionDenied(path) => write!(
                f,
                "Permission denied accessing {}. Add user to 'input' group: sudo usermod -aG input $USER",
                path
            ),
            Self::DeviceError(msg) => write!(f, "Device error: {}", msg),
            Self::ThreadError(msg) => write!(f, "Thread error: {}", msg),
        }
    }
}

/// State machine for tracking multi-finger gesture
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureState {
    /// Waiting for fingers down
    Idle,
    /// Fingers are down, tracking time and position
    FingersDown {
        start: Instant,
        /// Starting X position on touchpad
        start_x: Option<i32>,
        /// Starting Y position on touchpad
        start_y: Option<i32>,
        /// Maximum distance moved from start
        max_movement: i32,
    },
    /// Tap detected, waiting to confirm it's not a 3→4 finger transition
    /// (only used in 3-finger mode)
    PendingTrigger {
        /// When the pending trigger was set
        pending_since: Instant,
    },
}

/// Find all touchpad device paths in /dev/input/ that support the given finger count
fn find_touchpad_paths(finger_count: u8) -> Vec<PathBuf> {
    let mut touchpads = Vec::new();

    let input_dir = match std::fs::read_dir("/dev/input") {
        Ok(dir) => dir,
        Err(_) => return touchpads,
    };

    for entry in input_dir.flatten() {
        let path = entry.path();

        // Only look at event devices
        if !path.to_string_lossy().contains("event") {
            continue;
        }

        // Try to open device (may fail without permissions)
        let device = match Device::open(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Check if it's a touchpad with the required finger support
        if is_touchpad_with_finger_support(&device, finger_count) {
            println!(
                "Found touchpad with {}-finger support: {} ({})",
                finger_count,
                device.name().unwrap_or("Unknown"),
                path.display()
            );
            touchpads.push(path);
        }
    }

    touchpads
}

/// Check if a device is a touchpad with the required finger tap capability
fn is_touchpad_with_finger_support(device: &Device, finger_count: u8) -> bool {
    let keys = match device.supported_keys() {
        Some(k) => k,
        None => return false,
    };

    // Check for the appropriate key based on finger count
    let required_key = if finger_count == 3 {
        Key::BTN_TOOL_TRIPLETAP
    } else {
        Key::BTN_TOOL_QUADTAP
    };

    if !keys.contains(required_key) {
        return false;
    }

    // Must have absolute axes (touchpad characteristic)
    let abs = match device.supported_absolute_axes() {
        Some(a) => a,
        None => return false,
    };

    // Check for standard touchpad axes
    abs.contains(AbsoluteAxisType::ABS_X) || abs.contains(AbsoluteAxisType::ABS_MT_POSITION_X)
}

/// Debounce time for 3-finger mode to avoid false triggers on 3→4 transitions
const PENDING_TRIGGER_DEBOUNCE: Duration = Duration::from_millis(150);

/// Events returned from process_event
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureEvent {
    None,
    FingersDown,
    FingersUp,
    /// Trigger was cancelled (3→4 finger transition detected)
    TriggerCancelled,
    /// Swipe detected (too much movement or duration) - reset icon
    SwipeDetected,
}

/// Process a single input event, returning gesture events
/// Uses the provided config for tap detection parameters
fn process_event(
    event: &evdev::InputEvent,
    state: &mut GestureState,
    finger_count: u8,
    tap_max_duration: Duration,
    tap_max_movement: i32,
) -> GestureEvent {
    // Determine which key to watch based on finger count
    let tap_key = if finger_count == 3 {
        Key::BTN_TOOL_TRIPLETAP
    } else {
        Key::BTN_TOOL_QUADTAP
    };

    // In 3-finger mode, also watch for 4-finger to cancel pending triggers
    let cancel_key = if finger_count == 3 {
        Some(Key::BTN_TOOL_QUADTAP)
    } else {
        None
    };

    match event.kind() {
        InputEventKind::Key(key) if key == tap_key => {
            if event.value() == 1 {
                // Fingers went down - record the time, will capture position on first touch event
                *state = GestureState::FingersDown {
                    start: Instant::now(),
                    start_x: None,
                    start_y: None,
                    max_movement: 0,
                };
                return GestureEvent::FingersDown;
            } else if event.value() == 0 {
                // Fingers lifted - check if it was a quick tap (not a swipe)
                if let GestureState::FingersDown { start, max_movement, .. } = *state {
                    let duration = start.elapsed();

                    if duration <= tap_max_duration && max_movement <= tap_max_movement {
                        // Quick tap with little movement
                        if finger_count == 3 {
                            // In 3-finger mode, use pending trigger to avoid 3→4 transitions
                            *state = GestureState::PendingTrigger {
                                pending_since: Instant::now(),
                            };
                            // Don't return FingersUp yet - wait for debounce
                            return GestureEvent::None;
                        } else {
                            // In 4-finger mode, trigger immediately
                            *state = GestureState::Idle;
                            return GestureEvent::FingersUp;
                        }
                    } else {
                        // Swipe gesture - ignore and reset icon
                        *state = GestureState::Idle;
                        println!(
                            "Gesture ignored (duration: {:?}, movement: {}) - likely a swipe",
                            duration, max_movement
                        );
                        return GestureEvent::SwipeDetected;
                    }
                }
            }
        }
        // In 3-finger mode, watch for 4-finger to cancel pending trigger
        InputEventKind::Key(key) if Some(key) == cancel_key && event.value() == 1 => {
            if let GestureState::PendingTrigger { .. } = *state {
                // 4th finger went down while we had a pending 3-finger trigger
                // This is a 3→4 transition, cancel the trigger
                *state = GestureState::Idle;
                println!("3-finger trigger cancelled (4th finger detected)");
                return GestureEvent::TriggerCancelled;
            }
        }
        // Track absolute finger position while fingers are down
        InputEventKind::AbsAxis(axis) => {
            if let GestureState::FingersDown { start_x, start_y, max_movement, .. } = state {
                let val = event.value();
                match axis {
                    // Track multitouch position (primary finger)
                    AbsoluteAxisType::ABS_MT_POSITION_X | AbsoluteAxisType::ABS_X => {
                        if let Some(sx) = *start_x {
                            let dist = (val - sx).abs();
                            if dist > *max_movement {
                                *max_movement = dist;
                            }
                        } else {
                            *start_x = Some(val);
                        }
                    }
                    AbsoluteAxisType::ABS_MT_POSITION_Y | AbsoluteAxisType::ABS_Y => {
                        if let Some(sy) = *start_y {
                            let dist = (val - sy).abs();
                            if dist > *max_movement {
                                *max_movement = dist;
                            }
                        } else {
                            *start_y = Some(val);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    GestureEvent::None
}

/// Check if a pending trigger has timed out and should fire
fn check_pending_trigger(state: &mut GestureState) -> bool {
    if let GestureState::PendingTrigger { pending_since } = *state {
        if pending_since.elapsed() >= PENDING_TRIGGER_DEBOUNCE {
            *state = GestureState::Idle;
            return true;
        }
    }
    false
}

/// Wrapper to hold device
struct TouchpadDevice {
    device: Device,
}

/// Main gesture detection loop with configurable parameters
fn gesture_loop(tx: Sender<TrayMessage>, feedback: GestureFeedback, config: SharedConfig) {
    let mut state = GestureState::Idle;
    let mut last_scan = Instant::now();
    let mut last_config_check = Instant::now();
    let rescan_interval = Duration::from_secs(30);
    let config_check_interval = Duration::from_secs(2); // Check config file every 2 seconds

    // Read initial config from disk (settings may have changed while we were down)
    let initial_cfg = GestureConfig::from(&PieMenuConfig::load());
    let mut current_finger_count = initial_cfg.finger_count;
    let mut current_cfg = initial_cfg;

    // Update shared config with loaded values
    if let Ok(mut shared) = config.write() {
        *shared = current_cfg.clone();
    }

    // Initial device scan
    let paths = find_touchpad_paths(current_finger_count);
    let required_key = if current_finger_count == 3 {
        Key::BTN_TOOL_TRIPLETAP
    } else {
        Key::BTN_TOOL_QUADTAP
    };

    let mut devices: Vec<TouchpadDevice> = paths
        .iter()
        .filter_map(|p| {
            let device = Device::open(p).ok()?;
            if device.supported_keys()?.contains(required_key) {
                Some(TouchpadDevice { device })
            } else {
                None
            }
        })
        .collect();

    if devices.is_empty() {
        eprintln!("No touchpad devices available for gesture detection");
        return;
    }

    println!(
        "Gesture detection started with {} touchpad(s) ({}-finger tap)",
        devices.len(),
        current_finger_count
    );

    loop {
        // Periodically reload config from disk (for settings changes from subprocess)
        if last_config_check.elapsed() > config_check_interval {
            let new_cfg = GestureConfig::from(&PieMenuConfig::load());
            if new_cfg.finger_count != current_cfg.finger_count
                || new_cfg.tap_max_duration != current_cfg.tap_max_duration
                || new_cfg.tap_max_movement != current_cfg.tap_max_movement
            {
                println!(
                    "Config changed: {} fingers, {}ms duration, {} movement",
                    new_cfg.finger_count,
                    new_cfg.tap_max_duration.as_millis(),
                    new_cfg.tap_max_movement
                );
                current_cfg = new_cfg;
                // Update shared config
                if let Ok(mut shared) = config.write() {
                    *shared = current_cfg.clone();
                }
            }
            last_config_check = Instant::now();
        }

        let cfg = &current_cfg;

        // Check if finger count changed - need to rescan devices
        if cfg.finger_count != current_finger_count {
            println!(
                "Finger count changed from {} to {}, rescanning devices...",
                current_finger_count, cfg.finger_count
            );
            current_finger_count = cfg.finger_count;
            devices.clear();
            last_scan = Instant::now() - rescan_interval; // Force immediate rescan
        }

        // Periodic device rescan (for hotplug support)
        if last_scan.elapsed() > rescan_interval || devices.is_empty() {
            let paths = find_touchpad_paths(current_finger_count);
            let required_key = if current_finger_count == 3 {
                Key::BTN_TOOL_TRIPLETAP
            } else {
                Key::BTN_TOOL_QUADTAP
            };

            let new_devices: Vec<TouchpadDevice> = paths
                .iter()
                .filter_map(|p| {
                    let device = Device::open(p).ok()?;
                    if device.supported_keys()?.contains(required_key) {
                        Some(TouchpadDevice { device })
                    } else {
                        None
                    }
                })
                .collect();

            if !new_devices.is_empty() {
                devices = new_devices;
                println!("Rescanned: found {} touchpad(s)", devices.len());
            } else if devices.is_empty() {
                // No devices available, wait before rescanning
                std::thread::sleep(Duration::from_secs(5));
            }
            last_scan = Instant::now();
            continue;
        }

        // Track if any device had an error (for rescan)
        let mut needs_rescan = false;

        // Process events from all devices
        for touchpad in &mut devices {
            // fetch_events() is blocking but has a timeout in evdev
            match touchpad.device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        match process_event(
                            &event,
                            &mut state,
                            cfg.finger_count,
                            cfg.tap_max_duration,
                            cfg.tap_max_movement,
                        ) {
                            GestureEvent::FingersDown => {
                                println!("{} fingers down - icon highlighted", cfg.finger_count);
                                // Trigger visual feedback on tray icon
                                feedback.trigger();
                            }
                            GestureEvent::FingersUp => {
                                println!("{} fingers up - launching menu", cfg.finger_count);
                                // Send message to trigger pie menu at cursor position
                                if tx
                                    .send(TrayMessage::ShowPieMenu { x: 0, y: 0 })
                                    .is_err()
                                {
                                    // Channel closed, exit loop
                                    return;
                                }
                            }
                            GestureEvent::TriggerCancelled | GestureEvent::SwipeDetected => {
                                // Gesture cancelled or swipe detected, reset icon
                                feedback.reset();
                            }
                            GestureEvent::None => {}
                        }
                    }
                }
                Err(e) => {
                    // Device might have been disconnected
                    if e.raw_os_error() == Some(libc::ENODEV) {
                        eprintln!("Touchpad disconnected, will rescan...");
                        needs_rescan = true;
                    }
                }
            }
        }

        // Check for pending trigger timeout (3-finger mode debounce)
        if check_pending_trigger(&mut state) {
            println!("{} finger tap confirmed - launching menu", cfg.finger_count);
            if tx.send(TrayMessage::ShowPieMenu { x: 0, y: 0 }).is_err() {
                return;
            }
        }

        // Clear devices if rescan needed (outside the borrow)
        if needs_rescan {
            devices.clear();
        }
    }
}

/// Start the gesture detection thread with configurable parameters
///
/// Returns an error if no touchpad devices are found or if permission is denied.
/// The gesture detection runs in a background thread and sends `TrayMessage::ShowPieMenu`
/// when a multi-finger tap is detected.
///
/// The `config` parameter provides shared configuration that can be updated at runtime
/// for hot-reload support.
pub fn start_gesture_thread(
    tx: Sender<TrayMessage>,
    feedback: GestureFeedback,
    config: SharedConfig,
) -> Result<(), GestureError> {
    // Read initial finger count from config
    let finger_count = config.read().map(|c| c.finger_count).unwrap_or(4);

    // Find touchpad devices
    let paths = find_touchpad_paths(finger_count);

    if paths.is_empty() {
        // Check if it's a permission issue by trying to read /dev/input directly
        match std::fs::read_dir("/dev/input") {
            Ok(mut dir) => {
                // Can read directory, check if we can open any event device
                if let Some(Ok(entry)) = dir.find(|e| {
                    e.as_ref()
                        .map(|e| e.path().to_string_lossy().contains("event"))
                        .unwrap_or(false)
                }) {
                    let path = entry.path();
                    match Device::open(&path) {
                        Err(e) if e.raw_os_error() == Some(libc::EACCES) => {
                            return Err(GestureError::PermissionDenied(path.display().to_string()));
                        }
                        _ => {}
                    }
                }
            }
            Err(_) => {}
        }
        return Err(GestureError::NoTouchpadFound);
    }

    // Try to open first device to check permissions
    match Device::open(&paths[0]) {
        Ok(_) => {}
        Err(e) if e.raw_os_error() == Some(libc::EACCES) => {
            return Err(GestureError::PermissionDenied(paths[0].display().to_string()));
        }
        Err(e) => {
            return Err(GestureError::DeviceError(e.to_string()));
        }
    }

    // Spawn the detection thread
    std::thread::Builder::new()
        .name("gesture-detector".to_string())
        .spawn(move || gesture_loop(tx, feedback, config))
        .map_err(|e| GestureError::ThreadError(e.to_string()))?;

    Ok(())
}
