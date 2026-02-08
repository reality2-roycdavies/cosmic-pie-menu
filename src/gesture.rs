//! Touchpad gesture detection module
//!
//! Detects multi-finger tap and swipe gestures on touchpads using Linux evdev.
//! Triggers the pie menu on tap, or executes configured actions on swipe.
//!
//! # Requirements
//! - User must be in the 'input' group to access /dev/input devices
//! - Touchpad must support BTN_TOOL_TRIPLETAP (3-finger) or BTN_TOOL_QUADTAP (4-finger)
//!
//! # Features
//! - Configurable finger count (3 or 4 fingers)
//! - Configurable tap duration and movement threshold
//! - Swipe gesture detection with configurable actions per direction
//! - Early swipe detection (triggers before finger lift when threshold exceeded)
//! - Respects COSMIC workspace layout (ignores swipes used for workspace switching)
//! - Multitouch tracking with per-finger movement averaging for accurate direction detection

use evdev::{AbsoluteAxisType, Device, InputEventKind, Key};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::config::{GestureConfig, PieMenuConfig, SharedConfig, SwipeAction, WorkspaceLayout, read_workspace_layout};
use crate::applet::GestureMessage;
use std::process::Command;

/// Maximum number of touch slots to track (most touchpads support up to 5-10)
const MAX_SLOTS: usize = 10;

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

/// Tracks position for a single touch slot
#[derive(Debug, Clone, Copy, Default)]
struct TouchSlot {
    /// Whether this slot is currently active (finger touching)
    active: bool,
    /// Current X position
    x: i32,
    /// Current Y position
    y: i32,
    /// Starting X position (when finger first touched)
    start_x: Option<i32>,
    /// Starting Y position (when finger first touched)
    start_y: Option<i32>,
}

/// Multitouch tracker - tracks all finger positions for accurate gesture detection.
///
/// Handles the Linux multitouch protocol where each finger is assigned a slot,
/// and position events update the current slot. Tracks per-finger start positions
/// to calculate movement deltas for swipe direction detection.
#[derive(Debug, Clone)]
struct MultiTouchTracker {
    /// Current slot being updated (set by ABS_MT_SLOT events)
    current_slot: usize,
    /// Per-slot position data for up to MAX_SLOTS fingers
    slots: [TouchSlot; MAX_SLOTS],
    /// Whether we've captured enough start positions to begin tracking movement
    start_captured: bool,
    /// Time when first position event was seen (for settling time)
    first_event_time: Option<Instant>,
    /// Minimum fingers required before capturing start positions
    min_fingers_for_start: usize,
}

impl MultiTouchTracker {
    /// Create a new tracker requiring `min_fingers` before capturing start positions.
    fn new(min_fingers: usize) -> Self {
        Self {
            current_slot: 0,
            slots: [TouchSlot::default(); MAX_SLOTS],
            start_captured: false,
            first_event_time: None,
            min_fingers_for_start: min_fingers,
        }
    }
}

impl Default for MultiTouchTracker {
    fn default() -> Self {
        Self::new(3) // Default to requiring 3 fingers before capturing start
    }
}

impl MultiTouchTracker {
    /// Record that we received a position event (for settling time tracking).
    /// Called on each position update to track when gesture started.
    fn mark_event(&mut self) {
        if self.first_event_time.is_none() {
            self.first_event_time = Some(Instant::now());
        }
    }

    /// Attempt to mark that we have enough finger start positions captured.
    ///
    /// Waits until all `min_fingers_for_start` fingers have valid positions.
    /// This ensures consistent movement calculation across all fingers.
    fn try_capture_start(&mut self) {
        if self.start_captured {
            return;
        }

        let fingers_ready = self.fingers_with_start();

        if fingers_ready >= self.min_fingers_for_start {
            println!(
                "Start captured: {} fingers with valid positions",
                fingers_ready
            );
            self.start_captured = true;
        }
    }

    /// Get count of fingers with valid start positions (both X and Y captured).
    fn fingers_with_start(&self) -> usize {
        self.slots.iter()
            .filter(|s| s.active && s.start_x.is_some() && s.start_y.is_some())
            .count()
    }

    /// Calculate average movement delta across all tracked fingers.
    ///
    /// This averages the (current - start) movement for each finger individually,
    /// which is more accurate than comparing centroids when fingers may be in
    /// different slots or have different starting positions.
    ///
    /// Returns (avg_dx, avg_dy) where positive X is right, positive Y is down.
    fn average_movement(&self) -> (i32, i32) {
        let mut total_dx: i64 = 0;
        let mut total_dy: i64 = 0;
        let mut count = 0;

        for slot in &self.slots {
            if slot.active {
                if let (Some(sx), Some(sy)) = (slot.start_x, slot.start_y) {
                    total_dx += (slot.x - sx) as i64;
                    total_dy += (slot.y - sy) as i64;
                    count += 1;
                }
            }
        }

        if count == 0 {
            return (0, 0);
        }

        ((total_dx / count) as i32, (total_dy / count) as i32)
    }

    /// Calculate maximum movement from any finger's start position.
    /// Used to determine if gesture exceeds tap movement threshold.
    fn max_movement_from_start(&self) -> i32 {
        let mut max = 0;
        for slot in &self.slots {
            if slot.active {
                if let (Some(sx), Some(sy)) = (slot.start_x, slot.start_y) {
                    let dx = (slot.x - sx).abs();
                    let dy = (slot.y - sy).abs();
                    max = max.max(dx).max(dy);
                }
            }
        }
        max
    }
}

/// State machine for tracking multi-finger gesture
#[derive(Debug, Clone)]
enum GestureState {
    /// Waiting for fingers down
    Idle,
    /// Fingers are down, tracking time and position
    FingersDown {
        start: Instant,
        /// Multitouch position tracker
        tracker: MultiTouchTracker,
    },
    /// Tap detected, waiting to confirm it's not a 3→4 finger transition
    /// (only used in 3-finger mode)
    PendingTrigger {
        /// When the pending trigger was set
        pending_since: Instant,
    },
}

/// Calculate swipe direction from movement deltas
fn calculate_swipe_direction_from_delta(dx: i32, dy: i32) -> SwipeDirection {
    println!("Swipe calculation: dx={} dy={} (|dx|={} |dy|={})", dx, dy, dx.abs(), dy.abs());

    // Determine dominant axis using absolute values
    if dx.abs() > dy.abs() {
        // Horizontal swipe
        if dx > 0 {
            SwipeDirection::Right
        } else {
            SwipeDirection::Left
        }
    } else {
        // Vertical swipe
        // Note: On most touchpads, Y increases downward (like screen coords)
        // So positive dy = physical swipe down, negative dy = physical swipe up
        if dy > 0 {
            SwipeDirection::Down
        } else {
            SwipeDirection::Up
        }
    }
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

/// Direction of a swipe gesture (relative to touchpad orientation)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwipeDirection {
    /// Swipe toward top of touchpad (decreasing Y)
    Up,
    /// Swipe toward bottom of touchpad (increasing Y)
    Down,
    /// Swipe toward left of touchpad (decreasing X)
    Left,
    /// Swipe toward right of touchpad (increasing X)
    Right,
}

/// Events returned from gesture event processing
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureEvent {
    /// No significant event
    None,
    /// Required number of fingers touched down
    FingersDown,
    /// Fingers lifted after a quick tap (triggers pie menu)
    FingersUp,
    /// Gesture cancelled (e.g., 3→4 finger transition detected in 3-finger mode)
    TriggerCancelled,
    /// Swipe detected - triggered immediately when movement exceeds threshold
    SwipeDetected(SwipeDirection),
}

/// Process a single evdev input event and update gesture state.
///
/// Handles key events (finger down/up) and absolute axis events (position tracking).
/// Returns a `GestureEvent` indicating what happened:
/// - `FingersDown`: Required fingers touched down, start tracking
/// - `FingersUp`: Quick tap detected (short duration, little movement)
/// - `SwipeDetected`: Movement exceeded threshold, swipe direction determined
/// - `TriggerCancelled`: Gesture was cancelled (e.g., extra finger added)
/// - `None`: No significant state change
fn process_event(
    event: &evdev::InputEvent,
    state: &mut GestureState,
    finger_count: u8,
    tap_max_duration: Duration,
    tap_max_movement: i32,
    swipe_threshold: i32,
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
                // Fingers went down - record the time and start fresh tracker
                // Require all fingers to have valid positions before calculating movement
                let min_fingers = finger_count as usize;
                *state = GestureState::FingersDown {
                    start: Instant::now(),
                    tracker: MultiTouchTracker::new(min_fingers),
                };
                return GestureEvent::FingersDown;
            } else if event.value() == 0 {
                // Fingers lifted - check if it was a quick tap (not a swipe)
                if let GestureState::FingersDown { start, ref tracker } = state.clone() {
                    let duration = start.elapsed();
                    let max_movement = tracker.max_movement_from_start();

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
                        // Swipe gesture - determine direction using average finger movement
                        let (avg_dx, avg_dy) = tracker.average_movement();
                        println!(
                            "End state: {} fingers tracked, avg movement: dx={} dy={}",
                            tracker.fingers_with_start(),
                            avg_dx, avg_dy
                        );
                        let direction = calculate_swipe_direction_from_delta(avg_dx, avg_dy);
                        println!(
                            "Swipe detected: {:?} (duration: {:?}, movement: {})",
                            direction, duration, max_movement
                        );
                        *state = GestureState::Idle;
                        return GestureEvent::SwipeDetected(direction);
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
        // Track multitouch position while fingers are down
        InputEventKind::AbsAxis(axis) => {
            if let GestureState::FingersDown { ref mut tracker, .. } = state {
                let val = event.value();
                match axis {
                    // ABS_MT_SLOT tells us which finger slot the following events apply to
                    AbsoluteAxisType::ABS_MT_SLOT => {
                        let slot = val as usize;
                        if slot < MAX_SLOTS {
                            tracker.current_slot = slot;
                        }
                    }
                    // ABS_MT_TRACKING_ID: >= 0 means finger down, -1 means finger up
                    AbsoluteAxisType::ABS_MT_TRACKING_ID => {
                        let slot = tracker.current_slot;
                        if slot < MAX_SLOTS {
                            tracker.slots[slot].active = val >= 0;
                        }
                    }
                    // Track X position for current slot
                    AbsoluteAxisType::ABS_MT_POSITION_X => {
                        let slot = tracker.current_slot;
                        if slot < MAX_SLOTS {
                            // Capture start position on first X event for this slot
                            if tracker.slots[slot].start_x.is_none() {
                                tracker.slots[slot].start_x = Some(val);
                            }
                            tracker.slots[slot].x = val;
                            tracker.slots[slot].active = true;
                            tracker.mark_event();
                            tracker.try_capture_start();

                            // Check for early swipe detection
                            if tracker.start_captured {
                                if let Some(dir) = check_early_swipe(tracker, swipe_threshold) {
                                    *state = GestureState::Idle;
                                    return GestureEvent::SwipeDetected(dir);
                                }
                            }
                        }
                    }
                    // Track Y position for current slot
                    AbsoluteAxisType::ABS_MT_POSITION_Y => {
                        let slot = tracker.current_slot;
                        if slot < MAX_SLOTS {
                            // Capture start position on first Y event for this slot
                            if tracker.slots[slot].start_y.is_none() {
                                tracker.slots[slot].start_y = Some(val);
                            }
                            tracker.slots[slot].y = val;
                            tracker.slots[slot].active = true;
                            tracker.mark_event();
                            tracker.try_capture_start();

                            // Check for early swipe detection
                            if tracker.start_captured {
                                if let Some(dir) = check_early_swipe(tracker, swipe_threshold) {
                                    *state = GestureState::Idle;
                                    return GestureEvent::SwipeDetected(dir);
                                }
                            }
                        }
                    }
                    // Fallback for non-MT touchpads (single-touch style reporting)
                    AbsoluteAxisType::ABS_X => {
                        // Use slot 0 for legacy single-touch
                        if tracker.slots[0].start_x.is_none() {
                            tracker.slots[0].start_x = Some(val);
                        }
                        tracker.slots[0].x = val;
                        tracker.slots[0].active = true;
                        tracker.mark_event();
                        tracker.try_capture_start();

                        if tracker.start_captured {
                            if let Some(dir) = check_early_swipe(tracker, swipe_threshold) {
                                *state = GestureState::Idle;
                                return GestureEvent::SwipeDetected(dir);
                            }
                        }
                    }
                    AbsoluteAxisType::ABS_Y => {
                        if tracker.slots[0].start_y.is_none() {
                            tracker.slots[0].start_y = Some(val);
                        }
                        tracker.slots[0].y = val;
                        tracker.slots[0].active = true;
                        tracker.mark_event();
                        tracker.try_capture_start();

                        if tracker.start_captured {
                            if let Some(dir) = check_early_swipe(tracker, swipe_threshold) {
                                *state = GestureState::Idle;
                                return GestureEvent::SwipeDetected(dir);
                            }
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

/// Check if finger movement exceeds threshold for early swipe detection.
///
/// Called on each position update to detect swipes before finger lift.
/// This makes swipe gestures feel more responsive.
fn check_early_swipe(tracker: &MultiTouchTracker, threshold: i32) -> Option<SwipeDirection> {
    let (avg_dx, avg_dy) = tracker.average_movement();
    let movement = avg_dx.abs().max(avg_dy.abs());

    if movement >= threshold {
        println!(
            "Early swipe detected: {} fingers, avg movement: dx={} dy={}, threshold={}",
            tracker.fingers_with_start(), avg_dx, avg_dy, threshold
        );
        Some(calculate_swipe_direction_from_delta(avg_dx, avg_dy))
    } else {
        None
    }
}

/// Check if a pending trigger has timed out and should fire
fn check_pending_trigger(state: &mut GestureState) -> bool {
    if let GestureState::PendingTrigger { pending_since } = state {
        if pending_since.elapsed() >= PENDING_TRIGGER_DEBOUNCE {
            *state = GestureState::Idle;
            return true;
        }
    }
    false
}

/// Simple wrapper to hold device
struct TouchpadDevice {
    device: Device,
}

/// Main gesture detection loop with configurable parameters
fn gesture_loop(tx: Sender<GestureMessage>, config: SharedConfig) {
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

    // Track the last opened overlay (for opposite-direction closing)
    // Stores (action, direction) so we know what to close and which direction opened it
    let mut last_opened: Option<(SwipeAction, SwipeDirection)> = None;

    loop {
        // Periodically reload config from disk (for settings changes from subprocess)
        if last_config_check.elapsed() > config_check_interval {
            let new_cfg = GestureConfig::from(&PieMenuConfig::load());
            // Always update config to pick up swipe action changes
            let config_changed = new_cfg.finger_count != current_cfg.finger_count
                || new_cfg.tap_max_duration != current_cfg.tap_max_duration
                || new_cfg.tap_max_movement != current_cfg.tap_max_movement;

            if config_changed {
                println!(
                    "Config changed: {} fingers, {}ms duration, {} movement",
                    new_cfg.finger_count,
                    new_cfg.tap_max_duration.as_millis(),
                    new_cfg.tap_max_movement
                );
            }

            // Always update to get latest swipe actions
            current_cfg = new_cfg;
            // Update shared config
            if let Ok(mut shared) = config.write() {
                *shared = current_cfg.clone();
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

        // Only rescan when we have no devices (hotplug support)
        // Don't rescan periodically when we have working devices - that breaks the grab
        if devices.is_empty() && last_scan.elapsed() > Duration::from_secs(5) {
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
            } else {
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
            match touchpad.device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        match process_event(
                            &event,
                            &mut state,
                            cfg.finger_count,
                            cfg.tap_max_duration,
                            cfg.tap_max_movement,
                            cfg.swipe_threshold,
                        ) {
                            GestureEvent::FingersDown => {
                                println!("{} fingers down - icon highlighted", cfg.finger_count);
                                let _ = tx.send(GestureMessage::FingersDown);
                            }
                            GestureEvent::FingersUp => {
                                println!("{} fingers up - launching menu", cfg.finger_count);
                                if tx.send(GestureMessage::ShowPieMenu).is_err() {
                                    return;
                                }
                            }
                            GestureEvent::TriggerCancelled => {
                                let _ = tx.send(GestureMessage::Reset);
                            }
                            GestureEvent::SwipeDetected(direction) => {
                                let _ = tx.send(GestureMessage::Reset);

                                // Check workspace layout - only allow actions for available directions
                                let layout = read_workspace_layout();
                                let direction_allowed = match layout {
                                    // Horizontal workspaces: left/right used by system, up/down available
                                    WorkspaceLayout::Horizontal => matches!(direction, SwipeDirection::Up | SwipeDirection::Down),
                                    // Vertical workspaces: up/down used by system, left/right available
                                    WorkspaceLayout::Vertical => matches!(direction, SwipeDirection::Left | SwipeDirection::Right),
                                };

                                if !direction_allowed {
                                    println!(
                                        "Swipe {:?} ignored - direction used by system for {:?} workspace switching",
                                        direction, layout
                                    );
                                    continue;
                                }

                                // Check if something is already open - any swipe closes it
                                let (action_to_run, is_closing) = if let Some((prev_action, prev_dir)) = last_opened {
                                    // Something is open - close it with any swipe direction
                                    println!(
                                        "Swipe {:?} while {:?} open (opened with {:?}) - closing",
                                        direction, prev_action, prev_dir
                                    );
                                    (prev_action, true)
                                } else {
                                    // Nothing open - get configured action for this direction
                                    let action = match direction {
                                        SwipeDirection::Up => cfg.swipe_up,
                                        SwipeDirection::Down => cfg.swipe_down,
                                        SwipeDirection::Left => cfg.swipe_left,
                                        SwipeDirection::Right => cfg.swipe_right,
                                    };
                                    (action, false)
                                };

                                println!("Action: {:?}, closing={}", action_to_run, is_closing);

                                // Execute the action
                                match action_to_run {
                                    SwipeAction::None => {
                                        // Nothing configured - do nothing
                                    }
                                    SwipeAction::PieMenu => {
                                        // Pie menu doesn't need toggle tracking
                                        println!("Swipe {:?} - launching pie menu", direction);
                                        last_opened = None;
                                        if tx.send(GestureMessage::ShowPieMenu).is_err() {
                                            return;
                                        }
                                    }
                                    _ => {
                                        // Execute the command (toggles the overlay)
                                        if let Some(cmd) = action_to_run.command() {
                                            println!(
                                                "Swipe {:?} - {} {}",
                                                direction,
                                                if is_closing { "closing" } else { "opening" },
                                                cmd
                                            );

                                            // Get display env vars for GUI commands
                                            let wayland = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
                                            let xdg_runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();

                                            let spawn_result = Command::new(cmd)
                                                .env("WAYLAND_DISPLAY", &wayland)
                                                .env("XDG_RUNTIME_DIR", &xdg_runtime)
                                                .spawn()
                                                .or_else(|_| {
                                                    // Try with full path if simple command failed
                                                    let full_path = format!("/usr/bin/{}", cmd);
                                                    Command::new(&full_path)
                                                        .env("WAYLAND_DISPLAY", &wayland)
                                                        .env("XDG_RUNTIME_DIR", &xdg_runtime)
                                                        .spawn()
                                                });

                                            match spawn_result {
                                                Ok(child) => {
                                                    println!("Successfully spawned {} (pid {})", cmd, child.id());
                                                    // Update state: if closing, clear; if opening, record
                                                    if is_closing {
                                                        last_opened = None;
                                                    } else {
                                                        last_opened = Some((action_to_run, direction));
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("Failed to spawn {}: {}", cmd, e);
                                                }
                                            }
                                        }
                                    }
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

        // Check for pending trigger timeout (3-finger mode debounce)
        if check_pending_trigger(&mut state) {
            println!("{} finger tap confirmed - launching menu", cfg.finger_count);
            if tx.send(GestureMessage::ShowPieMenu).is_err() {
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
    tx: Sender<GestureMessage>,
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
        .spawn(move || gesture_loop(tx, config))
        .map_err(|e| GestureError::ThreadError(e.to_string()))?;

    Ok(())
}
