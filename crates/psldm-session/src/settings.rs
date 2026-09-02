// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings that both programs read.
//!
//! Each setting is one small file in `/etc/psldm`. The greeter runs as
//! another user with no home directory, so a file in `/etc` is the only place
//! that both programs can read.

use std::fs::read_to_string;
use std::path::{Path, PathBuf};

/// The file that holds the font family.
const FONT_PATH: &str = "/etc/psldm/font";

/// The wallpaper that `install.sh` copies for every user.
const SYSTEM_WALLPAPER: &str = "/etc/psldm/wallpaper";

/// The environment variable that replaces the wallpaper file.
const WALLPAPER_ENV: &str = "PSLDM_WALLPAPER";

/// The environment variable that replaces the font file. It helps a test.
const FONT_ENV: &str = "PSLDM_FONT";

/// The font family for the pane, or `None` when nothing sets one.
///
/// `install.sh` writes the file from the font of the user who installs PSLDM.
pub fn font() -> Option<String> {
    if let Ok(family) = std::env::var(FONT_ENV) {
        let family = family.trim().to_string();
        if !family.is_empty() {
            return Some(family);
        }
    }
    read_setting(FONT_PATH)
}

/// Read one short setting from a file. Empty means missing.
fn read_setting(path: &str) -> Option<String> {
    let text = read_to_string(Path::new(path)).ok()?;
    let value = text.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// The wallpaper for the pane, or `None` when there is no image.
///
/// The order is the environment variable, then the file of the user, then
/// the file for the whole computer. The greeter runs as another user with no
/// home directory, so it reaches the last one.
pub fn wallpaper() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(WALLPAPER_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let user_file = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/psldm/wallpaper"));

    user_file
        .into_iter()
        .chain([PathBuf::from(SYSTEM_WALLPAPER)])
        .find(|path| path.is_file())
}
