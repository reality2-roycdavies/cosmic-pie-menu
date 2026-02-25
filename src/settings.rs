//! Settings application for cosmic-pie-menu
//!
//! A libcosmic-based settings window for configuring gesture detection and swipe actions.
//! This is a thin wrapper around settings_page for standalone window use.

use cosmic::app::Core;
use cosmic::iced::Length;
use cosmic::widget::{self, container};
use cosmic::{Action, Application, Element, Task};

use crate::settings_page;

pub const APP_ID: &str = "io.github.reality2_roycdavies.cosmic-pie-menu.settings";

pub struct SettingsApp {
    core: Core,
    page: settings_page::State,
}

impl Application for SettingsApp {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = settings_page::Message;

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
        let page = settings_page::init();
        (Self { core, page }, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        settings_page::update(&mut self.page, message);
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let content = settings_page::view(&self.page);

        widget::scrollable(
            container(container(content).max_width(800))
                .width(Length::Fill)
                .center_x(Length::Fill)
                .padding(24),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

pub fn run_settings(_shared_config: Option<crate::config::SharedConfig>) {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(850.0, 700.0));

    let _ = cosmic::app::run::<SettingsApp>(settings, ());
}
