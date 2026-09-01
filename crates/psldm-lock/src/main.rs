// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PSLDM screen locker.
//!
//! - `psldm-lock` locks the session. It needs a compositor with the
//!   `ext-session-lock-v1` protocol.
//! - `psldm-lock --preview [WALLPAPER]` shows the pane in a normal window and
//!   uses the demo backend, so it asks for no real password.
//! - `psldm-lock --preview-lock [WALLPAPER]` locks the session with the demo
//!   backend. Run it inside a nested compositor to test the lock surface.
//! - `psldm-lock --check [USER]` runs PAM on the terminal.

use std::env;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use psldm_auth::{AuthEvent, demo, pam};
use psldm_session::LocalUser;
use psldm_ui::{AppSetup, HostKind, LoginState, Mode, UiAction, UiConfig, UserInfo};

/// The file name in `/etc/pam.d`.
const PAM_SERVICE: &str = "psldm";

/// The limit on failed attempts in check mode.
///
/// `pam_faillock` locks the account after 10 consecutive failures. The check
/// mode stops well before that limit.
const MAX_ATTEMPTS: usize = 3;

fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None => lock(default_wallpaper()),
        Some("--wallpaper") => lock(args.get(1).map(PathBuf::from)),
        Some("--preview") => preview(
            args.get(1).map(PathBuf::from).or_else(default_wallpaper),
            HostKind::Preview,
        ),
        Some("--preview-lock") => preview(
            args.get(1).map(PathBuf::from).or_else(default_wallpaper),
            HostKind::SessionLock,
        ),
        Some("--check") => {
            check(args.get(1).cloned());
            return;
        }
        _ => {
            eprintln!(
                "Usage:\n  \
                 psldm-lock [--wallpaper PATH]\n  \
                 psldm-lock --preview [WALLPAPER]\n  \
                 psldm-lock --preview-lock [WALLPAPER]\n  \
                 psldm-lock --check [USER]"
            );
            std::process::exit(2);
        }
    };

    std::process::exit(if code == gtk::glib::ExitCode::SUCCESS {
        0
    } else {
        1
    });
}

/// Lock the session.
fn lock(wallpaper: Option<PathBuf>) -> gtk::glib::ExitCode {
    let setup = setup(wallpaper, HostKind::SessionLock);
    psldm_ui::run(setup, pam::spawn(PAM_SERVICE))
}

/// Show the pane with the demo backend, on the given surface.
fn preview(wallpaper: Option<PathBuf>, host: HostKind) -> gtk::glib::ExitCode {
    let setup = setup(wallpaper, host);
    psldm_ui::run(setup, demo::spawn())
}

/// Build the settings for one run.
fn setup(wallpaper: Option<PathBuf>, host: HostKind) -> AppSetup {
    let user = LocalUser::current();
    let username = user
        .as_ref()
        .map(|user| user.username.clone())
        .unwrap_or_else(|| "user".into());

    AppSetup {
        app_id: "com.psldm.lock".into(),
        mode: Mode::Lock,
        config: UiConfig {
            wallpaper,
            ..UiConfig::default()
        },
        user: UserInfo {
            display_name: user
                .as_ref()
                .map(|user| user.full_name.clone())
                .unwrap_or_else(|| username.clone()),
            avatar: user.and_then(|user| user.avatar),
            username,
        },
        users: Vec::new(),
        sessions: Vec::new(),
        environment: Vec::new(),
        reboot: Vec::new(),
        poweroff: Vec::new(),
        host,
    }
}

/// The wallpaper of the desktop, if PSLDM can find one.
///
/// The order is `PSLDM_WALLPAPER`, then the PSLDM link, then the Omarchy link.
fn default_wallpaper() -> Option<PathBuf> {
    if let Some(path) = env::var_os("PSLDM_WALLPAPER") {
        return Some(PathBuf::from(path));
    }

    let home = PathBuf::from(env::var_os("HOME")?);
    [
        home.join(".config/psldm/wallpaper"),
        home.join(".config/omarchy/current/background"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

/// Run PAM on the terminal, without a window.
fn check(username: Option<String>) {
    let username = username.unwrap_or_else(|| env::var("USER").unwrap_or_default());
    if username.is_empty() {
        eprintln!("Give a user name, because USER is not set.");
        std::process::exit(2);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("cannot start the Tokio runtime");

    runtime.block_on(async move {
        let mut handle = pam::spawn(PAM_SERVICE);
        let mut state = LoginState::new(Mode::Lock, username);
        let mut attempts = 0usize;
        let request = state.start();
        handle.send(request).expect("the PAM backend stopped");

        while let Some(event) = handle.next_event().await {
            let prompt = match &event {
                AuthEvent::Prompt { text, secret } => Some((text.clone(), *secret)),
                _ => None,
            };

            match state.on_event(event) {
                UiAction::Unlock => {
                    println!("Authenticated. The locker would unlock now.");
                    return;
                }
                UiAction::Restart => {
                    if let Some(message) = &state.message {
                        println!("Failed: {}", message.text);
                    }
                    attempts += 1;
                    if attempts >= MAX_ATTEMPTS {
                        eprintln!("Stopped after {MAX_ATTEMPTS} failed attempts.");
                        std::process::exit(1);
                    }
                    let request = state.start();
                    handle.send(request).expect("the PAM backend stopped");
                }
                _ => (),
            }

            if state.closed {
                eprintln!("The backend stopped.");
                std::process::exit(1);
            }

            if let Some((text, secret)) = prompt {
                let Some(answer) = read_answer(&text, secret) else {
                    eprintln!("No more input.");
                    std::process::exit(1);
                };
                if let Some(request) = state.submit(answer) {
                    handle.send(request).expect("the PAM backend stopped");
                }
            }
        }
    });
}

/// Read one answer from the terminal, or from a pipe.
///
/// Returns `None` at the end of the input.
fn read_answer(text: &str, secret: bool) -> Option<String> {
    let stdin = std::io::stdin();
    if secret && stdin.is_terminal() {
        return rpassword::prompt_password(format!("{text} ")).ok();
    }

    if stdin.is_terminal() {
        print!("{text} ");
        std::io::stdout().flush().ok();
    }

    let mut line = String::new();
    match stdin.read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
    }
}
