//! Running Windows Detection Module
//!
//! Uses the ext-foreign-toplevel-list Wayland protocol to detect
//! which applications currently have open windows.
//!
//! Also provides window activation using zcosmic_toplevel_manager_v1.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wayland_client::{
    Connection, Dispatch, QueueHandle, Proxy,
    protocol::wl_registry::{self, WlRegistry},
    protocol::wl_seat::{self, WlSeat},
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
};
use cosmic_protocols::toplevel_info::v1::client::{
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
    zcosmic_toplevel_handle_v1::{self, ZcosmicToplevelHandleV1},
};
use cosmic_protocols::toplevel_management::v1::client::{
    zcosmic_toplevel_manager_v1::{self, ZcosmicToplevelManagerV1},
};

/// State for tracking running windows
struct ToplevelState {
    /// Map of app_ids to window count for currently running applications
    running_apps: Arc<Mutex<HashMap<String, u32>>>,
    /// Current app_id being built for a handle
    pending_app_ids: std::collections::HashMap<u32, String>,
    /// Whether the foreign toplevel list was found
    manager_bound: bool,
}

impl ToplevelState {
    fn new(running_apps: Arc<Mutex<HashMap<String, u32>>>) -> Self {
        Self {
            running_apps,
            pending_app_ids: std::collections::HashMap::new(),
            manager_bound: false,
        }
    }
}

impl Dispatch<WlRegistry, ()> for ToplevelState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == "ext_foreign_toplevel_list_v1" {
                registry.bind::<ExtForeignToplevelListV1, _, _>(
                    name,
                    version.min(1),
                    qh,
                    (),
                );
                state.manager_bound = true;
            }
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for ToplevelState {
    fn event(
        _state: &mut Self,
        _list: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_list_v1::Event::Toplevel { .. } => {}
            ext_foreign_toplevel_list_v1::Event::Finished => {}
            _ => {}
        }
    }

    // Handle creation of child toplevel handles
    wayland_client::event_created_child!(ToplevelState, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for ToplevelState {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let handle_id = handle.id().protocol_id();

        match event {
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.pending_app_ids.insert(handle_id, app_id);
            }
            ext_foreign_toplevel_handle_v1::Event::Title { .. } => {}
            ext_foreign_toplevel_handle_v1::Event::Done => {
                if let Some(app_id) = state.pending_app_ids.get(&handle_id) {
                    if let Ok(mut running) = state.running_apps.lock() {
                        *running.entry(app_id.clone()).or_insert(0) += 1;
                    }
                }
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                if let Some(app_id) = state.pending_app_ids.remove(&handle_id) {
                    if let Ok(mut running) = state.running_apps.lock() {
                        if let Some(count) = running.get_mut(&app_id) {
                            *count = count.saturating_sub(1);
                            if *count == 0 {
                                running.remove(&app_id);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Get a snapshot of currently running application IDs with window counts
pub fn get_running_apps() -> HashMap<String, u32> {
    // Run the Wayland query in a separate scope to ensure cleanup
    let result = query_running_apps();

    // Small delay to ensure Wayland resources are fully released
    std::thread::sleep(std::time::Duration::from_millis(50));

    result
}

fn query_running_apps() -> HashMap<String, u32> {
    let running_apps = Arc::new(Mutex::new(HashMap::new()));

    // Try to connect to Wayland
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut state = ToplevelState::new(running_apps.clone());

    // Get the registry
    let _registry = display.get_registry(&qh, ());

    // Roundtrip to get globals
    if event_queue.roundtrip(&mut state).is_err() {
        return HashMap::new();
    }

    // Another roundtrip to get toplevel info
    if event_queue.roundtrip(&mut state).is_err() {
        return HashMap::new();
    }

    // One more to ensure all Done events are received
    let _ = event_queue.roundtrip(&mut state);

    // Explicitly drop to release Wayland resources
    drop(event_queue);
    drop(conn);

    // Return the collected app IDs with counts
    match Arc::try_unwrap(running_apps) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

// ============================================================================
// Window Activation
// ============================================================================

/// State for window activation
struct ActivationState {
    /// Target app_id to activate
    target_app_id: String,
    /// Found toplevel handle matching the app_id (COSMIC handle)
    found_handle: Option<ZcosmicToplevelHandleV1>,
    /// The COSMIC toplevel manager
    manager: Option<ZcosmicToplevelManagerV1>,
    /// The seat (for activation request)
    seat: Option<WlSeat>,
    /// Current app_id being built for a handle (keyed by protocol ID)
    pending_app_ids: std::collections::HashMap<u32, String>,
}

impl ActivationState {
    fn new(target_app_id: String) -> Self {
        Self {
            target_app_id,
            found_handle: None,
            manager: None,
            seat: None,
            pending_app_ids: std::collections::HashMap::new(),
        }
    }
}

impl Dispatch<WlRegistry, ()> for ActivationState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "zcosmic_toplevel_info_v1" => {
                    registry.bind::<ZcosmicToplevelInfoV1, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    );
                }
                "zcosmic_toplevel_manager_v1" => {
                    state.manager = Some(registry.bind::<ZcosmicToplevelManagerV1, _, _>(
                        name,
                        version.min(2),
                        qh,
                        (),
                    ));
                }
                "wl_seat" => {
                    if state.seat.is_none() {
                        state.seat = Some(registry.bind::<WlSeat, _, _>(
                            name,
                            version.min(1),
                            qh,
                            (),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ZcosmicToplevelInfoV1, ()> for ActivationState {
    fn event(
        _state: &mut Self,
        _info: &ZcosmicToplevelInfoV1,
        _event: zcosmic_toplevel_info_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Events are handled in the handle dispatch
    }

    wayland_client::event_created_child!(ActivationState, ZcosmicToplevelInfoV1, [
        zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE => (ZcosmicToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZcosmicToplevelHandleV1, ()> for ActivationState {
    fn event(
        state: &mut Self,
        handle: &ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let handle_id = handle.id().protocol_id();

        match event {
            zcosmic_toplevel_handle_v1::Event::AppId { app_id } => {
                state.pending_app_ids.insert(handle_id, app_id);
            }
            zcosmic_toplevel_handle_v1::Event::Done => {
                if let Some(app_id) = state.pending_app_ids.get(&handle_id) {
                    // Check if this matches our target (case-insensitive for flexibility)
                    if app_id.eq_ignore_ascii_case(&state.target_app_id)
                        && state.found_handle.is_none()
                    {
                        state.found_handle = Some(handle.clone());
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZcosmicToplevelManagerV1, ()> for ActivationState {
    fn event(
        _state: &mut Self,
        _manager: &ZcosmicToplevelManagerV1,
        _event: zcosmic_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // We don't need to handle capabilities - just try to activate
    }
}

impl Dispatch<WlSeat, ()> for ActivationState {
    fn event(
        _state: &mut Self,
        _seat: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // We don't need seat events, just the seat object
    }
}

/// Activate a window by app_id using zcosmic_toplevel_manager_v1
///
/// Returns:
/// - Ok(true) if activation was requested
/// - Ok(false) if no matching window found
/// - Err if protocol not supported
pub fn activate_window_by_app_id(app_id: &str) -> Result<bool, String> {
    let conn = Connection::connect_to_env()
        .map_err(|e| format!("Wayland connection failed: {}", e))?;

    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut state = ActivationState::new(app_id.to_string());

    // Get the registry
    let _registry = display.get_registry(&qh, ());

    // Roundtrip to get globals (including manager and seat)
    event_queue.roundtrip(&mut state)
        .map_err(|e| format!("Roundtrip failed: {}", e))?;

    // Another roundtrip to get toplevel info
    event_queue.roundtrip(&mut state)
        .map_err(|e| format!("Roundtrip failed: {}", e))?;

    // One more to ensure all Done events are received
    let _ = event_queue.roundtrip(&mut state);

    // Check if we have the necessary protocol support
    let manager = state.manager.as_ref()
        .ok_or_else(|| "zcosmic_toplevel_manager_v1 not supported (COSMIC-specific feature)".to_string())?;
    let seat = state.seat.as_ref()
        .ok_or_else(|| "No seat available".to_string())?;

    // Check if we found a matching window
    let handle = match state.found_handle {
        Some(ref h) => h,
        None => return Ok(false),
    };

    // Request activation
    manager.activate(handle, seat);

    // Roundtrip to process the activation
    let _ = event_queue.roundtrip(&mut state);

    // Small delay to ensure activation completes
    std::thread::sleep(std::time::Duration::from_millis(50));

    Ok(true)
}
