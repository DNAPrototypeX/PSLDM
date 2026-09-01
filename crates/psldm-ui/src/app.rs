// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The event loop that joins the pane to a backend.
//!
//! The loop reads one [`AuthEvent`], gives it to the state machine, and draws
//! the result on every monitor. The greeter and the locker run the same loop.
//! They differ in the surface and in the backend.

use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;
use psldm_auth::{AuthHandle, AuthRequest};

use crate::host::{HostKind, Surfaces};
use crate::pane::{LoginPane, PowerAction, UserInfo};
use crate::state::{LoginState, UiAction};
use crate::{Mode, UiConfig};

/// The quiet period before the screen shows the clock only again.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// How often the program looks at the quiet period.
const IDLE_CHECK_SECONDS: u32 = 5;

/// One session that the greeter can start.
#[derive(Debug, Clone)]
pub struct SessionChoice {
    /// The name in the list, such as `Hyprland`.
    pub name: String,
    /// The command that starts the session.
    pub command: Vec<String>,
}

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
    /// Every user that the greeter offers. The locker leaves this empty.
    pub users: Vec<UserInfo>,
    /// Every session that the greeter offers. The locker leaves this empty.
    pub sessions: Vec<SessionChoice>,
    /// Extra variables for the new session, as `KEY=VALUE`.
    pub environment: Vec<String>,
    /// The command that restarts the computer.
    pub reboot: Vec<String>,
    /// The command that stops the computer.
    pub poweroff: Vec<String>,
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
    let failed = Rc::new(Cell::new(false));

    let activate_failed = Rc::clone(&failed);
    app.connect_activate(move |app| {
        load_css(setup.config.font.as_deref());

        let Some(backend) = backend.borrow_mut().take() else {
            // GTK activates a second time when the program starts again. One
            // set of surfaces is enough.
            return;
        };

        let make_pane = || LoginPane::new(setup.mode, &setup.config, &setup.user);
        let surfaces = match setup.host.present(app, &make_pane) {
            Ok(surfaces) => Rc::new(surfaces),
            Err(err) => {
                tracing::error!("Cannot show the pane: {err}");
                activate_failed.set(true);
                app.quit();
                return;
            }
        };

        let state = Rc::new(RefCell::new(LoginState::new(
            setup.mode,
            setup.user.username.clone(),
        )));

        if setup.mode == Mode::Greet {
            fill_greeter(&setup, &surfaces, &state);
        }

        let (sender, mut receiver) = backend.split();

        for pane in surfaces.panes() {
            let submit_state = Rc::clone(&state);
            let submit_sender = sender.clone();
            let submit_surfaces = Rc::clone(&surfaces);
            pane.connect_submit(move |answer| {
                let request = submit_state.borrow_mut().submit(answer);
                if let Some(request) = request {
                    send(&submit_sender, request);
                }
                submit_surfaces.render(&submit_state.borrow());
            });
        }

        // The field keeps the focus while the screen shows the clock only, so
        // the first key of the password lands in it.
        surfaces.focus_entry();

        let last_input = Rc::new(Cell::new(Instant::now()));

        for pane in surfaces.panes() {
            let wake_state = Rc::clone(&state);
            let wake_surfaces = Rc::clone(&surfaces);
            let wake_clock = Rc::clone(&last_input);
            pane.connect_activity(move || {
                wake_clock.set(Instant::now());
                let woke = wake_state.borrow_mut().wake();
                if woke {
                    wake_surfaces.render(&wake_state.borrow());
                }
                // Swallow the key that wakes the screen. macOS does not put
                // that first key in the password.
                woke
            });
        }

        let idle_state = Rc::clone(&state);
        let idle_surfaces = Rc::clone(&surfaces);
        let idle_clock = Rc::clone(&last_input);
        glib::timeout_add_seconds_local(IDLE_CHECK_SECONDS, move || {
            if idle_clock.get().elapsed() >= IDLE_TIMEOUT && idle_surfaces.fields_are_empty() {
                let changed = idle_state.borrow_mut().sleep();
                if changed {
                    idle_surfaces.render(&idle_state.borrow());
                }
            }
            glib::ControlFlow::Continue
        });

        // Start the first attempt.
        let request = state.borrow_mut().start();
        send(&sender, request);
        surfaces.render(&state.borrow());

        let loop_setup = Rc::clone(&setup);
        let loop_surfaces = Rc::clone(&surfaces);
        let loop_state = Rc::clone(&state);
        let loop_sender = sender.clone();
        let app = app.clone();
        glib::spawn_future_local(async move {
            while let Some(event) = receiver.next_event().await {
                let action = loop_state.borrow_mut().on_event(event);
                loop_surfaces.render(&loop_state.borrow());

                match action {
                    UiAction::Restart => {
                        let request = loop_state.borrow_mut().start();
                        send(&loop_sender, request);
                    }
                    UiAction::Unlock => {
                        loop_surfaces.dismiss();
                        return;
                    }
                    UiAction::StartSession => {
                        match chosen_session(&loop_setup, &loop_surfaces) {
                            Some(command) => send(
                                &loop_sender,
                                AuthRequest::StartSession {
                                    command,
                                    environment: loop_setup.environment.clone(),
                                },
                            ),
                            None => tracing::error!("No session is selected"),
                        }
                    }
                    UiAction::Exit => {
                        // greetd starts the session when the greeter stops.
                        loop_surfaces.dismiss();
                        app.quit();
                        return;
                    }
                    UiAction::None => (),
                }
            }
        });
    });

    // GTK reads the command line by itself. PSLDM parses its own arguments,
    // so give GTK an empty list.
    let code = app.run_with_args::<&str>(&[]);
    if failed.get() {
        return glib::ExitCode::FAILURE;
    }
    code
}

/// Add the parts that only the greeter shows.
fn fill_greeter(setup: &Rc<AppSetup>, surfaces: &Rc<Surfaces>, state: &Rc<RefCell<LoginState>>) {
    let names: Vec<String> = setup
        .sessions
        .iter()
        .map(|session| session.name.clone())
        .collect();

    for pane in surfaces.panes() {
        pane.set_sessions(&names);

        let select_state = Rc::clone(state);
        let select_surfaces = Rc::clone(surfaces);
        pane.set_users(&setup.users, move |user| {
            select_state.borrow_mut().username = user.username.clone();
            for pane in select_surfaces.panes() {
                pane.set_user(user);
            }
        });

        for action in [PowerAction::Restart, PowerAction::Shutdown] {
            let setup = Rc::clone(setup);
            pane.add_power_button(action, move |action| {
                let command = match action {
                    PowerAction::Restart => &setup.reboot,
                    PowerAction::Shutdown => &setup.poweroff,
                };
                run_command(command);
            });
        }
    }
}

/// The command of the session that the user selected.
fn chosen_session(setup: &Rc<AppSetup>, surfaces: &Rc<Surfaces>) -> Option<Vec<String>> {
    let selected = surfaces
        .panes()
        .iter()
        .find_map(|pane| pane.selected_session());

    match selected {
        Some(name) => setup
            .sessions
            .iter()
            .find(|session| session.name == name)
            .map(|session| session.command.clone()),
        None => setup
            .sessions
            .first()
            .map(|session| session.command.clone()),
    }
}

/// Start a power command and leave it to run.
fn run_command(command: &[String]) {
    let Some((program, arguments)) = command.split_first() else {
        tracing::error!("The power command is empty");
        return;
    };

    match Command::new(program).args(arguments).spawn() {
        Ok(_) => tracing::info!("Started {command:?}"),
        Err(err) => tracing::error!("Cannot start {command:?}: {err}"),
    }
}

fn send(sender: &psldm_auth::AuthSender, request: AuthRequest) {
    if let Err(err) = sender.send(request) {
        tracing::error!("Cannot reach the backend: {err}");
    }
}

/// Load the stylesheet that both programs share.
///
/// `font` names the family for every part of the pane. The rule comes after
/// the stylesheet, so it replaces the family there.
fn load_css(font: Option<&str>) {
    let mut style = crate::STYLE.to_string();
    if let Some(font) = font.map(str::trim).filter(|font| !font.is_empty()) {
        // A family name with a space needs quotation marks in CSS.
        style.push_str(&format!(
            "\nwindow, .psldm-root, popover {{ font-family: \"{}\"; }}\n",
            font.replace('"', "")
        ));
    }

    let provider = gtk::CssProvider::new();
    provider.load_from_string(&style);

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
