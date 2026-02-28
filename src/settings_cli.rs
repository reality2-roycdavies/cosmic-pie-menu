//! CLI settings protocol for cosmic-applet-settings hub integration.

use crate::config::{PieMenuConfig, SwipeAction, WorkspaceLayout, read_workspace_layout};

pub fn describe() {
    let config = PieMenuConfig::load();
    let layout = read_workspace_layout();

    let swipe_options = serde_json::json!([
        {"value": "None", "label": "None (System Default)"},
        {"value": "AppLibrary", "label": "App Library"},
        {"value": "Launcher", "label": "Launcher"},
        {"value": "Workspaces", "label": "Workspaces"},
        {"value": "PieMenu", "label": "Pie Menu"}
    ]);

    let mut swipe_items = vec![];
    match layout {
        WorkspaceLayout::Horizontal => {
            swipe_items.push(serde_json::json!({
                "type": "select",
                "key": "swipe_up",
                "label": "Swipe Up",
                "value": swipe_to_str(config.swipe_up),
                "options": swipe_options
            }));
            swipe_items.push(serde_json::json!({
                "type": "select",
                "key": "swipe_down",
                "label": "Swipe Down",
                "value": swipe_to_str(config.swipe_down),
                "options": swipe_options
            }));
        }
        WorkspaceLayout::Vertical => {
            swipe_items.push(serde_json::json!({
                "type": "select",
                "key": "swipe_left",
                "label": "Swipe Left",
                "value": swipe_to_str(config.swipe_left),
                "options": swipe_options
            }));
            swipe_items.push(serde_json::json!({
                "type": "select",
                "key": "swipe_right",
                "label": "Swipe Right",
                "value": swipe_to_str(config.swipe_right),
                "options": swipe_options
            }));
        }
    }

    swipe_items.push(serde_json::json!({
        "type": "slider",
        "key": "swipe_threshold",
        "label": "Swipe Threshold",
        "value": config.swipe_threshold as f64,
        "min": 100.0,
        "max": 600.0,
        "step": 50.0,
        "unit": ""
    }));

    let schema = serde_json::json!({
        "title": "Pie Menu Settings",
        "description": "Configure gesture detection and appearance for the radial app launcher.",
        "sections": [
            {
                "title": "Gesture Detection",
                "items": [
                    {
                        "type": "select",
                        "key": "finger_count",
                        "label": "Finger Count",
                        "value": config.finger_count.to_string(),
                        "options": [
                            {"value": "3", "label": "3 Fingers"},
                            {"value": "4", "label": "4 Fingers"}
                        ]
                    },
                    {
                        "type": "slider",
                        "key": "tap_duration_ms",
                        "label": "Tap Duration",
                        "value": config.tap_duration_ms as f64,
                        "min": 100.0,
                        "max": 500.0,
                        "step": 10.0,
                        "unit": "ms"
                    },
                    {
                        "type": "slider",
                        "key": "tap_movement",
                        "label": "Tap Movement Threshold",
                        "value": config.tap_movement as f64,
                        "min": 200.0,
                        "max": 1000.0,
                        "step": 50.0,
                        "unit": ""
                    },
                    {
                        "type": "toggle",
                        "key": "middle_click_trigger",
                        "label": "Middle Click Trigger",
                        "value": config.middle_click_trigger
                    }
                ]
            },
            {
                "title": "Swipe Actions",
                "items": swipe_items
            },
            {
                "title": "Appearance",
                "items": [
                    {
                        "type": "toggle",
                        "key": "show_background",
                        "label": "Show Background",
                        "value": config.show_background
                    },
                    {
                        "type": "toggle",
                        "key": "icon_only_highlight",
                        "label": "Icon-Only Highlight",
                        "value": config.icon_only_highlight
                    },
                    {
                        "type": "slider",
                        "key": "icon_size",
                        "label": "Icon Size",
                        "value": config.icon_size as f64,
                        "min": 24.0,
                        "max": 96.0,
                        "step": 4.0,
                        "unit": "px"
                    },
                    {
                        "type": "slider",
                        "key": "icon_spacing",
                        "label": "Icon Spacing",
                        "value": config.icon_spacing as f64,
                        "min": 50.0,
                        "max": 120.0,
                        "step": 5.0,
                        "unit": ""
                    },
                    {
                        "type": "slider",
                        "key": "hover_offset",
                        "label": "Selection Offset",
                        "value": config.hover_offset as f64,
                        "min": 0.0,
                        "max": 60.0,
                        "step": 5.0,
                        "unit": "px"
                    },
                    {
                        "type": "slider",
                        "key": "animation_speed",
                        "label": "Animation Speed",
                        "value": config.animation_speed as f64,
                        "min": 0.05,
                        "max": 0.5,
                        "step": 0.05,
                        "unit": ""
                    }
                ]
            }
        ],
        "actions": [
            {"id": "reset", "label": "Reset to Defaults", "style": "destructive"}
        ]
    });

    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}

pub fn set(key: &str, value: &str) {
    let mut config = PieMenuConfig::load();

    let result: Result<&str, String> = (|| -> Result<&str, String> {
        match key {
            "finger_count" => {
                let v: String = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid value: {e}"))?;
                match v.as_str() {
                    "3" => { config.finger_count = 3; Ok("Updated finger count") }
                    "4" => { config.finger_count = 4; Ok("Updated finger count") }
                    _ => Err("Finger count must be 3 or 4".to_string()),
                }
            }
            "tap_duration_ms" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.tap_duration_ms = v as u64;
                Ok("Updated tap duration")
            }
            "tap_movement" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.tap_movement = v as i32;
                Ok("Updated tap movement")
            }
            "swipe_threshold" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.swipe_threshold = v as i32;
                Ok("Updated swipe threshold")
            }
            "middle_click_trigger" => {
                let v: bool = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid boolean: {e}"))?;
                config.middle_click_trigger = v;
                Ok("Updated middle click trigger")
            }
            "swipe_up" => {
                let v: String = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid value: {e}"))?;
                config.swipe_up = str_to_swipe(&v)?;
                Ok("Updated swipe up")
            }
            "swipe_down" => {
                let v: String = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid value: {e}"))?;
                config.swipe_down = str_to_swipe(&v)?;
                Ok("Updated swipe down")
            }
            "swipe_left" => {
                let v: String = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid value: {e}"))?;
                config.swipe_left = str_to_swipe(&v)?;
                Ok("Updated swipe left")
            }
            "swipe_right" => {
                let v: String = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid value: {e}"))?;
                config.swipe_right = str_to_swipe(&v)?;
                Ok("Updated swipe right")
            }
            "show_background" => {
                let v: bool = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid boolean: {e}"))?;
                config.show_background = v;
                Ok("Updated show background")
            }
            "icon_only_highlight" => {
                let v: bool = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid boolean: {e}"))?;
                config.icon_only_highlight = v;
                Ok("Updated icon-only highlight")
            }
            "icon_size" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.icon_size = v as u16;
                Ok("Updated icon size")
            }
            "icon_spacing" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.icon_spacing = v as f32;
                Ok("Updated icon spacing")
            }
            "hover_offset" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.hover_offset = v as f32;
                Ok("Updated selection offset")
            }
            "animation_speed" => {
                let v: f64 = serde_json::from_str(value)
                    .map_err(|e| format!("Invalid number: {e}"))?;
                config.animation_speed = v as f32;
                Ok("Updated animation speed")
            }
            _ => Err(format!("Unknown key: {key}")),
        }
    })();

    match result {
        Ok(msg) => match config.save() {
            Ok(()) => print_response(true, msg),
            Err(e) => print_response(false, &format!("Save failed: {e}")),
        },
        Err(e) => print_response(false, &e),
    }
}

pub fn action(id: &str) {
    match id {
        "reset" => {
            let config = PieMenuConfig::default();
            match config.save() {
                Ok(()) => print_response(true, "Reset to defaults"),
                Err(e) => print_response(false, &format!("Reset failed: {e}")),
            }
        }
        _ => print_response(false, &format!("Unknown action: {id}")),
    }
}

fn swipe_to_str(action: SwipeAction) -> &'static str {
    match action {
        SwipeAction::None => "None",
        SwipeAction::AppLibrary => "AppLibrary",
        SwipeAction::Launcher => "Launcher",
        SwipeAction::Workspaces => "Workspaces",
        SwipeAction::PieMenu => "PieMenu",
    }
}

fn str_to_swipe(s: &str) -> Result<SwipeAction, String> {
    match s {
        "None" => Ok(SwipeAction::None),
        "AppLibrary" => Ok(SwipeAction::AppLibrary),
        "Launcher" => Ok(SwipeAction::Launcher),
        "Workspaces" => Ok(SwipeAction::Workspaces),
        "PieMenu" => Ok(SwipeAction::PieMenu),
        _ => Err(format!("Unknown swipe action: {s}")),
    }
}

fn print_response(ok: bool, message: &str) {
    let resp = serde_json::json!({"ok": ok, "message": message});
    println!("{}", resp);
}
