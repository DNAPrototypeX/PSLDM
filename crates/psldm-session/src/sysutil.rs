// SPDX-FileCopyrightText: 2022 The ReGreet Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helper for system utilities like users and sessions

mod accounts_service;

use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::path::Path;

use freedesktop_entry_parser::Entry;
use glob::glob;
use shlex::Shlex;
use tracing::{debug, info, warn};
use zbus::Connection;

use self::accounts_service::AccountsServiceProxy;
use self::accounts_service::UserProxy;
use crate::constants::SESSION_DIRS;

/// XDG data directory variable name (parent directory for X11/Wayland sessions)
const XDG_DIR_ENV_VAR: &str = "XDG_DATA_DIRS";

#[derive(Clone, Copy)]
pub enum SessionType {
    X11,
    Wayland,
    Unknown,
}

#[derive(Clone)]
pub struct SessionInfo {
    pub command: Vec<String>,
    pub sess_type: SessionType,
}

// Convenient aliases for used maps
type UserMap = HashMap<String, String>;
type ShellMap = HashMap<String, String>;
type AvatarMap = HashMap<String, String>;
type SessionMap = HashMap<String, SessionInfo>;

/// Stores info of all regular users and sessions
pub struct SysUtil {
    /// Maps a user's full name to their system username
    users: UserMap,
    /// Maps a system username to their shell
    shells: ShellMap,
    /// Maps a system username to the path of their avatar image
    avatars: AvatarMap,
    /// Maps a session's full name to its command
    sessions: SessionMap,
}

impl SysUtil {
    /// Read the users from AccountsService and the sessions from disk.
    ///
    /// `x11_prefix` starts the X server for an X11 session. The locker passes
    /// an empty slice, because it does not start a session.
    pub async fn new(x11_prefix: &[String]) -> Result<Self, Box<dyn Error>> {
        let dbus_system_conn = Connection::system().await?;
        let accounts_proxy = AccountsServiceProxy::new(&dbus_system_conn).await?;

        let mut user_proxies = Vec::new();
        for user_path in accounts_proxy.list_cached_users().await? {
            let user_proxy = UserProxy::builder(&dbus_system_conn)
                .path(user_path)?
                .build()
                .await?;

            user_proxies.push(user_proxy);
        }

        let mut usernames = HashMap::new();

        for user_proxy in &user_proxies {
            let mut real_name = user_proxy.real_name().await?;
            let user_name = user_proxy.user_name().await?;

            // If real name is not set, just use the username instead
            if real_name.is_empty() {
                real_name.clone_from(&user_name);
            }

            usernames.insert(real_name, user_name);
        }

        let mut shells = HashMap::new();
        let mut avatars = HashMap::new();

        for user_proxy in &user_proxies {
            let user_name = user_proxy.user_name().await?;
            let shell = user_proxy.shell().await?;

            // An avatar is optional. The readable copy comes first.
            // AccountsService often reports a path inside a home directory,
            // and the greeter runs as another user, so it cannot read that
            // one. The property can even name a file that no longer exists.
            let mut icon =
                crate::local::system_avatar(&user_name).map(|path| path.display().to_string());
            if icon.is_none() {
                icon = match user_proxy.icon_file().await {
                    Ok(path) if !path.is_empty() && Path::new(&path).is_file() => Some(path),
                    Ok(_) => None,
                    Err(err) => {
                        debug!("No avatar property for {user_name}: {err}");
                        None
                    }
                };
            }
            if let Some(icon) = icon {
                avatars.insert(user_name.clone(), icon);
            }

            shells.insert(user_name, shell);
        }

        Ok(Self {
            users: usernames,
            shells,
            avatars,
            sessions: Self::init_sessions(x11_prefix).await?,
        })
    }

    /// Get available X11 and Wayland sessions.
    ///
    /// These are defined as either X11 or Wayland session desktop files stored in specific
    /// directories.
    async fn init_sessions(x11_prefix: &[String]) -> Result<SessionMap, Box<dyn Error>> {
        let mut found_session_names = HashSet::new();
        let mut sessions = HashMap::new();

        // The XDG variable comes first, and the compiled list follows. The
        // greeter often runs with no XDG_DATA_DIRS, and a session file in
        // /usr/local/share would then stay hidden.
        let mut session_dirs: Vec<String> = Vec::new();
        let mut seen = HashSet::new();

        if let Ok(parent_dirs) = env::var(XDG_DIR_ENV_VAR) {
            debug!("Found XDG env var {XDG_DIR_ENV_VAR}: {parent_dirs}");
            for parent in parent_dirs.split(':').filter(|dir| !dir.is_empty()) {
                for kind in ["xsessions", "wayland-sessions"] {
                    let dir = format!("{parent}/{kind}");
                    if seen.insert(dir.clone()) {
                        session_dirs.push(dir);
                    }
                }
            }
        }

        for dir in SESSION_DIRS.split(':').map(str::trim) {
            if !dir.is_empty() && seen.insert(dir.to_string()) {
                session_dirs.push(dir.to_string());
            }
        }

        for sess_dir in &session_dirs {
            let sess_dir_path = Path::new(sess_dir.as_str());
            let sess_parent_dir = if let Some(sess_parent_dir) = sess_dir_path.parent() {
                sess_parent_dir
            } else {
                warn!("Session directory does not have a parent: {sess_dir}");
                continue;
            };

            let is_x11 = if let Some(name) = sess_dir_path.file_name() {
                name == "xsessions"
            } else {
                false
            };
            let cmd_prefix = if is_x11 { Some(x11_prefix) } else { None };

            debug!("Checking session directory: {sess_dir}");
            // Iterate over all '.desktop' files.
            for glob_path in glob(&format!("{sess_dir}/*.desktop"))
                .expect("Invalid glob pattern for session desktop files")
            {
                let path = match glob_path {
                    Ok(path) => path,
                    Err(err) => {
                        warn!("Error when globbing: {err}");
                        continue;
                    }
                };
                info!("Now scanning session file: {}", path.display());

                let fname_and_type = match path.strip_prefix(sess_parent_dir) {
                    Ok(fname_and_type) => fname_and_type.to_owned(),
                    Err(err) => {
                        warn!("Error with file name: {err}");
                        continue;
                    }
                };

                if found_session_names.contains(&fname_and_type) {
                    debug!(
                        "{fname_and_type:?} was already found elsewhere, skipping {}",
                        path.display()
                    );
                    continue;
                };

                let entry = Entry::parse(tokio::fs::read(&path).await?)?;
                let section = if let Some(section) = entry.section("Desktop Entry") {
                    section
                } else {
                    warn!("Session file {} is not a desktop entry", path.display());
                    continue;
                };

                let hidden = section
                    .attr("Hidden")
                    .first()
                    .is_some_and(|s| s.parse().unwrap_or(false));
                let no_display = section
                    .attr("NoDisplay")
                    .first()
                    .is_some_and(|s| s.parse().unwrap_or(false));

                if hidden | no_display {
                    found_session_names.insert(fname_and_type);
                    continue;
                };

                // Parse the desktop file to get the session command.
                let cmd = if let Some(cmd_str) = section.attr("Exec").first() {
                    let mut cmd = if let Some(prefix) = cmd_prefix {
                        prefix.to_vec()
                    } else {
                        Vec::new()
                    };
                    let prefix_len = cmd.len();
                    cmd.extend(Shlex::new(cmd_str.as_str()));
                    if cmd.len() > prefix_len {
                        cmd
                    } else {
                        warn!(
                            "Couldn't split command of '{}' into arguments: {}",
                            path.display(),
                            cmd_str.as_str()
                        );
                        // Skip the desktop file, since a missing command means that we can't
                        // use it.
                        continue;
                    }
                } else {
                    warn!("No command found for session: {}", path.display());
                    // Skip the desktop file, since a missing command means that we can't use it.
                    continue;
                };

                // Get the full name of this session.
                let name = if let Some(name) = section.attr("Name").first() {
                    debug!(
                        "Found name '{}' for session '{}' with command '{:?}'",
                        name.as_str(),
                        path.display(),
                        cmd
                    );
                    name.as_str()
                } else if let Some(stem) = path.file_stem() {
                    // Get the stem of the filename of this desktop file.
                    // This is used as backup, in case the file name doesn't exist.
                    if let Some(stem) = stem.to_str() {
                        debug!(
                            "Using file stem '{stem}', since no name was found for session: {}",
                            path.display()
                        );
                        stem
                    } else {
                        warn!("Non-UTF-8 file stem in session file: {}", path.display());
                        // No way to display this session name, so just skip it.
                        continue;
                    }
                } else {
                    warn!("No file stem found for session: {}", path.display());
                    // No file stem implies no file name, which shouldn't happen.
                    // Since there's no full name nor file stem, just skip this anomalous
                    // session.
                    continue;
                };
                found_session_names.insert(fname_and_type);
                sessions.insert(
                    name.to_string(),
                    SessionInfo {
                        command: cmd,
                        sess_type: if is_x11 {
                            SessionType::X11
                        } else {
                            SessionType::Wayland
                        },
                    },
                );
            }
        }

        Ok(sessions)
    }

    /// Get the mapping of a user's full name to their system username.
    ///
    /// If the full name is not available, their system username is used.
    pub fn get_users(&self) -> &UserMap {
        &self.users
    }

    /// Get the mapping of a system username to their shell.
    pub fn get_shells(&self) -> &ShellMap {
        &self.shells
    }

    /// Get the mapping of a system username to the path of their avatar.
    pub fn get_avatars(&self) -> &AvatarMap {
        &self.avatars
    }

    /// Get the full name of one user, or the user name when no full name is
    /// set.
    pub fn full_name_of(&self, username: &str) -> Option<&str> {
        self.users
            .iter()
            .find(|(_, name)| name.as_str() == username)
            .map(|(full_name, _)| full_name.as_str())
    }

    /// Get the mapping of a session's full name to its command.
    ///
    /// If the full name is not available, the filename stem is used.
    pub fn get_sessions(&self) -> &SessionMap {
        &self.sessions
    }
}
