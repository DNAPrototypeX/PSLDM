// SPDX-FileCopyrightText: 2026 The ReGreet Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Barebones D-Bus interface for the org.freedesktop.Accounts D-Bus service.

use zbus::{proxy, zvariant::OwnedObjectPath};

#[proxy(
    default_path = "/org/freedesktop/Accounts",
    default_service = "org.freedesktop.Accounts",
    interface = "org.freedesktop.Accounts"
)]
pub trait AccountsService {
    /// Returns an array of [`User`] paths.
    fn list_cached_users(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    default_service = "org.freedesktop.Accounts",
    default_path = "/org/freedesktop/Accounts",
    interface = "org.freedesktop.Accounts.User"
)]
pub trait User {
    #[zbus(property)]
    fn user_name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn real_name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn shell(&self) -> zbus::Result<String>;

    /// The path of the avatar image of the user. The value is empty when the
    /// user has no avatar.
    #[zbus(property)]
    fn icon_file(&self) -> zbus::Result<String>;
}
