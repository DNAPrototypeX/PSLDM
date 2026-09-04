// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The surfaces that hold the login pane.
//!
//! The pane is the same in all three hosts. Only the surface changes.
//!
//! - `Preview` is a normal window, for development.
//! - `LayerShell` is an overlay that covers every monitor. The greeter uses
//!   it inside the Hyprland instance that greetd starts.
//! - `SessionLock` uses the `ext-session-lock-v1` protocol. Hyprland keeps
//!   the screen locked even if the locker stops.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::gio::ApplicationHoldGuard;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use gtk4_session_lock::Instance as SessionLock;

use crate::pane::LoginPane;
use crate::state::LoginState;

/// The name that Hyprland shows for the greeter surface.
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
    #[error("no display. Is Hyprland running?")]
    NoDisplay,
    #[error("no monitor is connected")]
    NoMonitors,
    #[error("no ext-session-lock-v1 protocol. PSLDM needs Hyprland 0.56 or later")]
    LockUnsupported,
    #[error("Hyprland refused the lock")]
    LockRefused,
}

/// One pane for each monitor, and the surfaces that hold them.
pub struct Surfaces {
    panes: Vec<Rc<LoginPane>>,
    windows: Vec<gtk::Window>,
    lock: Option<SessionLock>,
    /// Keeps the program alive while no window belongs to the application.
    /// The lock drops it when the lock ends, so the program cannot outlive
    /// the lock.
    hold: Rc<RefCell<Option<ApplicationHoldGuard>>>,
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

    /// Report whether the surfaces still belong to a window.
    ///
    /// The session-lock library destroys its windows when the lock ends.
    pub fn are_on_screen(&self) -> bool {
        self.panes.iter().any(|pane| pane.is_on_screen())
    }

    /// Report whether the lock failed.
    pub fn failed(&self) -> bool {
        self.failed.get()
    }

    /// Stop keeping the program alive.
    pub fn release(&self) {
        self.hold.borrow_mut().take();
    }

    /// Remove the surfaces.
    ///
    /// The lock host asks Hyprland to unlock. Hyprland then destroys the
    /// windows.
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
        hold: Rc::new(RefCell::new(None)),
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
        hold: Rc::new(RefCell::new(None)),
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

    // The hold keeps the program alive while it owns no application window.
    // Every path that ends the lock must drop it, or the program stays in
    // memory for ever and blocks the next lock.
    let hold: Rc<RefCell<Option<ApplicationHoldGuard>>> = Rc::new(RefCell::new(Some(app.hold())));

    let failed_flag = Rc::clone(&failed);
    let failed_app = app.clone();
    let failed_hold = Rc::clone(&hold);
    lock.connect_failed(move |_| {
        tracing::error!("Hyprland refused the lock");
        failed_flag.set(true);
        failed_hold.borrow_mut().take();
        failed_app.quit();
    });

    lock.connect_locked(|_| tracing::info!("The session is locked"));

    let unlocked_app = app.clone();
    let unlocked_hold = Rc::clone(&hold);
    lock.connect_unlocked(move |_| {
        tracing::info!("The session is unlocked");
        unlocked_hold.borrow_mut().take();
        // The library destroys the windows while this signal runs. Leave the
        // main loop on the next turn, so that the work finishes first.
        let app = unlocked_app.clone();
        gtk::glib::idle_add_local_once(move || app.quit());
    });

    if !lock.lock() {
        return Err(HostError::LockRefused);
    }

    let mut panes = Vec::new();
    let mut windows = Vec::new();

    // A lock surface must not belong to the application. The library calls
    // gtk_window_destroy on every window when the lock ends, and the
    // application handles that signal for its own windows. That handler
    // crashed the program.

    // Each window must be new and unrealized here. The library maps it, and
    // it destroys the window when the lock ends.
    for monitor in monitors {
        let pane = Rc::new(make_pane());
        let window = gtk::Window::new();
        window.set_child(Some(pane.widget()));

        lock.assign_window_to_monitor(&window, &monitor);
        window.present();

        panes.push(pane);
        windows.push(window);
    }

    Ok(Surfaces {
        panes,
        windows,
        lock: Some(lock),
        hold,
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
