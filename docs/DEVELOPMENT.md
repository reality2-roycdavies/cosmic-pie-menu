# Development Notes

This document captures learnings and solutions discovered while developing cosmic-pie-menu for the COSMIC desktop environment. It serves as both documentation and an educational resource for developers working with similar technologies.

## Table of Contents

1. [Canvas-Based Radial Layout](#canvas-based-radial-layout)
2. [Layer-Shell for Overlay Windows](#layer-shell-for-overlay-windows)
3. [Transparent Window Background](#transparent-window-background)
4. [Scaled Display Handling](#scaled-display-handling)
5. [Icon Discovery and Rendering](#icon-discovery-and-rendering)
6. [Mouse Interaction in Canvas](#mouse-interaction-in-canvas)
7. [System Tray Integration](#system-tray-integration)
8. [Wayland Cursor Position Challenge](#wayland-cursor-position-challenge)
9. [Running App Detection](#running-app-detection)
10. [Full-Screen Layer Surface for Stability](#full-screen-layer-surface-for-stability)
11. [Desktop File Matching](#desktop-file-matching)
12. [Dock Applet Integration](#dock-applet-integration)
13. [Theme-Aware Tray Icon](#theme-aware-tray-icon)
14. [Dynamic Icon Positioning](#dynamic-icon-positioning)
15. [Automatic Autostart](#automatic-autostart)
16. [Icon Theme Search Order](#icon-theme-search-order)
17. [Touchpad Gesture Detection](#touchpad-gesture-detection)
18. [Resources](#resources)

---

## Canvas-Based Radial Layout

### The Challenge

Creating a true circular pie menu where app icons are positioned around a circle with radial segment highlighting—not achievable with standard row/column layouts.

### Solution: iced Canvas with Trigonometry

We use iced's canvas widget for complete control over rendering:

```rust
// Calculate position on circle for each app
let slice_angle = 2.0 * PI / num_apps as f32;
let angle = -PI / 2.0 + (i as f32 * slice_angle);  // Start from top

let icon_radius = (MENU_RADIUS + INNER_RADIUS) / 2.0 + 15.0;
let icon_pos = Point::new(
    center.x + icon_radius * angle.cos(),
    center.y + icon_radius * angle.sin(),
);
```

### Drawing Annular (Ring) Segments

Standard `arc()` functions didn't produce the expected shapes. Solution: approximate arcs with line segments:

```rust
let segment = Path::new(|builder| {
    builder.move_to(outer_start);

    // Draw outer arc using small line segments
    let steps = 20;
    let angle_step = (slice.end_angle - slice.start_angle) / steps as f32;
    for i in 1..=steps {
        let angle = slice.start_angle + angle_step * i as f32;
        let point = Point::new(
            center.x + outer_radius * angle.cos(),
            center.y + outer_radius * angle.sin(),
        );
        builder.line_to(point);
    }

    // Line to inner edge, then inner arc back
    builder.line_to(inner_end);
    for i in (0..steps).rev() {
        let angle = slice.start_angle + angle_step * i as f32;
        let point = Point::new(
            center.x + inner_radius * angle.cos(),
            center.y + inner_radius * angle.sin(),
        );
        builder.line_to(point);
    }
    builder.close();
});
```

**Key insight:** When arc drawing behaves unexpectedly, manual line-segment approximation gives predictable results and is fast enough for UI purposes.

---

## Layer-Shell for Overlay Windows

### The Challenge

Create a floating overlay window that appears above all other windows, without window decorations, and can be dismissed easily.

### Solution: Wayland Layer-Shell Protocol

COSMIC/libcosmic provides layer-shell support through `SctkLayerSurfaceSettings`:

```rust
let mut settings = SctkLayerSurfaceSettings::default();
settings.keyboard_interactivity = KeyboardInteractivity::OnDemand;
settings.layer = Layer::Top;
settings.size = Some((Some(window_size), Some(window_size)));
settings.size_limits = Limits::NONE
    .min_width(window_size as f32)
    .min_height(window_size as f32)
    .max_width(window_size as f32)
    .max_height(window_size as f32);
settings.anchor = Anchor::empty();  // Centered, not anchored to edges

// Create the surface
get_layer_surface(settings)
```

**Key insight:** `Anchor::empty()` creates a floating, centered surface. Anchoring to edges would stretch or position the surface differently.

---

## Transparent Window Background

### The Challenge

Only the circular pie menu should be visible—no square window background.

### Solution: Explicit Style with Transparent Background

The `daemon` API requires an explicit style function:

```rust
fn app_style(_state: &PieMenuApp, _theme: &Theme) -> cosmic::iced_runtime::Appearance {
    cosmic::iced_runtime::Appearance {
        background_color: Color::TRANSPARENT,
        text_color: Color::WHITE,
        icon_color: Color::WHITE,
    }
}

// Apply when creating the daemon
cosmic::iced::daemon(...)
    .style(app_style)
    .run_with(...)
```

**Key insight:** libcosmic's default appearance uses transparent background for non-maximized windows, but when using the daemon API directly, you must set it explicitly.

---

## Scaled Display Handling

### The Challenge

On displays with scale factors != 100% (e.g., 150% HiDPI), the canvas bounds start incorrect and only correct after user interaction.

### Discovery Process

Debug output revealed the issue:
```
Canvas bounds: 272x272  // Initial (wrong)
Canvas bounds: 408x408  // After mouse move (correct)
```

The ratio 272/408 ≈ 0.667 = 1/1.5, suggesting a 150% scale factor issue.

### Solution: Skip Drawing Until Bounds Correct + Tick Subscription

```rust
fn draw(&self, ..., bounds: Rectangle, ...) -> Vec<Geometry> {
    let window_size = (MENU_RADIUS * 2.0 + ICON_SIZE as f32 + 80.0) as f32;

    // Skip drawing if bounds are wrong
    if (bounds.width - window_size).abs() > 1.0 {
        return vec![];
    }

    // Proceed with drawing...
}
```

To trigger the bounds correction without user interaction, we use a tick subscription:

```rust
fn subscription(&self) -> Subscription<Message> {
    let keyboard_sub = keyboard::on_key_press(...);

    // Send ticks for first 500ms to trigger layout recalculation
    if self.tick_count < 10 {
        let tick_sub = time::every(Duration::from_millis(50)).map(|_| Message::Tick);
        Subscription::batch([keyboard_sub, tick_sub])
    } else {
        keyboard_sub
    }
}
```

**Key insight:** Timer-based events can trigger the relayout that corrects bounds on scaled displays.

---

## Icon Discovery and Rendering

### The Challenge

Find the correct icon for each app, handling:
- SVG icons (COSMIC apps)
- PNG icons (traditional apps)
- Flatpak-specific paths
- Alternate naming conventions

### Solution: Multi-Path Icon Lookup

```rust
pub fn find_icon_path(icon_name: &str, size: u16) -> Option<PathBuf> {
    // Try freedesktop-icons crate first
    if let Some(path) = freedesktop_icons::lookup(icon_name)
        .with_size(size)
        .with_cache()
        .find()
    {
        return Some(path);
    }

    // Try alternate names (e.g., brave-browser -> brave-desktop)
    let alternates = vec![
        format!("{}-desktop", icon_name),
        icon_name.replace("-browser", "-desktop"),
        icon_name.to_lowercase(),
    ];

    for alt in alternates {
        if let Some(path) = freedesktop_icons::lookup(&alt)...
    }

    // Try Flatpak paths
    let flatpak_paths = [
        format!("/var/lib/flatpak/exports/share/icons/hicolor/scalable/apps/{}.svg", icon_name),
        format!("{}/.local/share/flatpak/exports/share/icons/...", home),
    ];

    None
}
```

### Canvas Icon Rendering

iced's canvas `Frame` supports both SVG and raster images:

```rust
if let Some(ref icon_path) = slice.icon_path {
    let ext = icon_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("svg") {
        let handle = SvgHandle::from_path(icon_path);
        let svg = Svg::new(handle);
        frame.draw_svg(icon_bounds, svg);
    } else {
        let handle = ImageHandle::from_path(icon_path);
        let img = Image::new(handle);
        frame.draw_image(icon_bounds, img);
    }
}
```

**Key insight:** Canvas can render both SVG and raster images directly using `draw_svg()` and `draw_image()`.

---

## Mouse Interaction in Canvas

### The Challenge

Detect which pie segment the mouse is hovering over, and handle clicks appropriately.

### Solution: Angle-Based Hit Detection

The canvas `Program` trait's `update` method receives mouse events:

```rust
fn update(&self, _state: &mut (), event: Event, bounds: Rectangle, cursor: mouse::Cursor)
    -> (canvas::event::Status, Option<Message>)
{
    let cursor_pos = cursor.position_in(bounds)?;
    let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);

    let dx = cursor_pos.x - center.x;
    let dy = cursor_pos.y - center.y;
    let distance = (dx * dx + dy * dy).sqrt();

    // Check zones
    if distance < INNER_RADIUS {
        // Center area - close button
    } else if distance > MENU_RADIUS + 20.0 {
        // Outside menu
    } else {
        // In ring area - find which segment
        let angle = dy.atan2(dx);
        let segment = self.slices.iter().find(|s| {
            angle >= s.start_angle && angle <= s.end_angle
        });
    }
}
```

**Key insight:** `atan2(dy, dx)` gives the angle from center to cursor, which can be compared against each segment's angular range.

---

## System Tray Integration

### The Challenge

Provide an optional system tray icon for quick access to the pie menu.

### Solution: ksni Crate (Same as Previous Projects)

```rust
impl Tray for PieMenuTray {
    fn activate(&mut self, x: i32, y: i32) {
        // Tray click - x,y is cursor position (not useful for us)
        let _ = self.tx.send(TrayMessage::ShowPieMenu { x, y });
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        create_pie_icon()  // Custom pie-chart icon
    }
}
```

**Key learning from previous projects:** The `activate` method receives cursor coordinates, but for a panel-positioned tray icon, these coordinates aren't useful for positioning a menu elsewhere on screen.

---

## Wayland Cursor Position Challenge

### The Challenge

Position the pie menu at the cursor location, like traditional pie menus.

### Discovery: Wayland Security Model

Unlike X11, Wayland doesn't expose global cursor position to applications for security reasons. Applications only know cursor position when the cursor is over their own surfaces.

### Research: How Others Solve This

[Kando](https://github.com/kando-menu/kando) pie menu uses shell-specific extensions:
- GNOME: A shell extension exposes cursor position via D-Bus
- Other DEs: Similar integration extensions

### Current Solution: Centered Menu

For now, the menu appears centered on screen:
- Works well for keyboard shortcuts
- Predictable behavior
- Future enhancement could involve COSMIC-specific integration

**Key insight:** Some features that were trivial in X11 require compositor-specific integration in Wayland. This is a security feature, not a bug.

---

## Running App Detection

### The Challenge

Detect which applications are currently running to show indicators (like the dock does) and include non-favorite running apps in the menu.

### Discovery: Wayland Protocols

COSMIC desktop supports the `ext_foreign_toplevel_list_v1` Wayland protocol, which provides information about open windows including their `app_id`.

### Solution: Subprocess-Based Detection

Direct Wayland connection from the pie menu process conflicts with libcosmic's connection, causing the menu to fail. Solution: query running apps via subprocess:

```rust
// Main process spawns subprocess to avoid Wayland connection conflict
fn query_running_via_subprocess() -> HashSet<String> {
    let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
    match Command::new(&exe).arg("--query-running").output() {
        Ok(output) => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        }
        Err(_) => HashSet::new(),
    }
}
```

The `--query-running` mode connects to Wayland separately and prints app IDs:

```rust
// Wayland protocol implementation
impl Dispatch<ExtForeignToplevelHandleV1, ()> for ToplevelState {
    fn event(&mut self, handle: &ExtForeignToplevelHandleV1, event: Event, ...) {
        match event {
            Event::AppId { app_id } => {
                self.pending_app_ids.insert(handle_id, app_id);
            }
            Event::Done => {
                // Add to running apps set
                if let Some(app_id) = self.pending_app_ids.get(&handle_id) {
                    self.running_apps.lock().unwrap().insert(app_id.clone());
                }
            }
            Event::Closed => {
                // Remove from running apps
            }
            _ => {}
        }
    }
}
```

**Key insight:** Multiple Wayland connections from the same process can conflict. Subprocess isolation provides clean separation.

---

## Full-Screen Layer Surface for Stability

### The Challenge

After suspend/resume, the pie menu window would exist (receiving events) but not render visibly.

### Discovery Process

Debug output showed the window was processing mouse hover events and keyboard input, but nothing was visible on screen:

```
DEBUG: update called with CanvasEvent(HoverSegment(Some(5)))
DEBUG: update called with CanvasEvent(HoverSegment(Some(6)))
DEBUG: update called with KeyPressed(Named(Escape))
```

### Solution: Full-Screen Anchored Surface

The original centered mode used a fixed-size floating window:

```rust
// BEFORE: Fixed-size, unanchored (unreliable after resume)
settings.size = Some((Some(window_size), Some(window_size)));
settings.anchor = Anchor::empty();
```

Changing to full-screen anchored mode fixed the issue:

```rust
// AFTER: Full-screen, anchored to all edges (reliable)
settings.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
settings.size = Some((None, None)); // Fill available space
settings.exclusive_zone = -1;
```

The menu is drawn centered on the full-screen transparent surface.

**Key insight:** Full-screen anchored layer surfaces are more reliable with Wayland compositors, especially after suspend/resume when GPU state may be stale.

---

## Desktop File Matching

### The Challenge

Running apps report app_ids like `Slack` but desktop files are named `com.slack.Slack.desktop`. Need fuzzy matching.

### Solution: Multi-Strategy Lookup

```rust
fn find_desktop_file(app_id: &str) -> Option<PathBuf> {
    // 1. Try exact match first
    let filename = format!("{}.desktop", app_id);
    for dir in desktop_file_dirs() {
        let path = dir.join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    // 2. Search for partial matches (last component)
    let app_id_lower = app_id.to_lowercase();
    for dir in desktop_file_dirs() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy();
                if name.ends_with(".desktop") {
                    let base = name.trim_end_matches(".desktop");
                    // Check if last component matches (com.slack.Slack -> Slack)
                    if let Some(last_part) = base.rsplit('.').next() {
                        if last_part.to_lowercase() == app_id_lower {
                            return Some(entry.path());
                        }
                    }
                }
            }
        }
    }
    None
}
```

**Key insight:** App IDs from Wayland don't always match desktop file names exactly. Fuzzy matching by the last component handles most cases.

---

## Dock Applet Integration

### The Challenge

Include COSMIC dock applets (App Library, Launcher, Workspaces) in the pie menu alongside regular app favorites.

### Solution: Read Dock Plugin Configuration

COSMIC stores dock applet configuration in RON format:

```rust
fn dock_plugins_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("cosmic/com.system76.CosmicPanel.Dock/v1/plugins_center"))
}

pub fn read_dock_applets() -> Vec<String> {
    let content = fs::read_to_string(&path)?;
    // Format: Some(["com.system76.CosmicPanelAppButton", ...])
    ron::from_str::<Option<Vec<String>>>(&content)?.unwrap_or_default()
}
```

### Mapping Applets to Actions

Each applet ID maps to a name, command, and icon:

```rust
const DOCK_APPLETS: &[DockApplet] = &[
    DockApplet {
        id: "com.system76.CosmicPanelAppButton",
        name: "App Library",
        exec: "cosmic-app-library",
        icon: "com.system76.CosmicPanelAppButton",
    },
    // ...
];
```

**Key insight:** COSMIC provides its own icons in `/usr/share/icons/hicolor/scalable/apps/` that match the dock styling exactly.

---

## Theme-Aware Tray Icon

### The Challenge

The tray icon should adapt to light/dark mode changes in real-time, not just at startup.

### Solution: Theme Detection with Polling

```rust
fn cosmic_theme_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("cosmic/com.system76.CosmicTheme.Mode/v1/is_dark"))
}

fn is_dark_mode() -> bool {
    if let Ok(content) = fs::read_to_string(&path) {
        return content.trim() == "true";
    }
    true // Default to dark
}
```

The tray event loop polls for theme changes and restarts when detected:

```rust
if loop_start.duration_since(last_theme_check) > Duration::from_secs(1) {
    let new_dark_mode = is_dark_mode();
    if new_dark_mode != tracked_dark_mode {
        handle.shutdown();
        return Ok(TrayExitReason::ThemeChanged);
    }
}
```

**Key insight:** Polling the theme config file is simpler than file watching and sufficient for a 1-second response time.

---

## Dynamic Icon Positioning

### The Challenge

Icons appeared too close to center on larger pie menus and too far out on smaller ones.

### Solution: Context-Aware Formula

Instead of a fixed ratio, use a formula that considers pie size:

```rust
fn calculate_icon_radius(menu_radius: f32, num_apps: usize) -> f32 {
    let segment_depth = menu_radius - INNER_RADIUS;
    let center = INNER_RADIUS + segment_depth / 2.0;

    // More apps = narrower segments = push icons outward
    let bias = if num_apps <= 6 {
        0.1  // Small pie: slight outward bias
    } else if num_apps <= 10 {
        0.15 // Medium pie
    } else {
        0.2  // Large pie: more outward bias
    };

    center + segment_depth * bias
}
```

**Key insight:** Fixed ratios produce inconsistent visual results across different scale factors. Dynamic formulas adapt to context.

---

## Automatic Autostart

### The Challenge

Users expect the tray icon to persist across logins without manual configuration.

### Solution: Create Autostart Entry on First Run

```rust
fn ensure_autostart() {
    let desktop_file = autostart_dir.join("cosmic-pie-menu.desktop");

    // Don't overwrite user modifications
    if desktop_file.exists() {
        return;
    }

    let content = r#"[Desktop Entry]
Type=Application
Name=COSMIC Pie Menu
Exec=cosmic-pie-menu
..."#;

    fs::write(&desktop_file, content)?;
}
```

**Key insight:** Check for existing files before creating to respect user customizations. This pattern is used consistently across COSMIC tray apps.

---

## Icon Theme Search Order

### The Challenge

Icons weren't found when they existed in the Pop or COSMIC-specific locations.

### Solution: Search Multiple Themes with Priority

```rust
let icon_themes = ["Pop", "Adwaita", "hicolor", "Papirus"];
let sizes = [&format!("{}x{}", size, size), "scalable", "symbolic"];

for theme in icon_themes {
    for sz in sizes {
        for category in ["apps", "actions", "places", "status"] {
            let path = format!("/usr/share/icons/{}/{}/{}/{}.svg", theme, sz, category, icon_name);
            if Path::new(&path).exists() {
                return Some(path.into());
            }
        }
    }
}
```

**Key insight:** Different desktop environments install icons in different locations. Search the target environment's theme first (Pop for COSMIC).

---

## Touchpad Gesture Detection

### The Challenge

Enable opening the pie menu via touchpad gesture (four-finger tap) with the menu appearing at the cursor position, working around Wayland's cursor position restrictions.

### Solution: evdev Direct Input Access

Rather than using libinput (which abstracts gestures), we use the `evdev` crate to read raw touchpad events directly from `/dev/input/`:

```rust
use evdev::{AbsoluteAxisType, Device, InputEventKind, Key};

fn is_touchpad_with_quadtap(device: &Device) -> bool {
    let keys = device.supported_keys()?;
    if !keys.contains(Key::BTN_TOOL_QUADTAP) {
        return false;
    }
    // Must also have absolute axes (touchpad characteristic)
    let abs = device.supported_absolute_axes()?;
    abs.contains(AbsoluteAxisType::ABS_MT_POSITION_X)
}
```

### Distinguishing Taps from Swipes

Four-finger swipes (for workspace switching) must not trigger the menu. We track both duration and finger movement:

```rust
enum GestureState {
    Idle,
    FingersDown {
        start: Instant,
        start_x: Option<i32>,
        start_y: Option<i32>,
        max_movement: i32,
    },
}

const TAP_MAX_DURATION: Duration = Duration::from_millis(250);
const TAP_MAX_MOVEMENT: i32 = 500;  // Touchpad units

fn process_event(event: &InputEvent, state: &mut GestureState) -> GestureEvent {
    match event.kind() {
        InputEventKind::Key(Key::BTN_TOOL_QUADTAP) => {
            if event.value() == 1 {
                // Fingers down - start tracking
                *state = GestureState::FingersDown { start: Instant::now(), ... };
            } else if event.value() == 0 {
                // Fingers up - check if it was a tap
                if let GestureState::FingersDown { start, max_movement, .. } = *state {
                    if start.elapsed() <= TAP_MAX_DURATION && max_movement <= TAP_MAX_MOVEMENT {
                        return GestureEvent::FingersUp;  // Trigger menu
                    }
                    // Otherwise it was a swipe - ignore
                }
            }
        }
        // Track absolute position for movement detection
        InputEventKind::AbsAxis(AbsoluteAxisType::ABS_MT_POSITION_X) => {
            // Update max_movement based on distance from start position
        }
        _ => {}
    }
}
```

### Gesture Workflow with Visual Feedback

The gesture integrates with the tray icon for visual feedback:

1. **Four fingers touch** → `BTN_TOOL_QUADTAP=1` → Tray icon turns cyan
2. **User moves cursor** (fingers can be lifted) → Transparent overlay captures position
3. **Menu triggered** → `BTN_TOOL_QUADTAP=0` within time/movement limits → Menu appears
4. **Menu closes** → Tray icon returns to normal

```rust
pub struct GestureFeedback {
    triggered: Arc<AtomicBool>,
    reset_requested: Arc<AtomicBool>,
}

impl GestureFeedback {
    pub fn trigger(&self) { /* Turn icon cyan */ }
    pub fn reset(&self) { /* Return icon to normal */ }
}
```

### Permission Requirements

evdev requires read access to `/dev/input/event*` devices:

```bash
sudo gpasswd -a $USER input
newgrp input  # Apply immediately without logout
```

### Key Insights

- **evdev vs libinput**: evdev provides raw events; libinput provides gesture abstractions. For detecting specific button events like `BTN_TOOL_QUADTAP`, evdev is simpler.
- **Tap vs swipe detection**: Time alone isn't sufficient - swipes can be quick. Tracking finger position movement provides reliable discrimination.
- **No libinput dependency**: The evdev crate is pure Rust, reading directly from kernel input devices with no system library dependencies.
- **Compositor-independent**: Unlike Wayland protocols, evdev access works regardless of compositor because it reads from the kernel input layer.

---

## Resources

### Crates Used

- [`libcosmic`](https://github.com/pop-os/libcosmic) - COSMIC UI toolkit with Wayland support
- [`ksni`](https://crates.io/crates/ksni) - StatusNotifierItem (system tray) implementation
- [`freedesktop-icons`](https://crates.io/crates/freedesktop-icons) - Icon theme lookup
- [`ron`](https://crates.io/crates/ron) - Rusty Object Notation for COSMIC configs
- [`evdev`](https://crates.io/crates/evdev) - Linux input device (evdev) access for gesture detection

### Documentation

- [iced Canvas](https://docs.rs/iced/latest/iced/widget/canvas/index.html) - Canvas widget documentation
- [Wayland Layer-Shell](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) - Layer shell protocol
- [freedesktop Icon Theme Spec](https://specifications.freedesktop.org/icon-theme-spec/latest/) - Icon discovery rules

### Related Projects

- [Kando](https://github.com/kando-menu/kando) - Cross-platform pie menu (Electron-based)
- [cosmic-comp](https://github.com/pop-os/cosmic-comp) - COSMIC compositor
