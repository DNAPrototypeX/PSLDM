// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The surface that holds the login pane.
//!
//! The pane never changes between the greeter and the locker. Only the
//! surface changes. The preview host is a normal window, so you can work on
//! the pane without a lock screen. Step 4 adds a layer-shell host for the
//! greeter and a session-lock host for the locker.

use gtk::prelude::*;

/// Which surface holds the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    /// A normal application window, for development.
    Preview,
}

impl HostKind {
    /// Build the windows and show them.
    ///
    /// The list has one window for each monitor. The preview host always
    /// returns one window.
    pub fn present(&self, app: &gtk::Application, pane: &gtk::Widget) -> Vec<gtk::Window> {
        match self {
            HostKind::Preview => {
                let window = gtk::ApplicationWindow::builder()
                    .application(app)
                    .title("PSLDM preview")
                    .default_width(1280)
                    .default_height(800)
                    .child(pane)
                    .build();
                window.present();
                vec![window.upcast()]
            }
        }
    }
}
