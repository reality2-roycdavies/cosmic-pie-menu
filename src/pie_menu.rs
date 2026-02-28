//! Pie Menu UI Module
//!
//! Renders a radial menu of applications using iced with layer-shell.
//! Uses canvas for true circular positioning with radial segment highlighting.
//!
//! Includes a cursor tracking phase to capture mouse position before showing
//! the menu, working around Wayland's cursor position restrictions.

use cosmic::iced::widget::canvas;
use cosmic::iced::widget::canvas::{Event, Geometry, Path, Program, Stroke, Text};
use cosmic::iced::{Color, Font, Point, Rectangle, Renderer, Theme, mouse};
use cosmic::iced_core::svg::{Handle as SvgHandle, Svg};
use cosmic::iced_core::image::{Handle as ImageHandle, Image};
use cosmic::iced::window::Id;
use cosmic::iced::{Element, Length, Task, Subscription};
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::keyboard::{self, Key};
use cosmic::iced::time;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use std::f32::consts::PI;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::sync::{Arc, Mutex};

use crate::apps::{AppInfo, find_icon_path};
use crate::config::PieMenuConfig;
use crate::windows;

/// Minimum radius of the pie menu circle (for small number of apps)
const MIN_MENU_RADIUS: f32 = 80.0;

/// Minimum inner radius (for the center area with few apps)
const MIN_INNER_RADIUS: f32 = 40.0;

/// Ratio of inner radius to menu radius (for proportional scaling)
const INNER_RADIUS_RATIO: f32 = 0.4;

/// Calculate menu radius based on number of apps
/// Ensures icons have enough space around the circle
fn calculate_menu_radius(num_apps: usize, icon_spacing: f32) -> f32 {
    if num_apps == 0 {
        return MIN_MENU_RADIUS;
    }
    // Circumference needed = num_apps * spacing
    // Circumference = 2 * PI * radius
    // So radius = (num_apps * spacing) / (2 * PI)
    let calculated = (num_apps as f32 * icon_spacing) / (2.0 * PI);
    calculated.max(MIN_MENU_RADIUS)
}

/// Calculate the inner radius based on menu radius
/// Scales proportionally to maintain visual balance as menu grows
fn calculate_inner_radius(menu_radius: f32) -> f32 {
    let proportional = menu_radius * INNER_RADIUS_RATIO;
    proportional.max(MIN_INNER_RADIUS)
}

/// Calculate the radius at which icons should be placed
/// Places icons towards the outer edge of the segment area
fn calculate_icon_radius(menu_radius: f32, inner_radius: f32, _num_apps: usize) -> f32 {
    // Segments are drawn from (inner_radius + 2) to (menu_radius + 2)
    let segment_inner = inner_radius + 2.0;
    let segment_outer = menu_radius + 2.0;

    // Place icons at 65% from inner to outer (biased toward outer edge)
    segment_inner + (segment_outer - segment_inner) * 0.65
}

/// Rubber band effect: radial offset targets for neighbors by distance from hovered icon
/// Index 0 = hovered icon itself, 1 = immediate neighbor, 2 = next neighbor
const RUBBER_BAND_RADIAL: &[f32] = &[1.0, 0.4, 0.12];

/// Rubber band effect: angular pull (radians) for immediate neighbor at full animation
/// Neighbors get pulled toward the hovered icon as if connected by a rubber band
const RUBBER_BAND_ANGULAR_PULL: f32 = 0.06;

/// Rubber band effect: angular pull falloff per distance step
const RUBBER_BAND_ANGULAR_FALLOFF: &[f32] = &[0.0, 1.0, 0.35];

/// Calculate shortest circular distance between two indices in a ring of n elements
fn circular_distance(a: usize, b: usize, n: usize) -> usize {
    if n == 0 { return 0; }
    let forward = (b + n - a) % n;
    let backward = (a + n - b) % n;
    forward.min(backward)
}

/// Calculate direction from index `from` toward index `toward` in a ring of n elements
/// Returns -1.0 if `toward` is counter-clockwise, +1.0 if clockwise, 0.0 if same
fn circular_direction(from: usize, toward: usize, n: usize) -> f32 {
    if n == 0 || from == toward { return 0.0; }
    let forward = (toward + n - from) % n;
    let backward = (from + n - toward) % n;
    if forward <= backward { 1.0 } else { -1.0 }
}

/// Theme colors for the pie menu
/// Integrates with COSMIC theme system for consistent colors
struct PieTheme {
    /// Background color of the pie
    bg_color: Color,
    /// Color of a segment when not hovered
    segment_color: Color,
    /// Color of a segment when hovered (subtle shift)
    segment_hover_color: Color,
    /// Border/divider color
    border_color: Color,
    /// Text color
    text_color: Color,
    /// Running indicator color
    running_indicator_color: Color,
    /// Outer indicator ring background color
    indicator_ring_color: Color,
}

/// Convert a COSMIC Srgba color to iced Color with custom alpha
fn srgba_to_color(srgba: cosmic::theme::CosmicColor, alpha: f32) -> Color {
    Color::from_rgba(srgba.red, srgba.green, srgba.blue, alpha)
}

/// Convert a COSMIC Srgba color to iced Color preserving alpha
fn srgba_to_color_full(srgba: cosmic::theme::CosmicColor) -> Color {
    Color::from_rgba(srgba.red, srgba.green, srgba.blue, srgba.alpha)
}

impl PieTheme {
    /// Get theme from COSMIC's system preference
    fn current() -> Self {
        let theme = cosmic::theme::system_preference();
        let cosmic = theme.cosmic();

        // Use background container for the pie menu (matches dock/panel colors)
        let bg = &cosmic.background;
        let accent = &cosmic.accent;

        // Base background with high opacity for the pie
        let bg_color = srgba_to_color(bg.base, 0.95);

        // Segments use background component colors (same as dock)
        let segment_color = srgba_to_color(bg.component.base, 0.95);
        // Hover uses accent color from theme
        let segment_hover_color = srgba_to_color(accent.base, 0.85);

        // Divider color from theme
        let border_color = srgba_to_color(bg.divider, 0.6);

        // Text color from theme
        let text_color = srgba_to_color_full(bg.on);

        // Running indicator - use accent color for visibility
        let running_indicator_color = srgba_to_color(accent.base, 0.9);

        // Outer indicator ring - use a darker shade of the background
        let indicator_ring_color = srgba_to_color(bg.base, 1.0);

        Self {
            bg_color,
            segment_color,
            segment_hover_color,
            border_color,
            text_color,
            running_indicator_color,
            indicator_ring_color,
        }
    }
}

/// Get the path to COSMIC's config directory
fn cosmic_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/cosmic"))
}

/// Get the path to COSMIC's theme config file
fn cosmic_theme_path() -> Option<PathBuf> {
    cosmic_config_dir().map(|d| d.join("com.system76.CosmicTheme.Mode/v1/is_dark"))
}

/// Detect if the system is in dark mode
fn is_dark_mode() -> bool {
    // Try COSMIC's config file first
    if let Some(path) = cosmic_theme_path() {
        if let Ok(content) = fs::read_to_string(&path) {
            return content.trim() == "true";
        }
    }

    // Fall back to freedesktop portal
    if let Ok(output) = Command::new("gdbus")
        .args([
            "call", "--session",
            "--dest", "org.freedesktop.portal.Desktop",
            "--object-path", "/org/freedesktop/portal/desktop",
            "--method", "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance", "color-scheme"
        ])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Returns "(<uint32 1>,)" for dark, "(<uint32 0>,)" for light
        if stdout.contains("1") {
            return true;
        } else if stdout.contains("0") {
            return false;
        }
    }

    // Default to dark mode if we can't detect
    true
}

/// Messages for the pie menu
#[derive(Debug, Clone)]
pub enum Message {
    /// An app was clicked
    LaunchApp(usize),
    /// Close the menu
    Close,
    /// Key pressed
    KeyPressed(Key),
    /// Canvas event
    CanvasEvent(PieCanvasMessage),
    /// Initial tick to force layout
    Tick,
}

#[derive(Debug, Clone)]
pub enum PieCanvasMessage {
    HoverSegment(Option<usize>),
    ClickSegment(usize),
    RightClickSegment(usize),
    ClickCenter,
}

/// Create a tinted glow SVG handle for an icon
/// For SVGs: injects a color filter into the SVG content
/// For raster images: wraps in an SVG with the image base64-encoded and filtered
fn create_glow_handle(icon_path: &PathBuf, glow_color: &Color, icon_size: u16) -> Option<SvgHandle> {
    use base64::Engine;

    let r = (glow_color.r * 255.0) as u8;
    let g = (glow_color.g * 255.0) as u8;
    let b = (glow_color.b * 255.0) as u8;
    let filter_defs = format!(
        r#"<defs><filter id="gc"><feFlood flood-color="rgb({},{},{})" flood-opacity="0.18"/><feComposite in2="SourceAlpha" operator="in"/></filter></defs>"#,
        r, g, b
    );

    let ext = icon_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext.eq_ignore_ascii_case("svg") {
        // Inject filter into existing SVG
        let svg_content = fs::read_to_string(icon_path).ok()?;
        if let Some(svg_start) = svg_content.find("<svg") {
            if let Some(rel_end) = svg_content[svg_start..].find('>') {
                let insert_pos = svg_start + rel_end + 1;
                let tinted = format!(
                    "{}{}<g filter=\"url(#gc)\">{}",
                    &svg_content[..insert_pos],
                    filter_defs,
                    svg_content[insert_pos..].replace("</svg>", "</g></svg>")
                );
                return Some(SvgHandle::from_memory(tinted.into_bytes()));
            }
        }
        None
    } else {
        // Wrap raster image in SVG with base64 data URI and filter
        let bytes = fs::read(icon_path).ok()?;
        let mime = match ext.to_ascii_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/png",
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let size = icon_size;
        let wrapper = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{size}" height="{size}">{filter}<image href="data:{mime};base64,{b64}" width="{size}" height="{size}" filter="url(#gc)"/></svg>"#,
            size = size,
            filter = filter_defs,
            mime = mime,
            b64 = b64,
        );
        Some(SvgHandle::from_memory(wrapper.into_bytes()))
    }
}

/// App data with pre-calculated position
struct AppSlice {
    index: usize,
    name: String,
    icon_path: Option<PathBuf>,
    angle: f32,           // Center angle of this slice
    start_angle: f32,     // Start of slice
    end_angle: f32,       // End of slice
    running_count: u32,   // Number of running windows (0 = not running)
    glow_handle: Option<SvgHandle>, // Pre-created tinted glow handle
}

/// State for the pie menu application
struct PieMenuApp {
    apps: Vec<AppInfo>,
    slices: Vec<AppSlice>,
    hovered_slice: Option<usize>,
    tick_count: u32,  // Count ticks to trigger redraws on scaled displays
    /// Position mode: None = centered window, Some = full-screen with menu at position
    cursor_position: Option<(f32, f32)>,
    /// Dynamic menu radius based on number of apps
    menu_radius: f32,
    /// Dynamic inner radius (scales with menu size)
    inner_radius: f32,
    /// Theme: show background behind pie slices (also controls indicator ring background)
    show_background: bool,
    /// Theme: highlight only icon on hover (vs whole segment)
    icon_only_highlight: bool,
    /// Animation state: current hover offset for each slice (0.0 to 1.0)
    hover_offsets: Vec<f32>,
    /// Configurable icon size
    icon_size: u16,
    /// Configurable hover offset distance
    hover_offset: f32,
    /// Configurable animation speed
    animation_speed: f32,
}

impl PieMenuApp {
    fn new_at(apps: Vec<AppInfo>, position: Option<(f32, f32)>) -> (Self, Task<Message>) {
        // Load config for all settings
        let config = PieMenuConfig::load();
        let icon_size = config.icon_size;
        let menu_radius = calculate_menu_radius(apps.len(), config.icon_spacing);
        let inner_radius = calculate_inner_radius(menu_radius);

        let mut settings = SctkLayerSurfaceSettings::default();
        settings.keyboard_interactivity = KeyboardInteractivity::OnDemand;
        settings.layer = Layer::Top;

        // Full-screen anchored surface - draw menu at center or cursor position
        settings.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
        settings.size = Some((None, None)); // Fill available space
        settings.exclusive_zone = -1;

        // Pre-calculate slice data (positions calculated during draw)
        let num_apps = apps.len();

        // Get glow color from theme for pre-creating tinted icon handles
        let pie_theme = PieTheme::current();
        let glow_color = pie_theme.segment_hover_color;

        let slices: Vec<AppSlice> = apps
            .iter()
            .enumerate()
            .map(|(i, app)| {
                let slice_angle = 2.0 * PI / num_apps as f32;
                // Start from top (-PI/2), go clockwise
                let angle = -PI / 2.0 + (i as f32 * slice_angle);
                let start_angle = angle - slice_angle / 2.0;
                let end_angle = angle + slice_angle / 2.0;

                let icon_path = app.icon.as_ref()
                    .and_then(|name| find_icon_path(name, icon_size));

                // Pre-create tinted glow handle if icon_only_highlight is enabled
                let glow_handle = if config.icon_only_highlight {
                    icon_path.as_ref().and_then(|p| create_glow_handle(p, &glow_color, icon_size))
                } else {
                    None
                };

                AppSlice {
                    index: i,
                    name: app.name.clone(),
                    icon_path,
                    angle,
                    start_angle,
                    end_angle,
                    running_count: app.running_count,
                    glow_handle,
                }
            })
            .collect();

        let num_slices = slices.len();
        let app = Self {
            apps,
            slices,
            hovered_slice: None,
            tick_count: 0,
            cursor_position: position,
            menu_radius,
            inner_radius,
            show_background: config.show_background,
            icon_only_highlight: config.icon_only_highlight,
            hover_offsets: vec![0.0; num_slices],
            icon_size,
            hover_offset: config.hover_offset,
            animation_speed: config.animation_speed,
        };

        (app, get_layer_surface(settings))
    }

    fn title(&self, _id: Id) -> String {
        String::from("Pie Menu")
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LaunchApp(index) => {
                if let Some(app) = self.apps.get(index) {
                    if let Some(ref exec) = app.exec {
                        println!("Launching: {} ({})", app.name, exec);
                        // Parse exec command, removing field codes like %u, %F, etc.
                        let exec_clean: String = exec
                            .split_whitespace()
                            .filter(|s| !s.starts_with('%'))
                            .collect::<Vec<_>>()
                            .join(" ");

                        // Launch via shell with a small delay so the pie menu window
                        // closes before the app starts. This prevents apps like
                        // cosmic-screenshot from capturing the menu in their screenshot.
                        let _ = Command::new("sh")
                            .arg("-c")
                            .arg(format!("sleep 0.1 && {}", exec_clean))
                            .spawn();
                    }
                }
                std::process::exit(0);
            }
            Message::Close => {
                std::process::exit(0);
            }
            Message::CanvasEvent(PieCanvasMessage::HoverSegment(segment)) => {
                if self.hovered_slice != segment {
                    self.hovered_slice = segment;
                }
                Task::none()
            }
            Message::CanvasEvent(PieCanvasMessage::ClickSegment(index)) => {
                self.update(Message::LaunchApp(index))
            }
            Message::CanvasEvent(PieCanvasMessage::RightClickSegment(index)) => {
                if let Some(app) = self.apps.get(index) {
                    if app.running_count > 0 {
                        // Switch to existing window
                        println!("Switching to: {}", app.name);
                        match windows::activate_window_by_app_id(&app.id) {
                            Ok(true) => {
                                std::process::exit(0);
                            }
                            Ok(false) => {
                                eprintln!("No window found for {}, launching new instance", app.id);
                                // Fall through to launch new instance
                                return self.update(Message::LaunchApp(index));
                            }
                            Err(e) => {
                                eprintln!("Failed to activate: {}", e);
                            }
                        }
                    } else {
                        // Non-running app: launch it
                        return self.update(Message::LaunchApp(index));
                    }
                }
                Task::none()
            }
            Message::CanvasEvent(PieCanvasMessage::ClickCenter) => {
                self.update(Message::Close)
            }
            Message::KeyPressed(key) => {
                if matches!(key, Key::Named(keyboard::key::Named::Escape)) {
                    std::process::exit(0);
                }
                Task::none()
            }
            Message::Tick => {
                // Keep ticking for a bit to trigger layout recalculation on scaled displays
                self.tick_count += 1;

                // Animate hover offsets for smooth icon movement (rubber band effect)
                if self.icon_only_highlight {
                    let n = self.hover_offsets.len();
                    for (i, offset) in self.hover_offsets.iter_mut().enumerate() {
                        let target = if let Some(hovered) = self.hovered_slice {
                            let dist = circular_distance(i, hovered, n);
                            if dist < RUBBER_BAND_RADIAL.len() {
                                RUBBER_BAND_RADIAL[dist]
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };
                        if (*offset - target).abs() > 0.01 {
                            *offset += (target - *offset) * self.animation_speed;
                        } else {
                            *offset = target;
                        }
                    }
                }

                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let keyboard_sub = keyboard::on_key_press(|key, _modifiers| Some(Message::KeyPressed(key)));

        // Keep ticking for animations and initial layout
        // - First 500ms for scaled display layout
        // - Continuously when icon_only_highlight for smooth hover animations
        let needs_ticks = self.tick_count < 10 || self.icon_only_highlight;
        if needs_ticks {
            let tick_sub = time::every(Duration::from_millis(16)).map(|_| Message::Tick); // ~60fps
            Subscription::batch([keyboard_sub, tick_sub])
        } else {
            keyboard_sub
        }
    }

    fn view(&self, _id: Id) -> Element<'_, Message> {
        // Get hovered app name for center display
        let hovered_name = self.hovered_slice
            .and_then(|i| self.slices.get(i))
            .map(|s| s.name.clone())
            .unwrap_or_default();

        let pie_canvas = canvas(PieCanvas {
            slices: &self.slices,
            hovered: self.hovered_slice,
            cursor_position: self.cursor_position,
            menu_radius: self.menu_radius,
            inner_radius: self.inner_radius,
            hovered_name,
            show_background: self.show_background,
            icon_only_highlight: self.icon_only_highlight,
            hover_offsets: &self.hover_offsets,
            icon_size: self.icon_size,
            hover_offset: self.hover_offset,
        });

        // Always full-screen mode for reliable layer surface behavior
        pie_canvas.width(Length::Fill).height(Length::Fill).into()
    }

    fn theme(&self, _id: Id) -> Theme {
        if is_dark_mode() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

/// Canvas widget for rendering the pie menu
struct PieCanvas<'a> {
    slices: &'a [AppSlice],
    hovered: Option<usize>,
    /// If Some, draw the menu centered at this position; if None, center in bounds
    cursor_position: Option<(f32, f32)>,
    /// Dynamic menu radius
    menu_radius: f32,
    /// Dynamic inner radius (scales with menu size)
    inner_radius: f32,
    /// Name of hovered app (to display in center)
    hovered_name: String,
    /// Theme: show background behind pie slices (also controls indicator ring background)
    show_background: bool,
    /// Theme: highlight only icon on hover (vs whole segment)
    icon_only_highlight: bool,
    /// Animated hover offsets for each slice (0.0 = not hovered, 1.0 = fully hovered)
    hover_offsets: &'a [f32],
    /// Configurable icon size
    icon_size: u16,
    /// Configurable hover offset distance
    hover_offset: f32,
}

impl<'a> Program<Message> for PieCanvas<'a> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        let Some(cursor_pos) = cursor.position_in(bounds) else {
            return (canvas::event::Status::Ignored, None);
        };

        let menu_size = self.menu_radius * 2.0 + self.icon_size as f32 + 80.0;

        // Determine center point: cursor position or center of bounds
        let center = if let Some((cx, cy)) = self.cursor_position {
            // Clamp to keep menu fully visible (same logic as draw)
            let half_menu = menu_size / 2.0;
            // Handle case where screen is smaller than menu
            let min_x = half_menu.min(bounds.width - half_menu);
            let max_x = half_menu.max(bounds.width - half_menu);
            let min_y = half_menu.min(bounds.height - half_menu);
            let max_y = half_menu.max(bounds.height - half_menu);
            let x = cx.clamp(min_x, max_x);
            let y = cy.clamp(min_y, max_y);
            Point::new(x, y)
        } else {
            Point::new(bounds.width / 2.0, bounds.height / 2.0)
        };
        let dx = cursor_pos.x - center.x;
        let dy = cursor_pos.y - center.y;
        let distance = (dx * dx + dy * dy).sqrt();

        // Check if in center (close button area)
        if distance < self.inner_radius {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CanvasEvent(PieCanvasMessage::ClickCenter)),
                    );
                }
                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CanvasEvent(PieCanvasMessage::HoverSegment(None))),
                    );
                }
                _ => {}
            }
            return (canvas::event::Status::Ignored, None);
        }

        // Check if outside the menu
        if distance > self.menu_radius + 10.0 {
            match event {
                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CanvasEvent(PieCanvasMessage::HoverSegment(None))),
                    );
                }
                _ => {}
            }
            return (canvas::event::Status::Ignored, None);
        }

        // Calculate angle from center
        let mut angle = dy.atan2(dx);

        // Find which slice this angle falls into
        let hovered_slice = self.slices.iter().find(|slice| {
            let mut start = slice.start_angle;
            let mut end = slice.end_angle;

            // Normalize angles for comparison
            while start > PI { start -= 2.0 * PI; }
            while start < -PI { start += 2.0 * PI; }
            while end > PI { end -= 2.0 * PI; }
            while end < -PI { end += 2.0 * PI; }
            while angle > PI { angle -= 2.0 * PI; }
            while angle < -PI { angle += 2.0 * PI; }

            // Handle wrap-around
            if start > end {
                angle >= start || angle <= end
            } else {
                angle >= start && angle <= end
            }
        });

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(slice) = hovered_slice {
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CanvasEvent(PieCanvasMessage::ClickSegment(slice.index))),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(slice) = hovered_slice {
                    return (
                        canvas::event::Status::Captured,
                        Some(Message::CanvasEvent(PieCanvasMessage::RightClickSegment(slice.index))),
                    );
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let segment = hovered_slice.map(|s| s.index);
                return (
                    canvas::event::Status::Captured,
                    Some(Message::CanvasEvent(PieCanvasMessage::HoverSegment(segment))),
                );
            }
            _ => {}
        }

        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let menu_size = self.menu_radius * 2.0 + self.icon_size as f32 + 80.0;

        use cosmic::iced::widget::canvas::Frame;
        let mut frame = Frame::new(renderer, bounds.size());

        {
            // Determine center point: cursor position or center of bounds
            let center = if let Some((cx, cy)) = self.cursor_position {
                // Clamp to keep menu fully visible
                let half_menu = menu_size / 2.0;
                // Handle case where screen is smaller than menu
                let min_x = half_menu.min(bounds.width - half_menu);
                let max_x = half_menu.max(bounds.width - half_menu);
                let min_y = half_menu.min(bounds.height - half_menu);
                let max_y = half_menu.max(bounds.height - half_menu);
                let x = cx.clamp(min_x, max_x);
                let y = cy.clamp(min_y, max_y);
                Point::new(x, y)
            } else {
                Point::new(bounds.width / 2.0, bounds.height / 2.0)
            };
            let theme = PieTheme::current();

            // Clear with transparent background
            frame.fill_rectangle(
                Point::new(0.0, 0.0),
                bounds.size(),
                Color::TRANSPARENT,
            );

            // Draw background: transparent at inner edge, fading to solid, then fading to transparent at outer edge
            if self.show_background {
                let bg_color = theme.bg_color;
                let bg_outer = self.menu_radius + 2.0;
                let bg_inner = self.inner_radius;
                let bg_num_rings: usize = 60;
                let bg_ring_width = (bg_outer - bg_inner) / bg_num_rings as f32;

                for i in 0..bg_num_rings {
                    let stroke_radius = bg_inner + (i as f32 + 0.5) * bg_ring_width;
                    let progress = i as f32 / (bg_num_rings - 1) as f32; // 0 = inner, 1 = outer

                    // Fade in from transparent (0-30%), solid (30-85%), fade out (85-100%)
                    let alpha = if progress < 0.3 {
                        // Fade in from transparent at inner edge
                        let fade_progress = progress / 0.3;
                        bg_color.a * fade_progress
                    } else if progress > 0.85 {
                        // Fade out to transparent at outer edge
                        let fade_progress = (progress - 0.85) / 0.15;
                        bg_color.a * (1.0 - fade_progress)
                    } else {
                        // Solid middle
                        bg_color.a
                    };

                    let ring_color = Color::from_rgba(bg_color.r, bg_color.g, bg_color.b, alpha);
                    let ring_path = Path::circle(center, stroke_radius);
                    frame.stroke(
                        &ring_path,
                        Stroke::default()
                            .with_color(ring_color)
                            .with_width(bg_ring_width),
                    );
                }
            }

            // Draw ring for outer indicator area using theme color (only with background)
            if self.show_background {
                let indicator_ring_inner = self.menu_radius + 1.0;
                let indicator_ring_outer = self.menu_radius + 4.0;
                let indicator_ring_width = indicator_ring_outer - indicator_ring_inner;
                let indicator_ring_radius = (indicator_ring_inner + indicator_ring_outer) / 2.0;
                let indicator_bg = Path::circle(center, indicator_ring_radius);
                frame.stroke(
                    &indicator_bg,
                    Stroke::default()
                        .with_color(theme.indicator_ring_color)
                        .with_width(indicator_ring_width),
                );
            }

            // Draw each slice segment with fade at inner edge
            for slice in self.slices {
                let is_hovered = self.hovered == Some(slice.index);

                // Only draw segments if show_background is enabled
                if self.show_background {
                    let outer_radius = self.menu_radius + 2.0;
                    let inner_radius = self.inner_radius + 2.0;
                    let segment_depth = outer_radius - inner_radius;

                    // Base color for this segment
                    // In icon_only_highlight mode, don't highlight the whole segment
                    let base_color = if is_hovered && !self.icon_only_highlight {
                        theme.segment_hover_color
                    } else {
                        theme.segment_color
                    };

                    // Draw segment as concentric arc-strokes with fading alpha at inner edge
                    let num_rings = 60;
                    let ring_width = segment_depth / num_rings as f32;
                    let fade_rings = 24; // Number of rings that fade at inner edge

                    for r in 0..num_rings {
                        let ring_radius = inner_radius + (r as f32 + 0.5) * ring_width;

                        // Fade alpha for inner rings
                        let alpha = if r < fade_rings {
                            let fade_progress = r as f32 / fade_rings as f32;
                            base_color.a * fade_progress
                        } else {
                            base_color.a
                        };

                        let ring_color = Color::from_rgba(base_color.r, base_color.g, base_color.b, alpha);

                        // Draw arc for this ring
                        let arc = Path::new(|builder| {
                            let steps = 16;
                            let angle_step = (slice.end_angle - slice.start_angle) / steps as f32;
                            builder.move_to(Point::new(
                                center.x + ring_radius * slice.start_angle.cos(),
                                center.y + ring_radius * slice.start_angle.sin(),
                            ));
                            for i in 1..=steps {
                                let angle = slice.start_angle + angle_step * i as f32;
                                builder.line_to(Point::new(
                                    center.x + ring_radius * angle.cos(),
                                    center.y + ring_radius * angle.sin(),
                                ));
                            }
                        });

                        frame.stroke(
                            &arc,
                            Stroke::default()
                                .with_color(ring_color)
                                .with_width(ring_width),
                        );
                    }
                }

                // Calculate icon position using dynamic formula
                let base_icon_radius = calculate_icon_radius(self.menu_radius, self.inner_radius, self.slices.len());

                // In icon_only_highlight mode, smoothly animate icon outward when hovered
                // Rubber band effect: hovered icon moves out, neighbors get pulled out and toward it
                let hover_offset = self.hover_offsets.get(slice.index).copied().unwrap_or(0.0);
                let (icon_radius, draw_angle) = if self.icon_only_highlight {
                    let radial = base_icon_radius + self.hover_offset * hover_offset;

                    // Angular pull: neighbors get pulled toward the hovered icon
                    let angular = if let Some(hovered_idx) = self.hovered {
                        let n = self.slices.len();
                        let dist = circular_distance(slice.index, hovered_idx, n);
                        if dist > 0 && dist < RUBBER_BAND_ANGULAR_FALLOFF.len() {
                            let direction = circular_direction(slice.index, hovered_idx, n);
                            let falloff = RUBBER_BAND_ANGULAR_FALLOFF[dist];
                            direction * RUBBER_BAND_ANGULAR_PULL * falloff * hover_offset
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    (radial, slice.angle + angular)
                } else {
                    (base_icon_radius, slice.angle)
                };

                let icon_center = Point::new(
                    center.x + icon_radius * draw_angle.cos(),
                    center.y + icon_radius * draw_angle.sin(),
                );

                let icon_size = self.icon_size as f32;

                // Draw icon-shaped glow effect when in icon_only_highlight mode
                // Uses pre-created tinted SVG handles drawn at progressively larger sizes
                if self.icon_only_highlight && hover_offset > 0.01 {
                    if let Some(ref glow_handle) = slice.glow_handle {
                        let glow_svg = Svg::new(glow_handle.clone());
                        // Draw 4 layers at increasing sizes - overlap creates natural falloff
                        let glow_scales: &[f32] = &[1.6, 1.45, 1.3, 1.15];
                        for &scale in glow_scales {
                            let anim_scale = 1.0 + (scale - 1.0) * hover_offset;
                            let glow_size = icon_size * anim_scale;
                            let glow_bounds = Rectangle {
                                x: icon_center.x - glow_size / 2.0,
                                y: icon_center.y - glow_size / 2.0,
                                width: glow_size,
                                height: glow_size,
                            };
                            frame.draw_svg(glow_bounds, glow_svg.clone());
                        }
                    }
                }

                // Draw the icon or fallback to letter
                let icon_bounds = Rectangle {
                    x: icon_center.x - icon_size / 2.0,
                    y: icon_center.y - icon_size / 2.0,
                    width: icon_size,
                    height: icon_size,
                };

                if let Some(ref icon_path) = slice.icon_path {
                    let ext = icon_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("svg") {
                        // Draw SVG icon
                        let handle = SvgHandle::from_path(icon_path);
                        let svg = Svg::new(handle);
                        frame.draw_svg(icon_bounds, svg);
                    } else {
                        // Draw raster image (PNG, etc.)
                        let handle = ImageHandle::from_path(icon_path);
                        let img = Image::new(handle);
                        frame.draw_image(icon_bounds, img);
                    }
                } else {
                    // Fallback: draw first letter
                    let initial = slice.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                    frame.fill_text(Text {
                        content: initial,
                        position: icon_center,
                        color: theme.text_color,
                        size: 22.0.into(),
                        font: Font::DEFAULT,
                        horizontal_alignment: Horizontal::Center,
                        vertical_alignment: Vertical::Center,
                        ..Text::default()
                    });
                }

                // Draw running indicator (arc at outer edge)
                // Arc length varies based on number of running instances (like COSMIC dock)
                if slice.running_count > 0 {
                    let arc_radius = self.menu_radius + 4.0;
                    let slice_span = slice.end_angle - slice.start_angle;
                    let slice_center = (slice.start_angle + slice.end_angle) / 2.0;

                    // Calculate arc length based on running count:
                    // 1 window = small dot (12% of slice)
                    // 2 windows = medium indicator (35% of slice)
                    // 3+ windows = longer indicator (60% of slice)
                    let arc_fraction = match slice.running_count {
                        1 => 0.12,
                        2 => 0.35,
                        _ => 0.60,
                    };

                    let arc_half_span = (slice_span * arc_fraction) / 2.0;
                    let arc_start = slice_center - arc_half_span;
                    let arc_end = slice_center + arc_half_span;

                    if arc_end > arc_start {
                        let arc = Path::new(|builder| {
                            // Draw arc using line segments
                            let steps = 16;
                            let angle_step = (arc_end - arc_start) / steps as f32;
                            builder.move_to(Point::new(
                                center.x + arc_radius * arc_start.cos(),
                                center.y + arc_radius * arc_start.sin(),
                            ));
                            for i in 1..=steps {
                                let angle = arc_start + angle_step * i as f32;
                                builder.line_to(Point::new(
                                    center.x + arc_radius * angle.cos(),
                                    center.y + arc_radius * angle.sin(),
                                ));
                            }
                        });
                        frame.stroke(
                            &arc,
                            Stroke::default()
                                .with_color(theme.running_indicator_color)
                                .with_width(5.0)  // Thicker for better visibility
                                .with_line_cap(cosmic::iced::widget::canvas::LineCap::Round),
                        );
                    }
                }
            }

            // Inner circle is completely transparent - nothing drawn here
            // The fade happens in the background/segments from inner edge outward

            // Draw hovered app name in center with background pill for readability
            if !self.hovered_name.is_empty() {
                let words: Vec<&str> = self.hovered_name.split_whitespace().collect();
                let font_size = 16.0;
                let line_height = 20.0;
                let total_height = words.len() as f32 * line_height;
                let start_y = center.y - total_height / 2.0 + line_height / 2.0;

                // Estimate text width (rough approximation)
                let max_word_len = words.iter().map(|w| w.len()).max().unwrap_or(0);
                let text_width = (max_word_len as f32 * font_size * 0.6).max(60.0);

                // Draw semi-transparent background pill
                let padding_x = 16.0;
                let padding_y = 10.0;
                let pill_width = text_width + padding_x * 2.0;
                let pill_height = total_height + padding_y * 2.0;
                let pill_radius = pill_height / 2.0; // Fully rounded ends

                let pill = Path::new(|builder| {
                    // Draw rounded rectangle (pill shape)
                    let left = center.x - pill_width / 2.0;
                    let right = center.x + pill_width / 2.0;
                    let top = center.y - pill_height / 2.0;
                    let bottom = center.y + pill_height / 2.0;
                    let r = pill_radius.min(pill_width / 2.0);

                    // Start at top-left after the curve
                    builder.move_to(Point::new(left + r, top));
                    // Top edge
                    builder.line_to(Point::new(right - r, top));
                    // Top-right curve (approximate with lines)
                    for i in 0..=8 {
                        let angle = -PI / 2.0 + (i as f32 / 8.0) * (PI / 2.0);
                        builder.line_to(Point::new(
                            right - r + r * angle.cos(),
                            top + r + r * angle.sin(),
                        ));
                    }
                    // Right edge
                    builder.line_to(Point::new(right, bottom - r));
                    // Bottom-right curve
                    for i in 0..=8 {
                        let angle = 0.0 + (i as f32 / 8.0) * (PI / 2.0);
                        builder.line_to(Point::new(
                            right - r + r * angle.cos(),
                            bottom - r + r * angle.sin(),
                        ));
                    }
                    // Bottom edge
                    builder.line_to(Point::new(left + r, bottom));
                    // Bottom-left curve
                    for i in 0..=8 {
                        let angle = PI / 2.0 + (i as f32 / 8.0) * (PI / 2.0);
                        builder.line_to(Point::new(
                            left + r + r * angle.cos(),
                            bottom - r + r * angle.sin(),
                        ));
                    }
                    // Left edge
                    builder.line_to(Point::new(left, top + r));
                    // Top-left curve
                    for i in 0..=8 {
                        let angle = PI + (i as f32 / 8.0) * (PI / 2.0);
                        builder.line_to(Point::new(
                            left + r + r * angle.cos(),
                            top + r + r * angle.sin(),
                        ));
                    }
                    builder.close();
                });

                // Semi-transparent dark background
                let pill_color = Color::from_rgba(0.0, 0.0, 0.0, 0.7);
                frame.fill(&pill, pill_color);

                // Draw text
                for (i, word) in words.iter().enumerate() {
                    frame.fill_text(Text {
                        content: word.to_string(),
                        position: Point::new(center.x, start_y + i as f32 * line_height),
                        color: Color::WHITE,
                        size: font_size.into(),
                        font: Font::DEFAULT,
                        horizontal_alignment: Horizontal::Center,
                        vertical_alignment: Vertical::Center,
                        ..Text::default()
                    });
                }
            }

            // Only draw outer border if background is shown
            if self.show_background {
                let outer_border = Path::circle(center, self.menu_radius + 5.0);
                frame.stroke(
                    &outer_border,
                    Stroke::default()
                        .with_color(theme.border_color)
                        .with_width(2.0),
                );
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Style function for transparent background
fn app_style(_state: &PieMenuApp, _theme: &Theme) -> cosmic::iced_runtime::Appearance {
    cosmic::iced_runtime::Appearance {
        background_color: Color::TRANSPARENT,
        text_color: Color::WHITE,
        icon_color: Color::WHITE,
    }
}

/// Launch the pie menu at a specific screen position
/// If position is None, centers on screen
pub fn show_pie_menu_at(apps: Vec<AppInfo>, position: Option<(f32, f32)>) {
    println!("Launching pie menu with {} apps at {:?}", apps.len(), position);

    let _ = cosmic::iced::daemon(PieMenuApp::title, PieMenuApp::update, PieMenuApp::view)
        .subscription(PieMenuApp::subscription)
        .theme(PieMenuApp::theme)
        .style(app_style)
        .run_with(move || PieMenuApp::new_at(apps, position));
}

// ============================================================================
// Cursor Tracking Phase
// ============================================================================

/// Messages for the cursor tracker
#[derive(Debug, Clone)]
enum TrackerMessage {
    /// Mouse position captured
    CursorCaptured(f32, f32),
    /// Close without capturing (escape pressed)
    Cancel,
    /// Tick for timeout
    Tick,
}

/// Full-screen transparent overlay to capture cursor position
struct CursorTracker {
    captured: bool,
    tick_count: u32,
    /// Shared cursor position from draw() method
    cursor_pos: Arc<Mutex<Option<(f32, f32)>>>,
}

impl CursorTracker {
    fn new() -> (Self, Task<TrackerMessage>) {
        // Create a full-screen layer surface at overlay level
        let mut settings = SctkLayerSurfaceSettings::default();
        settings.keyboard_interactivity = KeyboardInteractivity::Exclusive;
        settings.layer = Layer::Overlay;
        // Full screen - anchor to all edges
        settings.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
        settings.size = Some((None, None)); // Fill available space
        settings.exclusive_zone = -1; // Don't reserve space

        // No timeout - wait for mouse movement (user can press Escape to cancel)

        let tracker = Self {
            captured: false,
            tick_count: 0,
            cursor_pos: Arc::new(Mutex::new(None)),
        };

        (tracker, get_layer_surface(settings))
    }

    fn title(&self, _id: Id) -> String {
        String::from("Cursor Tracker")
    }

    fn update(&mut self, message: TrackerMessage) -> Task<TrackerMessage> {
        match message {
            TrackerMessage::CursorCaptured(x, y) => {
                if !self.captured {
                    self.captured = true;
                    println!("Cursor captured at ({}, {})", x, y);

                    // Spawn a new process with the position
                    let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
                    let _ = Command::new(exe)
                        .arg("--pie-at")
                        .arg(format!("{}", x))
                        .arg(format!("{}", y))
                        .spawn();

                    // Exit the tracker
                    std::process::exit(0);
                }
                Task::none()
            }
            TrackerMessage::Cancel => {
                std::process::exit(0);
            }
            TrackerMessage::Tick => {
                self.tick_count += 1;

                // Check if cursor position was captured from draw()
                if !self.captured {
                    if let Ok(guard) = self.cursor_pos.lock() {
                        if let Some((x, y)) = *guard {
                            self.captured = true;
                            println!("Cursor captured from draw at ({}, {})", x, y);
                            let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
                            let _ = Command::new(exe)
                                .arg("--pie-at")
                                .arg(format!("{}", x))
                                .arg(format!("{}", y))
                                .spawn();
                            std::process::exit(0);
                        }
                    }
                }

                // No timeout - wait for mouse movement
                // User can press Escape to cancel
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<TrackerMessage> {
        let keyboard_sub = keyboard::on_key_press(|key, _modifiers| {
            if matches!(key, Key::Named(keyboard::key::Named::Escape)) {
                Some(TrackerMessage::Cancel)
            } else {
                None
            }
        });

        let tick_sub = time::every(Duration::from_millis(50)).map(|_| TrackerMessage::Tick);

        Subscription::batch([keyboard_sub, tick_sub])
    }

    fn view(&self, _id: Id) -> Element<'_, TrackerMessage> {
        use cosmic::iced::widget::{container, text, Column, mouse_area};
        use cosmic::iced::alignment::{Horizontal, Vertical};

        // Full-screen canvas that captures mouse position
        let tracker_canvas = canvas(TrackerCanvas {
            cursor_pos: self.cursor_pos.clone(),
        })
            .width(Length::Fill)
            .height(Length::Fill);

        // Add a centered instruction hint - place BEHIND the canvas so cursor works
        let instruction = container(
            Column::new()
                .push(text("Move mouse to position menu").size(18))
                .push(text("Press Escape to cancel").size(14))
                .align_x(Horizontal::Center)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .style(|_theme| {
            cosmic::iced::widget::container::Style {
                text_color: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.7)),
                background: Some(cosmic::iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.3))),
                ..Default::default()
            }
        });

        // Stack with instruction BEHIND canvas, then wrap in mouse_area for cursor
        let content = cosmic::iced::widget::stack![
            instruction,
            tracker_canvas,
        ];

        // Wrap in mouse_area to set crosshair cursor
        mouse_area(content)
            .interaction(mouse::Interaction::Crosshair)
            .into()
    }

    fn theme(&self, _id: Id) -> Theme {
        if is_dark_mode() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

/// Canvas for the cursor tracker - completely transparent, just captures mouse
struct TrackerCanvas {
    cursor_pos: Arc<Mutex<Option<(f32, f32)>>>,
}

impl Program<TrackerMessage> for TrackerCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<TrackerMessage>) {
        // Capture cursor position on any mouse event
        if let Some(pos) = cursor.position_in(bounds) {
            match event {
                Event::Mouse(_) |
                Event::Keyboard(_) => {
                    // Convert to screen coordinates
                    let screen_x = bounds.x + pos.x;
                    let screen_y = bounds.y + pos.y;
                    return (
                        canvas::event::Status::Captured,
                        Some(TrackerMessage::CursorCaptured(screen_x, screen_y)),
                    );
                }
                _ => {}
            }
        }
        (canvas::event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // Try to capture cursor position from the cursor state
        if let Some(pos) = cursor.position_in(bounds) {
            let screen_x = bounds.x + pos.x;
            let screen_y = bounds.y + pos.y;
            if let Ok(mut guard) = self.cursor_pos.lock() {
                *guard = Some((screen_x, screen_y));
            }
        }

        // Draw a very subtle background so cursor changes work
        // Completely transparent surfaces sometimes don't register for cursor events
        use cosmic::iced::widget::canvas::Frame;
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(
            Point::new(0.0, 0.0),
            bounds.size(),
            Color::from_rgba(0.0, 0.0, 0.0, 0.01), // Nearly invisible
        );
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        // Show crosshair cursor to indicate "click to place menu here"
        mouse::Interaction::Crosshair
    }
}

/// Style for tracker window - nearly transparent but with slight tint for cursor events
fn tracker_style(_state: &CursorTracker, _theme: &Theme) -> cosmic::iced_runtime::Appearance {
    cosmic::iced_runtime::Appearance {
        background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.01), // Nearly invisible
        text_color: Color::WHITE,
        icon_color: Color::WHITE,
    }
}

/// Launch the pie menu with cursor tracking
/// Shows an invisible full-screen overlay to capture cursor position first
pub fn show_pie_menu_with_tracking(_apps: Vec<AppInfo>) {
    println!("Starting cursor tracking overlay...");

    let _ = cosmic::iced::daemon(CursorTracker::title, CursorTracker::update, CursorTracker::view)
        .subscription(CursorTracker::subscription)
        .theme(CursorTracker::theme)
        .style(tracker_style)
        .run_with(CursorTracker::new);
}
