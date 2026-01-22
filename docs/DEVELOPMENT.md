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
12. [Resources](#resources)

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

## Resources

### Crates Used

- [`libcosmic`](https://github.com/pop-os/libcosmic) - COSMIC UI toolkit with Wayland support
- [`ksni`](https://crates.io/crates/ksni) - StatusNotifierItem (system tray) implementation
- [`freedesktop-icons`](https://crates.io/crates/freedesktop-icons) - Icon theme lookup
- [`ron`](https://crates.io/crates/ron) - Rusty Object Notation for COSMIC configs

### Documentation

- [iced Canvas](https://docs.rs/iced/latest/iced/widget/canvas/index.html) - Canvas widget documentation
- [Wayland Layer-Shell](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) - Layer shell protocol
- [freedesktop Icon Theme Spec](https://specifications.freedesktop.org/icon-theme-spec/latest/) - Icon discovery rules

### Related Projects

- [Kando](https://github.com/kando-menu/kando) - Cross-platform pie menu (Electron-based)
- [cosmic-comp](https://github.com/pop-os/cosmic-comp) - COSMIC compositor
