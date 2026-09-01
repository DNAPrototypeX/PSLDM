// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The event loop that joins the pane to a backend.
//!
//! The loop reads one [`AuthEvent`], gives it to the state machine, and draws
//! the result. The greeter and the locker run the same loop.
//!
//! The preview host closes the window after a correct password. The greeter
//! and the locker replace that step in milestone 4, when the real surfaces
//! exist.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;
use psldm_auth::AuthHandle;

use crate::host::HostKind;
use crate::pane::{LoginPane, PowerAction, UserInfo};
use crate::state::{LoginState, UiAction};
use crate::{Mode, UiConfig};

/// The quiet period before the screen shows the clock only again.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// How often the program looks at the quiet period.
const IDLE_CHECK_SECONDS: u32 = 5;

/// Everything that one run needs.
pub struct AppSetup {
    /// The application ID for GTK, such as `com.psldm.lock`.
    pub app_id: String,
    /// The greeter shows more of the interface than the locker.
    pub mode: Mode,
    /// The wallpaper, the blur, and the dark layer.
    pub config: UiConfig,
    /// The user to show.
    pub user: UserInfo,
    /// The names of the sessions. The locker leaves this empty.
    pub sessions: Vec<String>,
    /// The surface that holds the pane.
    pub host: HostKind,
}

/// Show the pane and run the GTK main loop.
pub fn run(setup: AppSetup, backend: AuthHandle) -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(setup.app_id.clone())
        .build();

    let setup = Rc::new(setup);
    let backend = Rc::new(RefCell::new(Some(backend)));

    app.connect_activate(move |app| {
        load_css();

        let Some(backend) = backend.borrow_mut().take() else {
            // GTK activates a second time when the user starts the program
            // again. One pane is enough.
            return;
        };

        let pane = Rc::new(LoginPane::new(setup.mode, &setup.config, &setup.user));
        let state = Rc::new(RefCell::new(LoginState::new(
            setup.mode,
            setup.user.username.clone(),
        )));

        if setup.mode == Mode::Greet {
            pane.set_users(std::slice::from_ref(&setup.user), |_| ());
            pane.set_sessions(&setup.sessions);
            for action in [PowerAction::Restart, PowerAction::Shutdown] {
                pane.add_power_button(action, |action| {
                    tracing::info!("The power button {action:?} is not wired yet");
                });
            }
        }

        let windows = setup.host.present(app, pane.widget());

        let (sender, mut receiver) = backend.split();

        let submit_state = Rc::clone(&state);
        let submit_sender = sender.clone();
        pane.connect_submit(move |answer| {
            if let Some(request) = submit_state.borrow_mut().submit(answer) {
                if let Err(err) = submit_sender.send(request) {
                    tracing::error!("Cannot reach the backend: {err}");
                }
            }
        });

        // The field keeps the focus while the screen shows the clock only,
        // so the first key of the password lands in it.
        pane.focus_entry();

        let last_input = Rc::new(Cell::new(Instant::now()));

        let wake_state = Rc::clone(&state);
        let wake_pane = Rc::clone(&pane);
        let wake_clock = Rc::clone(&last_input);
        pane.connect_activity(move || {
            wake_clock.set(Instant::now());
            let woke = wake_state.borrow_mut().wake();
            if woke {
                wake_pane.render(&wake_state.borrow());
            }
            // Swallow the key that wakes the screen. macOS does not put that
            // first key in the password.
            woke
        });

        let idle_state = Rc::clone(&state);
        let idle_pane = Rc::clone(&pane);
        let idle_clock = Rc::clone(&last_input);
        glib::timeout_add_seconds_local(IDLE_CHECK_SECONDS, move || {
            if idle_clock.get().elapsed() >= IDLE_TIMEOUT && idle_pane.is_empty() {
                let changed = idle_state.borrow_mut().sleep();
                if changed {
                    idle_pane.render(&idle_state.borrow());
                }
            }
            glib::ControlFlow::Continue
        });

        // Start the first attempt.
        let request = state.borrow_mut().start();
        if let Err(err) = sender.send(request) {
            tracing::error!("Cannot reach the backend: {err}");
        }
        pane.render(&state.borrow());

        let loop_pane = Rc::clone(&pane);
        let loop_state = Rc::clone(&state);
        let loop_sender = sender.clone();
        let app = app.clone();
        glib::spawn_future_local(async move {
            while let Some(event) = receiver.next_event().await {
                let action = loop_state.borrow_mut().on_event(event);
                loop_pane.render(&loop_state.borrow());

                match action {
                    UiAction::Restart => {
                        let request = loop_state.borrow_mut().start();
                        if let Err(err) = loop_sender.send(request) {
                            tracing::error!("Cannot reach the backend: {err}");
                        }
                    }
                    UiAction::Unlock | UiAction::StartSession | UiAction::Exit => {
                        for window in &windows {
                            window.close();
                        }
                        app.quit();
                        return;
                    }
                    UiAction::None => (),
                }
            }
        });
    });

    // GTK reads the command line by itself. PSLDM parses its own arguments, so
    // give GTK an empty list.
    app.run_with_args::<&str>(&[])
}

/// Load the stylesheet that both programs share.
fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(crate::STYLE);

    let Some(display) = gtk::gdk::Display::default() else {
        tracing::error!("No display. The stylesheet is not loaded.");
        return;
    };
    // The priority must beat the user stylesheet ~/.config/gtk-4.0/gtk.css.
    // A desktop theme there can change the shape of a button or an entry, and
    // the two screens must look the same on every system.
    #[allow(deprecated)]
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
    );
}
