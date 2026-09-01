// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! One user, read from `/etc/passwd`.
//!
//! AccountsService gives the full name and the avatar of every user. It is not
//! always installed, and the locker only needs the user who is logged in. This
//! module reads that one user from the files.

use std::env;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

/// The name and the avatar of one user.
#[derive(Debug, Clone)]
pub struct LocalUser {
    /// The system user name, such as `paul`.
    pub username: String,
    /// The full name from the GECOS field, or the user name.
    pub full_name: String,
    /// The avatar image, if the system has one.
    pub avatar: Option<PathBuf>,
    /// The home directory.
    pub home: PathBuf,
}

/// The directory where AccountsService keeps the avatars.
const ICON_DIR: &str = "/var/lib/AccountsService/icons";

impl LocalUser {
    /// Read the user who runs this program.
    pub fn current() -> Option<Self> {
        let username = env::var("USER").ok().filter(|name| !name.is_empty())?;
        Self::lookup(&username)
    }

    /// Read one user from `/etc/passwd`.
    pub fn lookup(username: &str) -> Option<Self> {
        let passwd = read_to_string("/etc/passwd").ok()?;
        let line = passwd
            .lines()
            .find(|line| line.split(':').next() == Some(username))?;

        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 6 {
            return None;
        }

        // The GECOS field holds the full name before the first comma.
        let full_name = fields[4]
            .split(',')
            .next()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(username)
            .to_string();

        let home = PathBuf::from(fields[5]);

        Some(Self {
            username: username.to_string(),
            full_name,
            avatar: find_avatar(username, &home),
            home,
        })
    }
}

/// The avatar that every program can read.
///
/// AccountsService keeps it in `/var/lib/AccountsService/icons`. That file is
/// the only one that the greeter can read, because the greeter runs as
/// another user and a home directory is often private.
pub fn system_avatar(username: &str) -> Option<PathBuf> {
    let path = PathBuf::from(ICON_DIR).join(username);
    path.is_file().then_some(path)
}

/// Find the avatar of a user.
///
/// The system copy comes first. A user can also put one at `~/.face`.
fn find_avatar(username: &str, home: &Path) -> Option<PathBuf> {
    if let Some(path) = system_avatar(username) {
        return Some(path);
    }
    [home.join(".face"), home.join(".face.icon")]
        .into_iter()
        .find(|path| path.is_file())
}
