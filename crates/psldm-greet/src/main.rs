// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PSLDM greeter for greetd.
//!
//! - `psldm-greet` shows the greeter on every monitor. greetd must run it,
//!   because it needs the `GREETD_SOCK` socket.
//! - `psldm-greet --preview [WALLPAPER]` shows the pane in a normal window
//!   and uses the demo backend, so it needs no greetd socket.
//! - `psldm-greet --preview-layer [WALLPAPER]` shows the pane on a
//!   layer-shell surface with the demo backend. Run it inside a nested
//!   compositor to test that surface.
//! - `psldm-greet --users` lists the users and the sessions that it finds.

use std::env;
use std::path::{Path, PathBuf};

use psldm_auth::{demo, greetd};
use psldm_session::constants::X11_CMD_PREFIX;
use psldm_session::{Cache, LocalUser, SysUtil, settings};
use psldm_ui::{AppSetup, HostKind, Mode, SessionChoice, UiConfig, UserInfo};

/// The wallpaper for the greeter. The greeter user has no home directory.
const SYSTEM_WALLPAPER: &str = "/etc/psldm/wallpaper";

fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None => greet(None),
        Some("--wallpaper") => greet(args.get(1).map(PathBuf::from)),
        Some("--preview") => preview(args.get(1).map(PathBuf::from), HostKind::Preview),
        Some("--preview-layer") => preview(args.get(1).map(PathBuf::from), HostKind::LayerShell),
        Some("--users") => {
            list_users();
            return;
        }
        _ => {
            eprintln!(
                "Usage:\n  \
                 psldm-greet [--wallpaper PATH]\n  \
                 psldm-greet --preview [WALLPAPER]\n  \
                 psldm-greet --preview-layer [WALLPAPER]\n  \
                 psldm-greet --users"
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

/// Show the greeter and talk to greetd.
fn greet(wallpaper: Option<PathBuf>) -> gtk::glib::ExitCode {
    // The greetd client reads a socket, so it needs a worker thread. The GTK
    // main loop keeps the first thread.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("Cannot start the Tokio runtime: {err}");
            return gtk::glib::ExitCode::FAILURE;
        }
    };

    let backend = match runtime.block_on(greetd::spawn()) {
        Ok(backend) => backend,
        Err(err) => {
            eprintln!("Cannot reach greetd: {err}");
            return gtk::glib::ExitCode::FAILURE;
        }
    };

    let system = runtime.block_on(read_system());
    let mut setup = build_setup(wallpaper, HostKind::LayerShell, system);
    setup.app_id = "com.psldm.greet".into();

    psldm_ui::run(setup, backend)
}

/// Show the pane with the demo backend, on the given surface.
fn preview(wallpaper: Option<PathBuf>, host: HostKind) -> gtk::glib::ExitCode {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("cannot start the Tokio runtime");
    let system = runtime.block_on(read_system());

    let setup = build_setup(wallpaper, host, system);
    psldm_ui::run(setup, demo::spawn())
}

/// The users and the sessions of this computer.
struct System {
    users: Vec<UserInfo>,
    sessions: Vec<SessionChoice>,
    last_user: Option<String>,
}

/// Read the users and the sessions.
///
/// AccountsService gives every user. If it is missing, the greeter falls back
/// to the user who runs it, so that the pane still works.
async fn read_system() -> System {
    let x11_prefix: Vec<String> = X11_CMD_PREFIX
        .split_whitespace()
        .map(String::from)
        .collect();

    let cache = Cache::new();
    let last_user = cache.get_last_user().map(str::to_string);

    match SysUtil::new(&x11_prefix).await {
        Ok(sysutil) => {
            let mut users: Vec<UserInfo> = sysutil
                .get_users()
                .iter()
                .map(|(display_name, username)| UserInfo {
                    username: username.clone(),
                    display_name: display_name.clone(),
                    avatar: sysutil.get_avatars().get(username).map(PathBuf::from),
                })
                .collect();
            users.sort_by(|left, right| left.display_name.cmp(&right.display_name));

            let mut sessions: Vec<SessionChoice> = sysutil
                .get_sessions()
                .iter()
                .map(|(name, session)| SessionChoice {
                    name: name.clone(),
                    command: session.command.clone(),
                })
                .collect();
            sessions.sort_by(|left, right| left.name.cmp(&right.name));

            System {
                users,
                sessions,
                last_user,
            }
        }
        Err(err) => {
            tracing::warn!("Cannot read AccountsService: {err}");
            System {
                users: LocalUser::current().map(local_user).into_iter().collect(),
                sessions: Vec::new(),
                last_user,
            }
        }
    }
}

fn local_user(user: LocalUser) -> UserInfo {
    UserInfo {
        username: user.username,
        display_name: user.full_name,
        avatar: user.avatar,
    }
}

/// Build the settings for one run.
fn build_setup(wallpaper: Option<PathBuf>, host: HostKind, system: System) -> AppSetup {
    let System {
        users,
        sessions,
        last_user,
    } = system;

    let user = last_user
        .and_then(|name| users.iter().find(|user| user.username == name).cloned())
        .or_else(|| users.first().cloned())
        .unwrap_or_else(|| UserInfo {
            username: "user".into(),
            display_name: "User".into(),
            avatar: None,
        });

    let wallpaper = wallpaper.or_else(|| {
        let path = Path::new(SYSTEM_WALLPAPER);
        path.exists().then(|| path.to_path_buf())
    });

    AppSetup {
        app_id: "com.psldm.greet".into(),
        mode: Mode::Greet,
        config: UiConfig {
            wallpaper,
            font: settings::font(),
            ..UiConfig::default()
        },
        user,
        users,
        sessions,
        environment: Vec::new(),
        reboot: vec!["systemctl".into(), "reboot".into()],
        poweroff: vec!["systemctl".into(), "poweroff".into()],
        host,
    }
}

/// Print what the greeter finds on this computer.
fn list_users() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("cannot start the Tokio runtime");
    let system = runtime.block_on(read_system());

    println!("Users:");
    for user in &system.users {
        let avatar = user
            .avatar
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "no avatar".into());
        println!("  {} ({}) [{avatar}]", user.display_name, user.username);
    }

    println!("Sessions:");
    for session in &system.sessions {
        println!("  {}: {:?}", session.name, session.command);
    }
}
