//! Settings application for cosmic-pie-menu
//!
//! A libcosmic-based settings window for configuring gesture detection and swipe actions.
//!
//! # Features
//! - Configure finger count (3 or 4 fingers)
//! - Adjust tap duration and movement thresholds
//! - Set swipe actions for available directions
//! - Respects COSMIC workspace layout (only shows available swipe directions)
//! - Changes are saved automatically

use cosmic::app::Core;
use cosmic::iced::Length;
use cosmic::widget::{self, settings, text, dropdown};
use cosmic::{Action, Application, Element, Task};

use crate::config::{PieMenuConfig, SwipeAction, WorkspaceLayout, read_workspace_layout};

/// Application ID
pub const APP_ID: &str = "io.github.reality2_roycdavies.cosmic-pie-menu.settings";

/// Messages for the settings application
#[derive(Debug, Clone)]
pub enum Message {
    /// Finger count changed (index in dropdown)
    FingerCountChanged(usize),
    /// Tap duration slider changed
    TapDurationChanged(f32),
    /// Movement threshold slider changed
    MovementThresholdChanged(f32),
    /// Swipe threshold slider changed
    SwipeThresholdChanged(f32),
    /// Swipe up action changed
    SwipeUpChanged(usize),
    /// Swipe down action changed
    SwipeDownChanged(usize),
    /// Swipe left action changed
    SwipeLeftChanged(usize),
    /// Swipe right action changed
    SwipeRightChanged(usize),
    /// Reset to defaults
    ResetDefaults,
}

/// Finger count options for dropdown
const FINGER_OPTIONS: &[&str] = &["3 fingers", "4 fingers"];

/// Swipe action options for dropdown (static)
const SWIPE_ACTION_OPTIONS: &[&str] = &[
    "None (system default)",
    "App Library",
    "Launcher",
    "Workspaces",
    "Pie Menu",
];

/// Convert SwipeAction to dropdown index
fn swipe_action_to_index(action: SwipeAction) -> usize {
    SwipeAction::all()
        .iter()
        .position(|&a| a == action)
        .unwrap_or(0)
}

/// Convert dropdown index to SwipeAction
fn index_to_swipe_action(index: usize) -> SwipeAction {
    SwipeAction::all()
        .get(index)
        .copied()
        .unwrap_or_default()
}

/// Settings application state
pub struct SettingsApp {
    core: Core,
    config: PieMenuConfig,
    /// Selected finger count index (0 = 3 fingers, 1 = 4 fingers)
    finger_index: usize,
    /// Swipe action indexes
    swipe_up_index: usize,
    swipe_down_index: usize,
    swipe_left_index: usize,
    swipe_right_index: usize,
    /// Current workspace layout (determines which swipe directions are available)
    workspace_layout: WorkspaceLayout,
}

impl Application for SettingsApp {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![]
    }

    fn header_center(&self) -> Vec<Element<'_, Self::Message>> {
        vec![]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        vec![]
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let config = PieMenuConfig::load();
        let finger_index = if config.finger_count == 3 { 0 } else { 1 };
        let workspace_layout = read_workspace_layout();

        (
            Self {
                core,
                finger_index,
                swipe_up_index: swipe_action_to_index(config.swipe_up),
                swipe_down_index: swipe_action_to_index(config.swipe_down),
                swipe_left_index: swipe_action_to_index(config.swipe_left),
                swipe_right_index: swipe_action_to_index(config.swipe_right),
                config,
                workspace_layout,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::FingerCountChanged(index) => {
                self.finger_index = index;
                self.config.finger_count = if index == 0 { 3 } else { 4 };
                let _ = self.config.save();
            }
            Message::TapDurationChanged(value) => {
                self.config.tap_duration_ms = value as u64;
                let _ = self.config.save();
            }
            Message::MovementThresholdChanged(value) => {
                self.config.tap_movement = value as i32;
                let _ = self.config.save();
            }
            Message::SwipeThresholdChanged(value) => {
                self.config.swipe_threshold = value as i32;
                let _ = self.config.save();
            }
            Message::SwipeUpChanged(index) => {
                self.swipe_up_index = index;
                self.config.swipe_up = index_to_swipe_action(index);
                let _ = self.config.save();
            }
            Message::SwipeDownChanged(index) => {
                self.swipe_down_index = index;
                self.config.swipe_down = index_to_swipe_action(index);
                let _ = self.config.save();
            }
            Message::SwipeLeftChanged(index) => {
                self.swipe_left_index = index;
                self.config.swipe_left = index_to_swipe_action(index);
                let _ = self.config.save();
            }
            Message::SwipeRightChanged(index) => {
                self.swipe_right_index = index;
                self.config.swipe_right = index_to_swipe_action(index);
                let _ = self.config.save();
            }
            Message::ResetDefaults => {
                self.config = PieMenuConfig::default();
                self.finger_index = if self.config.finger_count == 3 { 0 } else { 1 };
                self.swipe_up_index = swipe_action_to_index(self.config.swipe_up);
                self.swipe_down_index = swipe_action_to_index(self.config.swipe_down);
                self.swipe_left_index = swipe_action_to_index(self.config.swipe_left);
                self.swipe_right_index = swipe_action_to_index(self.config.swipe_right);
                let _ = self.config.save();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        // Page title
        let page_title = text::title1("Gesture Settings");

        // Build sections using COSMIC settings widgets
        let gesture_section = settings::section()
            .title("Gesture Detection")
            .add(
                settings::item(
                    "Finger Count",
                    dropdown(
                        FINGER_OPTIONS,
                        Some(self.finger_index),
                        Message::FingerCountChanged,
                    )
                    .width(Length::Fixed(150.0)),
                )
            )
            .add(
                settings::flex_item(
                    "Tap Duration",
                    widget::row()
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center)
                        .push(text::body(format!("{}ms", self.config.tap_duration_ms)))
                        .push(
                            widget::slider(
                                100.0..=500.0,
                                self.config.tap_duration_ms as f32,
                                Message::TapDurationChanged,
                            )
                            .step(10.0)
                            .width(Length::Fill)
                        ),
                )
            )
            .add(
                settings::flex_item(
                    "Movement Threshold",
                    widget::row()
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center)
                        .push(text::body(format!("{} units", self.config.tap_movement)))
                        .push(
                            widget::slider(
                                200.0..=1000.0,
                                self.config.tap_movement as f32,
                                Message::MovementThresholdChanged,
                            )
                            .step(50.0)
                            .width(Length::Fill)
                        ),
                )
            );

        // Swipe actions section - only show directions not used by workspace switching
        // Horizontal workspaces: left/right switch workspaces, so up/down are available
        // Vertical workspaces: up/down switch workspaces, so left/right are available
        let (layout_name, available_directions) = match self.workspace_layout {
            WorkspaceLayout::Horizontal => ("horizontal", "up/down"),
            WorkspaceLayout::Vertical => ("vertical", "left/right"),
        };

        let mut swipe_section = settings::section()
            .title("Swipe Actions");

        // Add available swipe directions based on workspace layout
        match self.workspace_layout {
            WorkspaceLayout::Horizontal => {
                // Horizontal workspaces use left/right for switching, so up/down are available
                swipe_section = swipe_section
                    .add(
                        settings::item(
                            "Swipe Up",
                            dropdown(
                                SWIPE_ACTION_OPTIONS,
                                Some(self.swipe_up_index),
                                Message::SwipeUpChanged,
                            )
                            .width(Length::Fixed(200.0)),
                        )
                    )
                    .add(
                        settings::item(
                            "Swipe Down",
                            dropdown(
                                SWIPE_ACTION_OPTIONS,
                                Some(self.swipe_down_index),
                                Message::SwipeDownChanged,
                            )
                            .width(Length::Fixed(200.0)),
                        )
                    );
            }
            WorkspaceLayout::Vertical => {
                // Vertical workspaces use up/down for switching, so left/right are available
                swipe_section = swipe_section
                    .add(
                        settings::item(
                            "Swipe Left",
                            dropdown(
                                SWIPE_ACTION_OPTIONS,
                                Some(self.swipe_left_index),
                                Message::SwipeLeftChanged,
                            )
                            .width(Length::Fixed(200.0)),
                        )
                    )
                    .add(
                        settings::item(
                            "Swipe Right",
                            dropdown(
                                SWIPE_ACTION_OPTIONS,
                                Some(self.swipe_right_index),
                                Message::SwipeRightChanged,
                            )
                            .width(Length::Fixed(200.0)),
                        )
                    );
            }
        }

        // Add swipe threshold slider to the section
        swipe_section = swipe_section.add(
            settings::flex_item(
                "Swipe Threshold",
                widget::row()
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center)
                    .push(text::body(format!("{} units", self.config.swipe_threshold)))
                    .push(
                        widget::slider(
                            100.0..=600.0,
                            self.config.swipe_threshold as f32,
                            Message::SwipeThresholdChanged,
                        )
                        .step(25.0)
                        .width(Length::Fill)
                    ),
            )
        );

        // Reset button
        let reset_button = widget::button::standard("Reset to Defaults")
            .on_press(Message::ResetDefaults);

        // Use settings::view_column for proper COSMIC styling
        let content = settings::view_column(vec![
            page_title.into(),
            text::caption("Configure how the touchpad gesture triggers the pie menu. Lower duration requires quicker taps. Higher movement threshold allows more finger movement during the tap. Changes are saved automatically.").into(),
            gesture_section.into(),
            text::caption(format!(
                "Your workspace layout is {}. Swipe {} to configure custom actions. Other directions are used for workspace switching.",
                layout_name, available_directions
            )).into(),
            swipe_section.into(),
            widget::container(reset_button)
                .padding([16, 0, 0, 0])
                .into(),
        ]);

        widget::container(
            widget::container(content)
                .max_width(800)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(24)
        .into()
    }
}

/// Run the settings application
pub fn run_settings(_shared_config: Option<crate::config::SharedConfig>) {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(850.0, 700.0));

    let _ = cosmic::app::run::<SettingsApp>(settings, ());
}
