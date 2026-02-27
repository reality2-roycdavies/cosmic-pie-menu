//! COSMIC Panel Applet module for cosmic-pie-menu
//!
//! Provides a native COSMIC panel applet that:
//! - Shows a panel icon for the pie menu
//! - Offers a popup with "Show Pie Menu" and "Settings" buttons
//! - Runs gesture detection in a background thread
//! - Spawns the pie menu as a subprocess when triggered

use cosmic::app::Core;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::Limits;
use cosmic::iced::{time, Subscription, Task};
use cosmic::iced_runtime::core::window;
use cosmic::{Action, Element};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::config::{GestureConfig, PieMenuConfig};

const APP_ID: &str = "io.github.reality2_roycdavies.cosmic-pie-menu";

/// Messages sent from the gesture detection thread to the applet
#[derive(Debug, Clone)]
pub enum GestureMessage {
    /// Pie menu should be shown (gesture completed)
    ShowPieMenu,
    /// Fingers touched down (for potential visual feedback)
    FingersDown,
    /// Gesture was cancelled or menu closed
    Reset,
}

/// Applet UI messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Poll for gesture events from the background thread
    PollGestureEvents,
    /// Show the pie menu (from gesture or popup button)
    ShowPieMenu,
    /// Toggle the popup menu
    TogglePopup,
    /// Popup was closed
    PopupClosed(Id),
    /// Open the settings window
    OpenSettings,
}

pub struct PieMenuApplet {
    core: Core,
    popup: Option<Id>,
    gesture_rx: mpsc::Receiver<GestureMessage>,
    gesture_active: bool,
}

impl cosmic::Application for PieMenuApplet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let pie_config = PieMenuConfig::load();
        let shared_config: Arc<RwLock<GestureConfig>> =
            Arc::new(RwLock::new(GestureConfig::from(&pie_config)));

        let (tx, rx) = mpsc::channel();

        // Start gesture detection in background thread
        match crate::gesture::start_gesture_thread(tx, shared_config) {
            Ok(()) => println!(
                "Gesture detection started ({}-finger tap)",
                pie_config.finger_count
            ),
            Err(e) => eprintln!("Gesture detection not available: {}", e),
        }

        let applet = PieMenuApplet {
            core,
            popup: None,
            gesture_rx: rx,
            gesture_active: false,
        };

        (applet, Task::none())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // Poll gesture channel every 100ms
        time::every(Duration::from_millis(100)).map(|_| Message::PollGestureEvents)
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::PollGestureEvents => {
                // Drain all pending gesture messages
                while let Ok(gesture_msg) = self.gesture_rx.try_recv() {
                    match gesture_msg {
                        GestureMessage::ShowPieMenu => {
                            self.gesture_active = false;
                            spawn_pie_menu();
                        }
                        GestureMessage::FingersDown => {
                            self.gesture_active = true;
                        }
                        GestureMessage::Reset => {
                            self.gesture_active = false;
                        }
                    }
                }
            }
            Message::ShowPieMenu => {
                // Close popup first, then spawn pie menu
                let task = if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    Task::none()
                };
                spawn_pie_menu();
                return task;
            }
            Message::TogglePopup => {
                return if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let main_id = self.core.main_window_id().unwrap_or_else(Id::unique);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        main_id,
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(250.0)
                        .min_width(200.0)
                        .min_height(100.0)
                        .max_height(300.0);
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::OpenSettings => {
                // Close popup first
                let task = if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    Task::none()
                };
                spawn_settings();
                return task;
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("io.github.reality2_roycdavies.cosmic-pie-menu-symbolic")
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let content = cosmic::widget::column::with_children(vec![
            cosmic::applet::menu_button(cosmic::widget::text::body("Show Pie Menu"))
                .on_press(Message::ShowPieMenu)
                .into(),
            cosmic::applet::menu_button(cosmic::widget::text::body("Settings..."))
                .on_press(Message::OpenSettings)
                .into(),
        ]);

        self.core.applet.popup_container(content).into()
    }
}

/// Spawn the pie menu as a subprocess
fn spawn_pie_menu() {
    // Kill any existing pie menu instances first
    let _ = Command::new("pkill")
        .args(["-f", "cosmic-pie-menu --track"])
        .output();
    let _ = Command::new("pkill")
        .args(["-f", "cosmic-pie-menu --pie-at"])
        .output();

    println!("Launching pie menu overlay...");
    let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
    if let Err(e) = Command::new(exe).arg("--track").spawn() {
        eprintln!("Failed to launch pie menu: {}", e);
    }
}

/// Spawn the settings window as a subprocess
fn spawn_settings() {
    // Try unified settings hub first, fall back to standalone
    let unified = Command::new("cosmic-applet-settings")
        .arg(APP_ID)
        .spawn();
    if unified.is_err() {
        let exe = std::env::current_exe().unwrap_or_else(|_| "cosmic-pie-menu".into());
        if let Err(e) = Command::new(exe).arg("--settings-standalone").spawn() {
            eprintln!("Failed to open settings: {}", e);
        }
    }
}

/// Entry point for the applet (called from main when no args)
pub fn run_applet() -> cosmic::iced::Result {
    cosmic::applet::run::<PieMenuApplet>(())?;
    Ok(())
}
