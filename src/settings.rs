//! Settings application for cosmic-pie-menu
//!
//! A libcosmic-based settings window for configuring gesture detection.

use cosmic::app::Core;
use cosmic::iced::Length;
use cosmic::widget::{self, settings, text, dropdown};
use cosmic::{Action, Application, Element, Task};

use crate::config::PieMenuConfig;

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
    /// Reset to defaults
    ResetDefaults,
}

/// Finger count options for dropdown
const FINGER_OPTIONS: &[&str] = &["3 fingers", "4 fingers"];

/// Settings application state
pub struct SettingsApp {
    core: Core,
    config: PieMenuConfig,
    /// Selected finger count index (0 = 3 fingers, 1 = 4 fingers)
    finger_index: usize,
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
        vec![text::heading("Pie Menu Settings").into()]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        vec![]
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let config = PieMenuConfig::load();
        let finger_index = if config.finger_count == 3 { 0 } else { 1 };

        (
            Self {
                core,
                config,
                finger_index,
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
            Message::ResetDefaults => {
                self.config = PieMenuConfig::default();
                self.finger_index = if self.config.finger_count == 3 { 0 } else { 1 };
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

        // Reset button
        let reset_button = widget::button::standard("Reset to Defaults")
            .on_press(Message::ResetDefaults);

        // Use settings::view_column for proper COSMIC styling
        let content = settings::view_column(vec![
            page_title.into(),
            text::caption("Configure how the touchpad gesture triggers the pie menu. Lower duration requires quicker taps. Higher movement threshold allows more finger movement during the tap. Changes are saved automatically.").into(),
            gesture_section.into(),
            widget::container(reset_button)
                .padding([16, 0, 0, 0])
                .into(),
        ]);

        widget::container(
            widget::container(content)
                .max_width(600)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .padding(16)
        .into()
    }
}

/// Run the settings application
pub fn run_settings(_shared_config: Option<crate::config::SharedConfig>) {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(500.0, 420.0));

    let _ = cosmic::app::run::<SettingsApp>(settings, ());
}
