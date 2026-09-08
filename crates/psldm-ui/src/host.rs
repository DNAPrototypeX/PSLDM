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
//!
//! The set of monitors changes while the program runs. A user plugs a screen
//! in, or a laptop lid closes and the panel goes away. [`Surfaces`] watches
//! the monitor list, builds a pane for every monitor that arrives, and drops
//! the pane of every monitor that leaves. The count can fall to zero, and the
//! program stays on the screen that comes back.

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
    #[error("no ext-session-lock-v1 protocol. PSLDM needs Hyprland 0.56 or later")]
    LockUnsupported,
    #[error("Hyprland refused the lock")]
    LockRefused,
}

/// Builds the pane that one monitor shows.
pub type MakePane = dyn Fn() -> LoginPane;

/// Prepares a new pane before it appears.
///
/// The application gives this to [`Surfaces::on_new_pane`]. Every pane runs
/// it one time, both the panes of the first monitors and the panes of the
/// monitors that arrive later.
pub type PaneSetup = dyn Fn(&Rc<LoginPane>);

/// The pane on one monitor, and the window that holds it.
struct Screen {
    /// The monitor that shows the pane. The preview window follows no
    /// monitor, so it holds `None`.
    monitor: Option<gtk::gdk::Monitor>,
    pane: Rc<LoginPane>,
    window: gtk::Window,
}

/// One pane for each monitor, and the surfaces that hold them.
pub struct Surfaces {
    kind: HostKind,
    app: gtk::Application,
    make_pane: Box<MakePane>,
    screens: RefCell<Vec<Screen>>,
    lock: Option<SessionLock>,
    /// Keeps the program alive while no window belongs to the application.
    /// A laptop lid that closes takes the last window away, so both the
    /// greeter and the locker need this. Every path that ends the program
    /// must drop it, or the program stays in memory for ever.
    hold: Rc<RefCell<Option<ApplicationHoldGuard>>>,
    /// The compositor ended the lock, or it never started it.
    failed: Rc<Cell<bool>>,
    /// What every new pane needs. [`Surfaces::on_new_pane`] sets it.
    setup: RefCell<Option<Rc<PaneSetup>>>,
}

impl Surfaces {
    /// Draw the same state on every monitor.
    pub fn render(&self, state: &LoginState) {
        for screen in self.screens.borrow().iter() {
            screen.pane.render(state);
        }
    }

    /// Report whether every field is empty.
    pub fn fields_are_empty(&self) -> bool {
        self.screens
            .borrow()
            .iter()
            .all(|screen| screen.pane.is_empty())
    }

    /// Give the keyboard to the field on the first monitor.
    pub fn focus_entry(&self) {
        if let Some(screen) = self.screens.borrow().first() {
            screen.pane.focus_entry();
        }
    }

    /// The pane on each monitor.
    pub fn panes(&self) -> Vec<Rc<LoginPane>> {
        self.screens
            .borrow()
            .iter()
            .map(|screen| Rc::clone(&screen.pane))
            .collect()
    }

    /// Prepare every pane with `setup`.
    ///
    /// The panes that already exist run `setup` at once. Every pane of a
    /// monitor that arrives later runs it as well, so a screen that the user
    /// plugs in behaves like the first screen.
    pub fn on_new_pane(&self, setup: impl Fn(&Rc<LoginPane>) + 'static) {
        let setup: Rc<PaneSetup> = Rc::new(setup);
        for pane in self.panes() {
            setup(&pane);
        }
        *self.setup.borrow_mut() = Some(setup);
    }

    /// Report whether the surfaces still belong to a window.
    ///
    /// The session-lock library destroys its windows when the lock ends. No
    /// monitor at all is not the end of the lock, so an empty set counts as
    /// on screen.
    pub fn are_on_screen(&self) -> bool {
        let screens = self.screens.borrow();
        screens.is_empty() || screens.iter().any(|screen| screen.pane.is_on_screen())
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
                // Drop the hold first. It outlives the last window, and the
                // program would stay in memory without this.
                self.release();
                for screen in self.screens.borrow().iter() {
                    screen.window.close();
                }
            }
        }
    }

    /// Watch the monitor list of the display.
    ///
    /// The handler holds a weak reference, so the surfaces still end when
    /// nothing else points at them.
    fn watch_monitors(self: &Rc<Self>) {
        // The preview host is one ordinary window on the monitor that the
        // compositor picks. It follows no monitor list.
        if self.kind == HostKind::Preview {
            return;
        }

        let Some(display) = gtk::gdk::Display::default() else {
            return;
        };

        let weak = Rc::downgrade(self);
        display
            .monitors()
            .connect_items_changed(move |list, _, _, _| {
                let Some(surfaces) = weak.upgrade() else {
                    return;
                };
                surfaces.drop_gone_monitors();

                // The session-lock library reports a new monitor with its own
                // `monitor` signal, because a lock surface may exist only while
                // the lock is held. The layer-shell host reads the list here.
                if surfaces.kind == HostKind::LayerShell {
                    for monitor in monitors_of(list) {
                        surfaces.add_monitor(&monitor);
                    }
                }
            });
    }

    /// Build a pane on `monitor` and show it.
    ///
    /// A monitor that already has a pane changes nothing.
    fn add_monitor(&self, monitor: &gtk::gdk::Monitor) {
        if self
            .screens
            .borrow()
            .iter()
            .any(|screen| screen.monitor.as_ref() == Some(monitor))
        {
            return;
        }

        // Find the field that holds the keyboard now. The new pane must not
        // take the password from the screen that the user types on.
        let typing = self
            .screens
            .borrow()
            .iter()
            .find(|screen| screen.pane.entry_has_focus())
            .map(|screen| Rc::clone(&screen.pane));

        let pane = Rc::new((self.make_pane)());
        let window = match self.kind {
            // The preview window follows no monitor list, so nothing reaches
            // this arm.
            HostKind::Preview => return,
            HostKind::LayerShell => layer_window(&self.app, &pane, monitor),
            HostKind::SessionLock => {
                let Some(lock) = &self.lock else {
                    return;
                };
                // The window must be new and unrealized here. The library
                // maps it, and it destroys the window when the monitor goes
                // away or the lock ends.
                let window = gtk::Window::new();
                window.set_child(Some(pane.widget()));
                lock.assign_window_to_monitor(&window, monitor);
                window
            }
        };

        self.screens.borrow_mut().push(Screen {
            monitor: Some(monitor.clone()),
            pane: Rc::clone(&pane),
            window: window.clone(),
        });

        // Take the callback out of the cell first. The setup reads the pane
        // list, and a borrow that is still open would panic.
        let setup = self.setup.borrow().clone();
        if let Some(setup) = setup {
            setup(&pane);
        }

        window.present();

        // The setup draws the state, and that gives the keyboard to the new
        // field. Give it back to the field that the user was typing in.
        typing.unwrap_or(pane).focus_entry();

        tracing::info!("Showing the pane on {}", monitor_name(monitor));
    }

    /// Drop the pane of every monitor that the display no longer lists.
    fn drop_gone_monitors(&self) {
        let live = monitors();
        let mut closed = Vec::new();

        {
            let mut screens = self.screens.borrow_mut();
            screens.retain(|screen| {
                let Some(monitor) = &screen.monitor else {
                    return true;
                };
                let keep = monitor.is_valid() && live.contains(monitor);
                if !keep {
                    tracing::info!("Removing the pane on {}", monitor_name(monitor));
                    closed.push(screen.window.clone());
                }
                keep
            });
        }

        if closed.is_empty() {
            return;
        }

        for window in closed {
            // The session-lock library unmaps and destroys its own window
            // when the monitor goes away. Closing it here as well can break
            // the Wayland connection, so only the other hosts close it.
            if self.lock.is_none() {
                window.destroy();
            }
        }

        // The closed screen may have held the keyboard. Give it to a screen
        // that is still there, or the password field takes no keys.
        let screens = self.screens.borrow();
        if screens.iter().any(|screen| screen.pane.entry_has_focus()) {
            return;
        }
        if let Some(screen) = screens.first() {
            screen.pane.focus_entry();
        }
    }
}

impl HostKind {
    /// Build one pane for each monitor and show it.
    ///
    /// The result watches the monitor list from this point on.
    pub fn present(
        &self,
        app: &gtk::Application,
        make_pane: Box<MakePane>,
    ) -> Result<Rc<Surfaces>, HostError> {
        let surfaces = match self {
            HostKind::Preview => preview(app, make_pane),
            HostKind::LayerShell => layer_shell(app, make_pane)?,
            HostKind::SessionLock => session_lock(app, make_pane)?,
        };
        surfaces.watch_monitors();
        Ok(surfaces)
    }
}

/// A set of surfaces with no monitor yet.
fn empty(kind: HostKind, app: &gtk::Application, make_pane: Box<MakePane>) -> Surfaces {
    Surfaces {
        kind,
        app: app.clone(),
        make_pane,
        screens: RefCell::new(Vec::new()),
        lock: None,
        hold: Rc::new(RefCell::new(None)),
        failed: Rc::new(Cell::new(false)),
        setup: RefCell::new(None),
    }
}

/// One normal window, for development.
fn preview(app: &gtk::Application, make_pane: Box<MakePane>) -> Rc<Surfaces> {
    let surfaces = empty(HostKind::Preview, app, make_pane);

    let pane = Rc::new((surfaces.make_pane)());
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("PSLDM preview")
        .default_width(1280)
        .default_height(800)
        .child(pane.widget())
        .build();
    window.present();

    // The preview window follows no monitor. The compositor places it, and
    // the monitor list never removes it.
    surfaces.screens.borrow_mut().push(Screen {
        monitor: None,
        pane,
        window: window.upcast(),
    });

    Rc::new(surfaces)
}

/// One overlay on each monitor.
fn layer_shell(
    app: &gtk::Application,
    make_pane: Box<MakePane>,
) -> Result<Rc<Surfaces>, HostError> {
    if gtk::gdk::Display::default().is_none() {
        return Err(HostError::NoDisplay);
    }

    let mut surfaces = empty(HostKind::LayerShell, app, make_pane);
    // The greeter must survive a moment with no monitor, such as a laptop
    // lid that closes. Without the hold the last window that closes ends the
    // program, and greetd would start the greeter again.
    surfaces.hold = Rc::new(RefCell::new(Some(app.hold())));

    let surfaces = Rc::new(surfaces);
    for monitor in monitors() {
        surfaces.add_monitor(&monitor);
    }
    warn_on_empty();

    Ok(surfaces)
}

/// One layer-shell window for one monitor.
fn layer_window(
    app: &gtk::Application,
    pane: &Rc<LoginPane>,
    monitor: &gtk::gdk::Monitor,
) -> gtk::Window {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .child(pane.widget())
        .build();

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some(LAYER_NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    // A negative zone keeps the overlay over every panel.
    window.set_exclusive_zone(-1);

    window.upcast()
}

/// One lock surface on each monitor.
fn session_lock(
    app: &gtk::Application,
    make_pane: Box<MakePane>,
) -> Result<Rc<Surfaces>, HostError> {
    if gtk::gdk::Display::default().is_none() {
        return Err(HostError::NoDisplay);
    }
    if !gtk4_session_lock::is_supported() {
        return Err(HostError::LockUnsupported);
    }

    let lock = SessionLock::new();
    let mut surfaces = empty(HostKind::SessionLock, app, make_pane);
    // The hold keeps the program alive while it owns no application window.
    // A lock surface never belongs to the application, and a lid that closes
    // can take every monitor away. Every path that ends the lock must drop
    // it, or the program stays in memory for ever and blocks the next lock.
    surfaces.hold = Rc::new(RefCell::new(Some(app.hold())));

    let failed = Rc::clone(&surfaces.failed);
    let hold = Rc::clone(&surfaces.hold);

    let failed_app = app.clone();
    let failed_hold = Rc::clone(&hold);
    lock.connect_failed(move |_| {
        tracing::error!("Hyprland refused the lock");
        failed.set(true);
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

    surfaces.lock = Some(lock);
    let surfaces = Rc::new(surfaces);

    // The library reports every monitor that exists when the lock starts,
    // and every monitor that arrives while the lock holds. It is the only
    // safe moment to build a lock surface, so the panes come from here and
    // not from the monitor list.
    let weak = Rc::downgrade(&surfaces);
    let lock = surfaces.lock.as_ref().expect("the lock was just set");
    lock.connect_monitor(move |_, monitor| {
        if let Some(surfaces) = weak.upgrade() {
            surfaces.add_monitor(monitor);
        }
    });

    if !lock.lock() {
        surfaces.release();
        return Err(HostError::LockRefused);
    }

    warn_on_empty();
    Ok(surfaces)
}

/// Say that no screen shows the pane yet.
///
/// This is not an error. A laptop with a closed lid and no second screen has
/// no monitor at all, and the pane appears as soon as one arrives.
fn warn_on_empty() {
    if monitors().is_empty() {
        tracing::warn!("No monitor is connected. PSLDM waits for one.");
    }
}

/// Every monitor of the default display.
fn monitors() -> Vec<gtk::gdk::Monitor> {
    let Some(display) = gtk::gdk::Display::default() else {
        return Vec::new();
    };
    monitors_of(&display.monitors())
}

/// Every monitor in one list model.
fn monitors_of(list: &gtk::gio::ListModel) -> Vec<gtk::gdk::Monitor> {
    (0..list.n_items())
        .filter_map(|index| list.item(index))
        .filter_map(|object| object.downcast::<gtk::gdk::Monitor>().ok())
        .collect()
}

/// The connector name of a monitor, such as `DP-7`.
///
/// A monitor that arrives can reach the program before its name does, so the
/// model and then a fixed text stand in for the name.
fn monitor_name(monitor: &gtk::gdk::Monitor) -> String {
    monitor
        .connector()
        .or_else(|| monitor.model())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "an unnamed monitor".into())
}
