// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PSLDM greeter for greetd.
//!
//! The layer-shell surface arrives in milestone 4. This build has two test
//! modes.
//!
//! - `psldm-greet --users` lists the users and the sessions that it finds.
//! - `psldm-greet --preview [WALLPAPER]` shows the pane in a normal window.
//!
//! The preview mode uses the demo backend, so it needs no greetd socket.

use std::env;
use std::path::PathBuf;

use psldm_auth::demo;
use psldm_session::constants::X11_CMD_PREFIX;
use psldm_session::{LocalUser, SysUtil};
use psldm_ui::{AppSetup, HostKind, Mode, UiConfig, UserInfo};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--users") => (),
        Some("--preview") => {
            preview(args.get(1).map(PathBuf::from));
            return;
        }
        _ => {
            eprintln!(
                "The greeter window is not built yet.\n\
                 Run `psldm-greet --users` or `psldm-greet --preview [WALLPAPER]`."
            );
            std::process::exit(2);
        }
    }

    let x11_prefix: Vec<String> = X11_CMD_PREFIX
        .split_whitespace()
        .map(String::from)
        .collect();

    let sysutil = match SysUtil::new(&x11_prefix).await {
        Ok(sysutil) => sysutil,
        Err(err) => {
            eprintln!("Cannot read the users and the sessions: {err}");
            std::process::exit(1);
        }
    };

    println!("Users:");
    for (full_name, username) in sysutil.get_users() {
        let avatar = sysutil
            .get_avatars()
            .get(username)
            .map(String::as_str)
            .unwrap_or("no avatar");
        println!("  {full_name} ({username}) [{avatar}]");
    }

    println!("Sessions:");
    for (name, session) in sysutil.get_sessions() {
        println!("  {name}: {:?}", session.command);
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
        app_id: "com.psldm.greet".into(),
        mode: Mode::Greet,
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
        sessions: vec!["Hyprland".into()],
        host: HostKind::Preview,
    };

    psldm_ui::run(setup, demo::spawn());
}
