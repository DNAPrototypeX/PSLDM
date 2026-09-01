// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The surfaces that hold the login pane.
//!
//! The pane is the same in all three hosts. Only the surface changes.
//!
//! - `Preview` is a normal window, for development.
//! - `LayerShell` is an overlay that covers every monitor. The greeter uses
//!   it inside the compositor that greetd starts.
//! - `SessionLock` uses the `ext-session-lock-v1` protocol. The compositor
//!   keeps the screen locked even if the locker stops.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use gtk4_session_lock::Instance as SessionLock;

use crate::pane::LoginPane;
use crate::state::LoginState;

/// The name that a compositor shows for the greeter surface.
const LAYER_NAMESPACE: &str = "psldm";

/// Which surface holds the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    /// A normal application window, for development.
    Preview,
    /// A layer-shell overlay on every monitor.
    LayerShell,
    /// A lock surface on every monitor.
    SessionLock,
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("no display. Is a Wayland compositor running?")]
    NoDisplay,
    #[error("no monitor is connected")]
    NoMonitors,
    #[error("this compositor does not support the ext-session-lock-v1 protocol")]
    LockUnsupported,
    #[error("the compositor refused the lock")]
    LockRefused,
}

/// One pane for each monitor, and the surfaces that hold them.
pub struct Surfaces {
    panes: Vec<Rc<LoginPane>>,
    windows: Vec<gtk::Window>,
    lock: Option<SessionLock>,
    /// The compositor ended the lock, or it never started it.
    failed: Rc<Cell<bool>>,
}

impl Surfaces {
    /// Draw the same state on every monitor.
    pub fn render(&self, state: &LoginState) {
        for pane in &self.panes {
            pane.render(state);
        }
    }

    /// Report whether every field is empty.
    pub fn fields_are_empty(&self) -> bool {
        self.panes.iter().all(|pane| pane.is_empty())
    }

    /// Give the keyboard to the field on the first monitor.
    pub fn focus_entry(&self) {
        if let Some(pane) = self.panes.first() {
            pane.focus_entry();
        }
    }

    /// The pane on each monitor.
    pub fn panes(&self) -> &[Rc<LoginPane>] {
        &self.panes
    }

    /// Report whether the lock failed.
    pub fn failed(&self) -> bool {
        self.failed.get()
    }

    /// Remove the surfaces.
    ///
    /// The lock host asks the compositor to unlock. The compositor then
    /// destroys the windows.
    pub fn dismiss(&self) {
        match &self.lock {
            Some(lock) => lock.unlock(),
            None => {
                for window in &self.windows {
                    window.close();
                }
            }
        }
    }
}

impl HostKind {
    /// Build one pane for each monitor and show it.
    pub fn present(
        &self,
        app: &gtk::Application,
        make_pane: &dyn Fn() -> LoginPane,
    ) -> Result<Surfaces, HostError> {
        match self {
            HostKind::Preview => Ok(preview(app, make_pane)),
            HostKind::LayerShell => layer_shell(app, make_pane),
            HostKind::SessionLock => session_lock(app, make_pane),
        }
    }
}

/// One normal window, for development.
fn preview(app: &gtk::Application, make_pane: &dyn Fn() -> LoginPane) -> Surfaces {
    let pane = Rc::new(make_pane());
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("PSLDM preview")
        .default_width(1280)
        .default_height(800)
        .child(pane.widget())
        .build();
    window.present();

    Surfaces {
        panes: vec![pane],
        windows: vec![window.upcast()],
        lock: None,
        failed: Rc::new(Cell::new(false)),
    }
}

/// One overlay on each monitor.
fn layer_shell(
    app: &gtk::Application,
    make_pane: &dyn Fn() -> LoginPane,
) -> Result<Surfaces, HostError> {
    let mut panes = Vec::new();
    let mut windows = Vec::new();

    for monitor in monitors()? {
        let pane = Rc::new(make_pane());
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .child(pane.widget())
            .build();

        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace(Some(LAYER_NAMESPACE));
        window.set_monitor(Some(&monitor));
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        // A negative zone keeps the overlay over every panel.
        window.set_exclusive_zone(-1);
        window.present();

        panes.push(pane);
        windows.push(window.upcast());
    }

    Ok(Surfaces {
        panes,
        windows,
        lock: None,
        failed: Rc::new(Cell::new(false)),
    })
}

/// One lock surface on each monitor.
fn session_lock(
    app: &gtk::Application,
    make_pane: &dyn Fn() -> LoginPane,
) -> Result<Surfaces, HostError> {
    if !gtk4_session_lock::is_supported() {
        return Err(HostError::LockUnsupported);
    }

    let monitors = monitors()?;
    let lock = SessionLock::new();
    let failed = Rc::new(Cell::new(false));

    let failed_flag = Rc::clone(&failed);
    let failed_app = app.clone();
    lock.connect_failed(move |_| {
        tracing::error!("The compositor refused the lock");
        failed_flag.set(true);
        failed_app.quit();
    });

    lock.connect_locked(|_| tracing::info!("The session is locked"));

    let unlocked_app = app.clone();
    lock.connect_unlocked(move |_| {
        tracing::info!("The session is unlocked");
        unlocked_app.quit();
    });

    if !lock.lock() {
        return Err(HostError::LockRefused);
    }

    let mut panes = Vec::new();
    let mut windows = Vec::new();

    // Each window must be new and unrealized here. The library maps it, and
    // it destroys the window when the lock ends.
    for monitor in monitors {
        let pane = Rc::new(make_pane());
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .child(pane.widget())
            .build();

        lock.assign_window_to_monitor(&window, &monitor);
        window.present();

        panes.push(pane);
        windows.push(window.upcast());
    }

    Ok(Surfaces {
        panes,
        windows,
        lock: Some(lock),
        failed,
    })
}

/// Every monitor of the default display.
fn monitors() -> Result<Vec<gtk::gdk::Monitor>, HostError> {
    let display = gtk::gdk::Display::default().ok_or(HostError::NoDisplay)?;
    let list = display.monitors();

    let monitors: Vec<gtk::gdk::Monitor> = (0..list.n_items())
        .filter_map(|index| list.item(index))
        .filter_map(|object| object.downcast::<gtk::gdk::Monitor>().ok())
        .collect();

    if monitors.is_empty() {
        return Err(HostError::NoMonitors);
    }
    Ok(monitors)
}
