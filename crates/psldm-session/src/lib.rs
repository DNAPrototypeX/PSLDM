// SPDX-FileCopyrightText: 2022 The ReGreet Authors
// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! System information that the greeter and the locker share.
//!
//! The crate reads the users from AccountsService and the sessions from the
//! desktop files. It also stores the last user and the last session between
//! logins. Most of the code comes from ReGreet. See ATTRIBUTION.md.

pub mod cache;
pub mod constants;
pub mod local;
pub mod sysutil;
pub mod tomlutils;

pub use cache::Cache;
pub use local::LocalUser;
pub use sysutil::{SessionInfo, SessionType, SysUtil};
