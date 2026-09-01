// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PSLDM screen locker.
//!
//! The lock surface arrives in milestone 4. This build has two test modes.
//!
//! - `psldm-lock --check [USER]` runs PAM on the terminal.
//! - `psldm-lock --preview [WALLPAPER]` shows the pane in a normal window.
//!
//! The preview mode uses the demo backend, so it asks for no real password.

use std::env;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use psldm_auth::{AuthEvent, demo, pam};
use psldm_session::LocalUser;
use psldm_ui::{AppSetup, HostKind, LoginState, Mode, UiConfig, UiAction, UserInfo};

/// The file name in `/etc/pam.d`.
const PAM_SERVICE: &str = "psldm";

/// The limit on failed attempts in check mode.
///
/// `pam_faillock` locks the account after 10 consecutive failures. The check
/// mode stops well before that limit.
const MAX_ATTEMPTS: usize = 3;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--check") => (),
        Some("--preview") => {
            preview(args.get(1).map(PathBuf::from));
            return;
        }
        _ => {
            eprintln!(
                "The lock surface is not built yet.\n\
                 Run `psldm-lock --check [USER]` or `psldm-lock --preview [WALLPAPER]`."
            );
            std::process::exit(2);
        }
    }

    let username = match args.get(1) {
        Some(name) => name.clone(),
        None => env::var("USER").unwrap_or_default(),
    };
    if username.is_empty() {
        eprintln!("Give a user name, because USER is not set.");
        std::process::exit(2);
    }

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

/// Show the pane in a normal window, with the demo backend.
fn preview(wallpaper: Option<PathBuf>) {
    let user = LocalUser::current();
    let username = user
        .as_ref()
        .map(|user| user.username.clone())
        .unwrap_or_else(|| "user".into());

    let setup = AppSetup {
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
        sessions: Vec::new(),
        host: HostKind::Preview,
    };

    psldm_ui::run(setup, demo::spawn());
}
