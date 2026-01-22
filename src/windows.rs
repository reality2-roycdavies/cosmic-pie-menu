//! Running Windows Detection Module
//!
//! Uses the ext-foreign-toplevel-list Wayland protocol to detect
//! which applications currently have open windows.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use wayland_client::{
    Connection, Dispatch, QueueHandle, Proxy,
    protocol::wl_registry::{self, WlRegistry},
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
};

/// State for tracking running windows
struct ToplevelState {
    /// Set of app_ids for currently running applications
    running_apps: Arc<Mutex<HashSet<String>>>,
    /// Current app_id being built for a handle
    pending_app_ids: std::collections::HashMap<u32, String>,
    /// Whether the foreign toplevel list was found
    manager_bound: bool,
}

impl ToplevelState {
    fn new(running_apps: Arc<Mutex<HashSet<String>>>) -> Self {
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
                        running.insert(app_id.clone());
                    }
                }
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                if let Some(app_id) = state.pending_app_ids.remove(&handle_id) {
                    if let Ok(mut running) = state.running_apps.lock() {
                        running.remove(&app_id);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Get a snapshot of currently running application IDs
pub fn get_running_apps() -> HashSet<String> {
    // Run the Wayland query in a separate scope to ensure cleanup
    let result = query_running_apps();

    // Small delay to ensure Wayland resources are fully released
    std::thread::sleep(std::time::Duration::from_millis(50));

    result
}

fn query_running_apps() -> HashSet<String> {
    let running_apps = Arc::new(Mutex::new(HashSet::new()));

    // Try to connect to Wayland
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };

    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut state = ToplevelState::new(running_apps.clone());

    // Get the registry
    let _registry = display.get_registry(&qh, ());

    // Roundtrip to get globals
    if event_queue.roundtrip(&mut state).is_err() {
        return HashSet::new();
    }

    // Another roundtrip to get toplevel info
    if event_queue.roundtrip(&mut state).is_err() {
        return HashSet::new();
    }

    // One more to ensure all Done events are received
    let _ = event_queue.roundtrip(&mut state);

    // Explicitly drop to release Wayland resources
    drop(event_queue);
    drop(conn);

    // Return the collected app IDs
    match Arc::try_unwrap(running_apps) {
        Ok(mutex) => mutex.into_inner().unwrap_or_default(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}
