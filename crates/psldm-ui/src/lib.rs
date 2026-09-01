// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user interface that the greeter and the locker share.
//!
//! This module holds the state machine only. It has no toolkit code, so the
//! tests run without a display. The widgets read this state, and the two
//! binaries draw the same widgets.
//!
//! One [`Mode`] value is the only difference between the greeter and the
//! locker. The mode hides the power menu and the user picker in the locker.

pub mod app;
pub mod avatar;
pub mod background;
pub mod host;
pub mod pane;
pub mod state;

pub use app::{AppSetup, run};
pub use host::HostKind;
pub use pane::{LoginPane, PowerAction, UserInfo};
pub use state::{LoginState, Message, MessageKind, Phase, Prompt, UiAction};

use std::path::PathBuf;

/// The stylesheet that the greeter and the locker share.
pub const STYLE: &str = include_str!("../../../assets/style.css");

/// The look of the background.
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// The wallpaper file. A missing file gives a dark gradient.
    pub wallpaper: Option<PathBuf>,
    /// The blur radius in pixels. Zero keeps the wallpaper sharp, as macOS
    /// shows it.
    pub blur: f64,
    /// The opacity of the dark layer over the wallpaper, from 0.0 to 1.0.
    pub scrim: f64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            wallpaper: None,
            blur: 0.0,
            scrim: 0.28,
        }
    }
}

/// Which program shows the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The display manager. It logs a user in.
    Greet,
    /// The screen locker. It returns to a session that already runs.
    Lock,
}

/// The parts of the interface that the mode can hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    /// Show the shutdown and restart buttons.
    pub power_menu: bool,
    /// Show the list of users.
    pub user_picker: bool,
    /// Show the list of sessions.
    pub session_picker: bool,
}

impl Mode {
    /// Get the parts of the interface for this mode.
    ///
    /// The locker knows the user, and it must not offer a power menu, because
    /// the screen is locked.
    pub fn chrome(self) -> Chrome {
        match self {
            Mode::Greet => Chrome {
                power_menu: true,
                user_picker: true,
                session_picker: true,
            },
            Mode::Lock => Chrome {
                power_menu: false,
                user_picker: false,
                session_picker: false,
            },
        }
    }
}
