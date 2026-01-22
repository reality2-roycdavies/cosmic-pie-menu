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
use cosmic::iced_core::layout::Limits;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use std::f32::consts::PI;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::sync::{Arc, Mutex};

use crate::apps::{AppInfo, find_icon_path};

/// Icon size for the pie menu
const ICON_SIZE: u16 = 48;

/// Minimum radius of the pie menu circle (for small number of apps)
const MIN_MENU_RADIUS: f32 = 100.0;

/// Inner radius (for the center area)
const INNER_RADIUS: f32 = 50.0;

/// Spacing between icons on the circumference
const ICON_SPACING: f32 = 100.0;

/// Calculate menu radius based on number of apps
/// Ensures icons have enough space around the circle
fn calculate_menu_radius(num_apps: usize) -> f32 {
    if num_apps == 0 {
        return MIN_MENU_RADIUS;
    }
    // Circumference needed = num_apps * spacing
    // Circumference = 2 * PI * radius
    // So radius = (num_apps * spacing) / (2 * PI)
    let calculated = (num_apps as f32 * ICON_SPACING) / (2.0 * PI);
    calculated.max(MIN_MENU_RADIUS)
}

/// Theme colors for the pie menu
/// TODO: Integrate with COSMIC theme system for light/dark mode
struct PieTheme {
    /// Background color of the pie
    bg_color: Color,
    /// Color of a segment when not hovered
    segment_color: Color,
    /// Color of a segment when hovered (subtle shift)
    segment_hover_color: Color,
    /// Center area background
    center_color: Color,
    /// Border/divider color
    border_color: Color,
    /// Icon background color
    icon_bg_color: Color,
    /// Icon background when hovered
    icon_bg_hover_color: Color,
    /// Text color
    text_color: Color,
    /// Subtle text color (for close button, etc.)
    text_subtle_color: Color,
}

impl PieTheme {
    /// Dark theme (default)
    fn dark() -> Self {
        Self {
            bg_color: Color::from_rgba(0.10, 0.10, 0.12, 0.95),
            segment_color: Color::from_rgba(0.14, 0.14, 0.16, 0.95),
            segment_hover_color: Color::from_rgba(0.22, 0.22, 0.26, 0.95),
            center_color: Color::from_rgba(0.12, 0.12, 0.14, 0.98),
            border_color: Color::from_rgba(0.25, 0.25, 0.28, 0.6),
            icon_bg_color: Color::from_rgba(0.18, 0.18, 0.22, 0.7),
            icon_bg_hover_color: Color::from_rgba(0.28, 0.28, 0.34, 0.8),
            text_color: Color::from_rgba(0.95, 0.95, 0.95, 1.0),
            text_subtle_color: Color::from_rgba(0.6, 0.6, 0.6, 0.9),
        }
    }

    /// Light theme (for future use)
    #[allow(dead_code)]
    fn light() -> Self {
        Self {
            bg_color: Color::from_rgba(0.95, 0.95, 0.96, 0.95),
            segment_color: Color::from_rgba(0.92, 0.92, 0.93, 0.95),
            segment_hover_color: Color::from_rgba(0.85, 0.85, 0.88, 0.95),
            center_color: Color::from_rgba(0.98, 0.98, 0.98, 0.98),
            border_color: Color::from_rgba(0.75, 0.75, 0.78, 0.5),
            icon_bg_color: Color::from_rgba(0.88, 0.88, 0.90, 0.7),
            icon_bg_hover_color: Color::from_rgba(0.80, 0.80, 0.84, 0.8),
            text_color: Color::from_rgba(0.1, 0.1, 0.1, 1.0),
            text_subtle_color: Color::from_rgba(0.4, 0.4, 0.4, 0.9),
        }
    }

    /// Get theme based on system preference
    /// TODO: Hook into COSMIC theme detection
    fn current() -> Self {
        // For now, always use dark theme
        // Later: check cosmic::theme::is_dark() or similar
        Self::dark()
    }
}

/// Messages for the pie menu
#[derive(Debug, Clone)]
pub enum Message {
    /// An app was clicked
    LaunchApp(usize),
    /// Mouse moved - update hover state
    MouseMoved(Point),
    /// Close the menu
    Close,
    /// Layer surface created
    LayerReady,
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
    ClickCenter,
}

/// App data with pre-calculated position
struct AppSlice {
    index: usize,
    name: String,
    icon_path: Option<PathBuf>,
    angle: f32,           // Center angle of this slice
    start_angle: f32,     // Start of slice
    end_angle: f32,       // End of slice
    icon_pos: Point,      // Position for icon center
    is_running: bool,     // Whether app is currently running
    is_favorite: bool,    // Whether app is a dock favorite
}

/// State for the pie menu application
struct PieMenuApp {
    apps: Vec<AppInfo>,
    slices: Vec<AppSlice>,
    hovered_slice: Option<usize>,
    center: Point,
    tick_count: u32,  // Count ticks to trigger redraws on scaled displays
    /// Position mode: None = centered window, Some = full-screen with menu at position
    cursor_position: Option<(f32, f32)>,
    /// Whether we're in full-screen mode (for cursor-positioned menu)
    full_screen: bool,
    /// Dynamic menu radius based on number of apps
    menu_radius: f32,
}

impl PieMenuApp {
    fn new(apps: Vec<AppInfo>) -> (Self, Task<Message>) {
        Self::new_at(apps, None)
    }

    fn new_at(apps: Vec<AppInfo>, position: Option<(f32, f32)>) -> (Self, Task<Message>) {
        let menu_radius = calculate_menu_radius(apps.len());
        let menu_size = (menu_radius * 2.0 + ICON_SIZE as f32 + 80.0) as f32;
        // Always use full-screen mode for better layer surface compatibility
        let full_screen = true;

        let mut settings = SctkLayerSurfaceSettings::default();
        settings.keyboard_interactivity = KeyboardInteractivity::OnDemand;
        settings.layer = Layer::Top;

        // Full-screen anchored surface - draw menu at center or cursor position
        settings.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
        settings.size = Some((None, None)); // Fill available space
        settings.exclusive_zone = -1;

        // Center will be calculated during draw based on surface size
        // If position is provided, use that; otherwise center of screen
        let center = Point::new(0.0, 0.0); // Placeholder, will be updated in draw

        // Pre-calculate slice data (positions will be adjusted during draw if full-screen)
        let num_apps = apps.len();
        let slices: Vec<AppSlice> = apps
            .iter()
            .enumerate()
            .map(|(i, app)| {
                let slice_angle = 2.0 * PI / num_apps as f32;
                // Start from top (-PI/2), go clockwise
                let angle = -PI / 2.0 + (i as f32 * slice_angle);
                let start_angle = angle - slice_angle / 2.0;
                let end_angle = angle + slice_angle / 2.0;

                // Calculate icon position on the circle (at 3/4 mark, closer to outer edge)
                let icon_radius = INNER_RADIUS + (menu_radius - INNER_RADIUS) * 0.65;
                let icon_pos = Point::new(
                    center.x + icon_radius * angle.cos(),
                    center.y + icon_radius * angle.sin(),
                );

                let icon_path = app.icon.as_ref()
                    .and_then(|name| find_icon_path(name, ICON_SIZE));

                AppSlice {
                    index: i,
                    name: app.name.clone(),
                    icon_path,
                    angle,
                    start_angle,
                    end_angle,
                    icon_pos,
                    is_running: app.is_running,
                    is_favorite: app.is_favorite,
                }
            })
            .collect();

        let app = Self {
            apps,
            slices,
            hovered_slice: None,
            center,
            tick_count: 0,
            cursor_position: position,
            full_screen,
            menu_radius,
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
                        let parts: Vec<&str> = exec_clean.split_whitespace().collect();
                        if let Some(program) = parts.first() {
                            let args: Vec<&str> = parts.iter().skip(1).copied().collect();
                            let _ = Command::new(program).args(&args).spawn();
                        }
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
                return self.update(Message::LaunchApp(index));
            }
            Message::CanvasEvent(PieCanvasMessage::ClickCenter) => {
                return self.update(Message::Close);
            }
            Message::MouseMoved(_) => Task::none(),
            Message::LayerReady => {
                println!("Layer surface ready");
                Task::none()
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
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let keyboard_sub = keyboard::on_key_press(|key, _modifiers| Some(Message::KeyPressed(key)));

        // Send ticks for the first 500ms to trigger layout recalculation on scaled displays
        if self.tick_count < 10 {
            let tick_sub = time::every(Duration::from_millis(50)).map(|_| Message::Tick);
            Subscription::batch([keyboard_sub, tick_sub])
        } else {
            keyboard_sub
        }
    }

    fn view(&self, _id: Id) -> Element<'_, Message> {
        let menu_size = (self.menu_radius * 2.0 + ICON_SIZE as f32 + 80.0) as f32;

        // Get hovered app name for center display
        let hovered_name = self.hovered_slice
            .and_then(|i| self.slices.get(i))
            .map(|s| s.name.clone())
            .unwrap_or_default();

        let pie_canvas = canvas(PieCanvas {
            slices: &self.slices,
            hovered: self.hovered_slice,
            cursor_position: self.cursor_position,
            full_screen: self.full_screen,
            menu_radius: self.menu_radius,
            hovered_name,
        });

        // In full-screen mode, fill the screen; otherwise fixed size
        if self.full_screen {
            pie_canvas.width(Length::Fill).height(Length::Fill).into()
        } else {
            pie_canvas.width(Length::Fixed(menu_size)).height(Length::Fixed(menu_size)).into()
        }
    }

    fn theme(&self, _id: Id) -> Theme {
        Theme::Dark
    }
}

/// Canvas widget for rendering the pie menu
struct PieCanvas<'a> {
    slices: &'a [AppSlice],
    hovered: Option<usize>,
    /// If Some, draw the menu centered at this position; if None, center in bounds
    cursor_position: Option<(f32, f32)>,
    /// Whether we're in full-screen mode
    full_screen: bool,
    /// Dynamic menu radius
    menu_radius: f32,
    /// Name of hovered app (to display in center)
    hovered_name: String,
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

        let menu_size = (self.menu_radius * 2.0 + ICON_SIZE as f32 + 80.0) as f32;

        // Determine center point: cursor position or center of bounds
        let center = if let Some((cx, cy)) = self.cursor_position {
            // Clamp to keep menu fully visible (same logic as draw)
            let half_menu = menu_size / 2.0;
            let x = cx.clamp(half_menu, bounds.width - half_menu);
            let y = cy.clamp(half_menu, bounds.height - half_menu);
            Point::new(x, y)
        } else {
            Point::new(bounds.width / 2.0, bounds.height / 2.0)
        };
        let dx = cursor_pos.x - center.x;
        let dy = cursor_pos.y - center.y;
        let distance = (dx * dx + dy * dy).sqrt();

        // Check if in center (close button area)
        if distance < INNER_RADIUS {
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
        if distance > self.menu_radius + 20.0 {
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
        // Expected size for our pie menu
        let menu_size = (self.menu_radius * 2.0 + ICON_SIZE as f32 + 80.0) as f32;

        // In fixed-size mode, skip drawing if bounds are wrong (scaling issue)
        if !self.full_screen {
            if (bounds.width - menu_size).abs() > 1.0 || (bounds.height - menu_size).abs() > 1.0 {
                return vec![];
            }
        }

        use cosmic::iced::widget::canvas::Frame;
        let mut frame = Frame::new(renderer, bounds.size());

        {
            // Determine center point: cursor position or center of bounds
            let center = if let Some((cx, cy)) = self.cursor_position {
                // Clamp to keep menu fully visible
                let half_menu = menu_size / 2.0;
                let x = cx.clamp(half_menu, bounds.width - half_menu);
                let y = cy.clamp(half_menu, bounds.height - half_menu);
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

            // Draw background circle
            let bg_circle = Path::circle(center, self.menu_radius + 10.0);
            frame.fill(&bg_circle, theme.bg_color);

            // Draw each slice segment as an annular (ring) segment
            for slice in self.slices {
                let is_hovered = self.hovered == Some(slice.index);

                // Draw the annular segment (donut slice between inner and outer radius)
                let outer_radius = self.menu_radius + 5.0;
                let inner_radius = INNER_RADIUS + 2.0;

                // Calculate the 4 corners of the annular segment
                let outer_start = Point::new(
                    center.x + outer_radius * slice.start_angle.cos(),
                    center.y + outer_radius * slice.start_angle.sin(),
                );
                let outer_end = Point::new(
                    center.x + outer_radius * slice.end_angle.cos(),
                    center.y + outer_radius * slice.end_angle.sin(),
                );
                let inner_end = Point::new(
                    center.x + inner_radius * slice.end_angle.cos(),
                    center.y + inner_radius * slice.end_angle.sin(),
                );
                let inner_start = Point::new(
                    center.x + inner_radius * slice.start_angle.cos(),
                    center.y + inner_radius * slice.start_angle.sin(),
                );

                // Build the annular segment path
                // We'll use multiple points along the arcs for smooth curves
                let segment = Path::new(|builder| {
                    // Start at outer edge, start angle
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

                    // Line to inner edge at end angle
                    builder.line_to(inner_end);

                    // Draw inner arc back (reverse direction)
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

                // Subtle color shift for hover
                let segment_color = if is_hovered {
                    theme.segment_hover_color
                } else {
                    theme.segment_color
                };
                frame.fill(&segment, segment_color);

                // Draw slice divider line
                let divider = Path::new(|builder| {
                    let inner_x = center.x + inner_radius * slice.start_angle.cos();
                    let inner_y = center.y + inner_radius * slice.start_angle.sin();
                    let outer_x = center.x + outer_radius * slice.start_angle.cos();
                    let outer_y = center.y + outer_radius * slice.start_angle.sin();
                    builder.move_to(Point::new(inner_x, inner_y));
                    builder.line_to(Point::new(outer_x, outer_y));
                });
                frame.stroke(
                    &divider,
                    Stroke::default()
                        .with_color(theme.border_color)
                        .with_width(1.0),
                );

                // Calculate icon position (at 3/4 mark, closer to outer edge)
                let icon_radius = INNER_RADIUS + (self.menu_radius - INNER_RADIUS) * 0.65;
                let icon_center = Point::new(
                    center.x + icon_radius * slice.angle.cos(),
                    center.y + icon_radius * slice.angle.sin(),
                );

                // Icon background circle
                let icon_bg = Path::circle(icon_center, ICON_SIZE as f32 / 2.0 + 4.0);
                let icon_bg_color = if is_hovered {
                    theme.icon_bg_hover_color
                } else {
                    theme.icon_bg_color
                };
                frame.fill(&icon_bg, icon_bg_color);

                // Draw the icon or fallback to letter
                let icon_size = ICON_SIZE as f32;
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

                // Draw running indicator (arc near inner circle)
                if slice.is_running {
                    let arc_radius = INNER_RADIUS + 8.0;
                    // Shorten the arc slightly to leave gaps between slices
                    let arc_padding = 0.08; // radians
                    let arc_start = slice.start_angle + arc_padding;
                    let arc_end = slice.end_angle - arc_padding;

                    if arc_end > arc_start {
                        let arc = Path::new(|builder| {
                            // Draw arc using line segments
                            let steps = 12;
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
                                .with_color(Color::from_rgb(0.5, 0.5, 0.5))
                                .with_width(3.0),
                        );
                    }
                }
            }

            // Draw inner circle (center area)
            let inner_circle = Path::circle(center, INNER_RADIUS);
            frame.fill(&inner_circle, theme.center_color);

            // Inner circle border
            frame.stroke(
                &inner_circle,
                Stroke::default()
                    .with_color(theme.border_color)
                    .with_width(1.5),
            );

            // Draw hovered app name in center, wrapped by words
            if !self.hovered_name.is_empty() {
                let words: Vec<&str> = self.hovered_name.split_whitespace().collect();
                let line_height = 16.0;
                let total_height = words.len() as f32 * line_height;
                let start_y = center.y - total_height / 2.0 + line_height / 2.0;

                for (i, word) in words.iter().enumerate() {
                    frame.fill_text(Text {
                        content: word.to_string(),
                        position: Point::new(center.x, start_y + i as f32 * line_height),
                        color: theme.text_color,
                        size: 13.0.into(),
                        font: Font::DEFAULT,
                        horizontal_alignment: Horizontal::Center,
                        vertical_alignment: Vertical::Center,
                        ..Text::default()
                    });
                }
            }

            // Draw outer border
            let outer_border = Path::circle(center, self.menu_radius + 10.0);
            frame.stroke(
                &outer_border,
                Stroke::default()
                    .with_color(theme.border_color)
                    .with_width(2.0),
            );
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

/// Launch the pie menu with the given apps (centered on screen)
pub fn show_pie_menu(apps: Vec<AppInfo>) {
    show_pie_menu_at(apps, None);
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
    fn new(_apps: Vec<AppInfo>) -> (Self, Task<TrackerMessage>) {
        // Create a full-screen layer surface at overlay level
        let mut settings = SctkLayerSurfaceSettings::default();
        settings.keyboard_interactivity = KeyboardInteractivity::Exclusive;
        settings.layer = Layer::Overlay;
        // Full screen - anchor to all edges
        settings.anchor = Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT;
        settings.size = Some((None, None)); // Fill available space
        settings.exclusive_zone = -1; // Don't reserve space

        // Hard timeout thread - if iced's event loop is stuck, this will still fire
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(800));
            eprintln!("Hard timeout: cursor tracker stuck, falling back to centered menu");
            let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
            let _ = Command::new(exe).arg("--pie").spawn();
            std::process::exit(0);
        });

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

                // Timeout after 500ms - fall back to centered
                if self.tick_count > 10 {
                    println!("Cursor tracking timeout, falling back to centered");
                    let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
                    let _ = Command::new(exe).arg("--pie").spawn();
                    std::process::exit(0);
                }
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
        // Full-screen transparent canvas that captures mouse position
        canvas(TrackerCanvas {
            cursor_pos: self.cursor_pos.clone(),
        })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self, _id: Id) -> Theme {
        Theme::Dark
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

        // Draw nothing - completely transparent
        use cosmic::iced::widget::canvas::Frame;
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(
            Point::new(0.0, 0.0),
            bounds.size(),
            Color::TRANSPARENT,
        );
        vec![frame.into_geometry()]
    }
}

/// Style for transparent tracker window
fn tracker_style(_state: &CursorTracker, _theme: &Theme) -> cosmic::iced_runtime::Appearance {
    cosmic::iced_runtime::Appearance {
        background_color: Color::TRANSPARENT,
        text_color: Color::WHITE,
        icon_color: Color::WHITE,
    }
}

/// Launch the pie menu with cursor tracking
/// Shows an invisible full-screen overlay to capture cursor position first
pub fn show_pie_menu_with_tracking(apps: Vec<AppInfo>) {
    println!("Starting cursor tracking for {} apps", apps.len());

    let _ = cosmic::iced::daemon(CursorTracker::title, CursorTracker::update, CursorTracker::view)
        .subscription(CursorTracker::subscription)
        .theme(CursorTracker::theme)
        .style(tracker_style)
        .run_with(move || CursorTracker::new(apps));
}
