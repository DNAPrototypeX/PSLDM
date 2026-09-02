// SPDX-FileCopyrightText: 2022 The ReGreet Authors
// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paths that the build can change.
//!
//! Set the environment variable at build time to change a path. A package for
//! another distribution can point these at its own directories.

/// Read an environment variable at build time, or return the default.
macro_rules! env_or {
    ($name:expr, $default:expr) => {
        // `Option::unwrap_or` is not a const function, so use a match.
        // See https://github.com/rust-lang/rust/issues/91930
        if let Some(value) = option_env!($name) {
            value
        } else {
            $default
        }
    };
}

/// Directories with the desktop files of the X11 and Wayland sessions. A colon
/// divides the directories.
///
/// The list holds both default data directories of the XDG standard. A
/// distribution puts its session files in `/usr/share`, and a local
/// installation puts them in `/usr/local/share`. The greeter often runs with
/// no XDG_DATA_DIRS, so it must know both.
pub const SESSION_DIRS: &str = env_or!(
    "PSLDM_SESSION_DIRS",
    "/usr/local/share/xsessions:/usr/local/share/wayland-sessions:\
     /usr/share/xsessions:/usr/share/wayland-sessions"
);

/// The file that holds the last user and the last session of each user.
pub const CACHE_PATH: &str = env_or!("PSLDM_CACHE_PATH", "/var/lib/psldm/state.toml");

/// The command that starts the X server for an X11 session.
pub const X11_CMD_PREFIX: &str = env_or!("PSLDM_X11_CMD_PREFIX", "startx /usr/bin/env");
