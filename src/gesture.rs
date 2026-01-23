//! Touchpad gesture detection module
//!
//! Detects four-finger press/release on touchpads using Linux evdev and triggers
//! the pie menu. Requires user to be in the 'input' group.

use evdev::{AbsoluteAxisType, Device, InputEventKind, Key};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::tray::{GestureFeedback, TrayMessage};

/// Errors that can occur during gesture detection setup
#[derive(Debug)]
pub enum GestureError {
    /// No touchpad devices found with four-finger tap support
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
                write!(f, "No touchpad with four-finger tap support found")
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

/// Maximum duration for a tap gesture (longer = swipe, not tap)
const TAP_MAX_DURATION: Duration = Duration::from_millis(250);

/// Maximum position change for a tap (in touchpad units)
/// Swipes involve significant movement, taps are mostly stationary
/// Set high enough to allow for slightly "unclean" finger placement
const TAP_MAX_MOVEMENT: i32 = 500;

/// State machine for tracking four-finger gesture
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureState {
    /// Waiting for fingers down
    Idle,
    /// Four fingers are down, tracking time and position
    FingersDown {
        start: Instant,
        /// Starting X position on touchpad
        start_x: Option<i32>,
        /// Starting Y position on touchpad
        start_y: Option<i32>,
        /// Maximum distance moved from start
        max_movement: i32,
    },
}

/// Find all touchpad device paths in /dev/input/
fn find_touchpad_paths() -> Vec<PathBuf> {
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

        // Check if it's a touchpad with four-finger support
        if is_touchpad_with_tripletap(&device) {
            println!(
                "Found touchpad with four-finger support: {} ({})",
                device.name().unwrap_or("Unknown"),
                path.display()
            );
            touchpads.push(path);
        }
    }

    touchpads
}

/// Check if a device is a touchpad with four-finger tap capability
fn is_touchpad_with_tripletap(device: &Device) -> bool {
    // Must have BTN_TOOL_TRIPLETAP in supported keys
    let keys = match device.supported_keys() {
        Some(k) => k,
        None => return false,
    };

    if !keys.contains(Key::BTN_TOOL_QUADTAP) {
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

/// Events returned from process_event
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureEvent {
    None,
    FingersDown,
    FingersUp,
}

/// Process a single input event, returning gesture events
fn process_event(event: &evdev::InputEvent, state: &mut GestureState) -> GestureEvent {
    match event.kind() {
        InputEventKind::Key(Key::BTN_TOOL_QUADTAP) => {
            if event.value() == 1 {
                // Four fingers went down - record the time, will capture position on first touch event
                *state = GestureState::FingersDown {
                    start: Instant::now(),
                    start_x: None,
                    start_y: None,
                    max_movement: 0,
                };
                return GestureEvent::FingersDown;
            } else if event.value() == 0 {
                // Four fingers lifted - check if it was a quick tap (not a swipe)
                if let GestureState::FingersDown { start, max_movement, .. } = *state {
                    *state = GestureState::Idle;
                    let duration = start.elapsed();

                    if duration <= TAP_MAX_DURATION && max_movement <= TAP_MAX_MOVEMENT {
                        // Quick tap with little movement - trigger menu
                        return GestureEvent::FingersUp;
                    } else {
                        // Swipe gesture - ignore
                        println!(
                            "Gesture ignored (duration: {:?}, movement: {}) - likely a swipe",
                            duration, max_movement
                        );
                    }
                }
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

/// Wrapper to hold device
struct TouchpadDevice {
    device: Device,
}

/// Main gesture detection loop
fn gesture_loop(tx: Sender<TrayMessage>, feedback: GestureFeedback) {
    let mut state = GestureState::Idle;
    let mut last_scan = Instant::now();
    let rescan_interval = Duration::from_secs(30);

    // Initial device scan
    let paths = find_touchpad_paths();
    let mut devices: Vec<TouchpadDevice> = paths
        .iter()
        .filter_map(|p| {
            let device = Device::open(p).ok()?;
            if device.supported_keys()?.contains(Key::BTN_TOOL_QUADTAP) {
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
        "Gesture detection started with {} touchpad(s)",
        devices.len()
    );

    loop {
        // Periodic device rescan (for hotplug support)
        if last_scan.elapsed() > rescan_interval || devices.is_empty() {
            let paths = find_touchpad_paths();
            let new_devices: Vec<TouchpadDevice> = paths
                .iter()
                .filter_map(|p| {
                    let device = Device::open(p).ok()?;
                    if device.supported_keys()?.contains(Key::BTN_TOOL_QUADTAP) {
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
                        match process_event(&event, &mut state) {
                            GestureEvent::FingersDown => {
                                println!("Four fingers down - icon highlighted");
                                // Trigger visual feedback on tray icon
                                feedback.trigger();
                            }
                            GestureEvent::FingersUp => {
                                println!("Four fingers up - launching menu");
                                // Send message to trigger pie menu at cursor position
                                if tx
                                    .send(TrayMessage::ShowPieMenu { x: 0, y: 0 })
                                    .is_err()
                                {
                                    // Channel closed, exit loop
                                    return;
                                }
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

        // Clear devices if rescan needed (outside the borrow)
        if needs_rescan {
            devices.clear();
        }
    }
}

/// Start the gesture detection thread
///
/// Returns an error if no touchpad devices are found or if permission is denied.
/// The gesture detection runs in a background thread and sends `TrayMessage::ShowPieMenu`
/// when a four-finger tap is detected.
pub fn start_gesture_thread(tx: Sender<TrayMessage>, feedback: GestureFeedback) -> Result<(), GestureError> {
    // Find touchpad devices
    let paths = find_touchpad_paths();

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
        .spawn(move || gesture_loop(tx, feedback))
        .map_err(|e| GestureError::ThreadError(e.to_string()))?;

    Ok(())
}
