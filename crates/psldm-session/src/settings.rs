// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings that both programs read.
//!
//! Each setting is one small file in `/etc/psldm`. The greeter runs as
//! another user with no home directory, so a file in `/etc` is the only place
//! that both programs can read.

use std::fs::read_to_string;
use std::path::Path;

/// The file that holds the font family.
const FONT_PATH: &str = "/etc/psldm/font";

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
