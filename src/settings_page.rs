//! Embeddable settings page for cosmic-pie-menu
//!
//! Provides the settings UI as standalone State/Message/init/update/view
//! functions that can be embedded in cosmic-applet-settings or wrapped
//! in a standalone Application window.

use cosmic::iced::Length;
use cosmic::widget::{self, settings, text, dropdown};
use cosmic::Element;

use crate::config::{PieMenuConfig, SwipeAction, WorkspaceLayout, read_workspace_layout};

const FINGER_OPTIONS: &[&str] = &["3 fingers", "4 fingers"];

const SWIPE_ACTION_OPTIONS: &[&str] = &[
    "None (system default)",
    "App Library",
    "Launcher",
    "Workspaces",
    "Pie Menu",
];

fn swipe_action_to_index(action: SwipeAction) -> usize {
    SwipeAction::all()
        .iter()
        .position(|&a| a == action)
        .unwrap_or(0)
}

fn index_to_swipe_action(index: usize) -> SwipeAction {
    SwipeAction::all()
        .get(index)
        .copied()
        .unwrap_or_default()
}

pub struct State {
    pub config: PieMenuConfig,
    pub finger_index: usize,
    pub swipe_up_index: usize,
    pub swipe_down_index: usize,
    pub swipe_left_index: usize,
    pub swipe_right_index: usize,
    pub workspace_layout: WorkspaceLayout,
}

#[derive(Debug, Clone)]
pub enum Message {
    FingerCountChanged(usize),
    TapDurationChanged(f32),
    MovementThresholdChanged(f32),
    SwipeThresholdChanged(f32),
    SwipeUpChanged(usize),
    SwipeDownChanged(usize),
    SwipeLeftChanged(usize),
    SwipeRightChanged(usize),
    ShowBackgroundToggled(bool),
    IconOnlyHighlightToggled(bool),
    MiddleClickToggled(bool),
    ResetDefaults,
}

pub fn init() -> State {
    let config = PieMenuConfig::load();
    let finger_index = if config.finger_count == 3 { 0 } else { 1 };
    let workspace_layout = read_workspace_layout();

    State {
        finger_index,
        swipe_up_index: swipe_action_to_index(config.swipe_up),
        swipe_down_index: swipe_action_to_index(config.swipe_down),
        swipe_left_index: swipe_action_to_index(config.swipe_left),
        swipe_right_index: swipe_action_to_index(config.swipe_right),
        config,
        workspace_layout,
    }
}

pub fn update(state: &mut State, message: Message) {
    match message {
        Message::FingerCountChanged(index) => {
            state.finger_index = index;
            state.config.finger_count = if index == 0 { 3 } else { 4 };
            let _ = state.config.save();
        }
        Message::TapDurationChanged(value) => {
            state.config.tap_duration_ms = value as u64;
            let _ = state.config.save();
        }
        Message::MovementThresholdChanged(value) => {
            state.config.tap_movement = value as i32;
            let _ = state.config.save();
        }
        Message::SwipeThresholdChanged(value) => {
            state.config.swipe_threshold = value as i32;
            let _ = state.config.save();
        }
        Message::SwipeUpChanged(index) => {
            state.swipe_up_index = index;
            state.config.swipe_up = index_to_swipe_action(index);
            let _ = state.config.save();
        }
        Message::SwipeDownChanged(index) => {
            state.swipe_down_index = index;
            state.config.swipe_down = index_to_swipe_action(index);
            let _ = state.config.save();
        }
        Message::SwipeLeftChanged(index) => {
            state.swipe_left_index = index;
            state.config.swipe_left = index_to_swipe_action(index);
            let _ = state.config.save();
        }
        Message::SwipeRightChanged(index) => {
            state.swipe_right_index = index;
            state.config.swipe_right = index_to_swipe_action(index);
            let _ = state.config.save();
        }
        Message::ShowBackgroundToggled(enabled) => {
            state.config.show_background = enabled;
            let _ = state.config.save();
        }
        Message::IconOnlyHighlightToggled(enabled) => {
            state.config.icon_only_highlight = enabled;
            let _ = state.config.save();
        }
        Message::MiddleClickToggled(enabled) => {
            state.config.middle_click_trigger = enabled;
            let _ = state.config.save();
        }
        Message::ResetDefaults => {
            state.config = PieMenuConfig::default();
            state.finger_index = if state.config.finger_count == 3 { 0 } else { 1 };
            state.swipe_up_index = swipe_action_to_index(state.config.swipe_up);
            state.swipe_down_index = swipe_action_to_index(state.config.swipe_down);
            state.swipe_left_index = swipe_action_to_index(state.config.swipe_left);
            state.swipe_right_index = swipe_action_to_index(state.config.swipe_right);
            let _ = state.config.save();
        }
    }
}

pub fn view(state: &State) -> Element<'_, Message> {
    let page_title = text::title1("Gesture Settings");

    let gesture_section = settings::section()
        .title("Gesture Detection")
        .add(
            settings::item(
                "Finger Count",
                dropdown(
                    FINGER_OPTIONS,
                    Some(state.finger_index),
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
                    .push(text::body(format!("{}ms", state.config.tap_duration_ms)))
                    .push(
                        widget::slider(
                            100.0..=500.0,
                            state.config.tap_duration_ms as f32,
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
                    .push(text::body(format!("{} units", state.config.tap_movement)))
                    .push(
                        widget::slider(
                            200.0..=1000.0,
                            state.config.tap_movement as f32,
                            Message::MovementThresholdChanged,
                        )
                        .step(50.0)
                        .width(Length::Fill)
                    ),
            )
        )
        .add(
            settings::item(
                "Middle Mouse Click",
                widget::toggler(state.config.middle_click_trigger)
                    .on_toggle(Message::MiddleClickToggled),
            )
        );

    let (layout_name, available_directions) = match state.workspace_layout {
        WorkspaceLayout::Horizontal => ("horizontal", "up/down"),
        WorkspaceLayout::Vertical => ("vertical", "left/right"),
    };

    let mut swipe_section = settings::section()
        .title("Swipe Actions");

    match state.workspace_layout {
        WorkspaceLayout::Horizontal => {
            swipe_section = swipe_section
                .add(
                    settings::item(
                        "Swipe Up",
                        dropdown(
                            SWIPE_ACTION_OPTIONS,
                            Some(state.swipe_up_index),
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
                            Some(state.swipe_down_index),
                            Message::SwipeDownChanged,
                        )
                        .width(Length::Fixed(200.0)),
                    )
                );
        }
        WorkspaceLayout::Vertical => {
            swipe_section = swipe_section
                .add(
                    settings::item(
                        "Swipe Left",
                        dropdown(
                            SWIPE_ACTION_OPTIONS,
                            Some(state.swipe_left_index),
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
                            Some(state.swipe_right_index),
                            Message::SwipeRightChanged,
                        )
                        .width(Length::Fixed(200.0)),
                    )
                );
        }
    }

    swipe_section = swipe_section.add(
        settings::flex_item(
            "Swipe Threshold",
            widget::row()
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
                .push(text::body(format!("{} units", state.config.swipe_threshold)))
                .push(
                    widget::slider(
                        100.0..=600.0,
                        state.config.swipe_threshold as f32,
                        Message::SwipeThresholdChanged,
                    )
                    .step(25.0)
                    .width(Length::Fill)
                ),
        )
    );

    let appearance_section = settings::section()
        .title("Appearance")
        .add(
            settings::item(
                "Show Background",
                widget::toggler(state.config.show_background)
                    .on_toggle(Message::ShowBackgroundToggled),
            )
        )
        .add(
            settings::item(
                "Icon-Only Highlight",
                widget::toggler(state.config.icon_only_highlight)
                    .on_toggle(Message::IconOnlyHighlightToggled),
            )
        );

    let reset_button = widget::button::standard("Reset to Defaults")
        .on_press(Message::ResetDefaults);

    settings::view_column(vec![
        page_title.into(),
        text::caption("Configure how the touchpad gesture triggers the pie menu. Changes are saved automatically.").into(),
        gesture_section.into(),
        text::caption(format!(
            "Your workspace layout is {}. Swipe {} to configure custom actions.",
            layout_name, available_directions
        )).into(),
        swipe_section.into(),
        text::caption("Customize the visual appearance of the pie menu.").into(),
        appearance_section.into(),
        widget::container(reset_button)
            .padding([16, 0, 0, 0])
            .into(),
    ])
    .into()
}
