// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! A backend for the preview window.
//!
//! The backend accepts one password. It runs no PAM code and it needs no
//! greetd socket, so the preview runs as a normal application.

use tracing::warn;

use crate::{AuthChannels, AuthEvent, AuthHandle, AuthRequest, channel_pair};

/// The only password that the preview accepts.
pub const DEMO_PASSWORD: &str = "pass";

/// Start the preview backend on a new thread.
pub fn spawn() -> AuthHandle {
    warn!("The preview backend accepts the password '{DEMO_PASSWORD}'");
    let (handle, channels) = channel_pair();
    std::thread::Builder::new()
        .name("psldm-demo".into())
        .spawn(move || run(channels))
        .expect("cannot start the preview thread");
    handle
}

fn run(channels: AuthChannels) {
    let AuthChannels {
        mut requests,
        events,
    } = channels;

    while let Some(request) = requests.blocking_recv() {
        let event = match request {
            AuthRequest::Start { .. } => AuthEvent::Prompt {
                text: "Password:".into(),
                secret: true,
            },
            AuthRequest::Respond(answer) => {
                if answer.as_deref() == Some(DEMO_PASSWORD) {
                    AuthEvent::Success
                } else {
                    AuthEvent::Failure("Incorrect password".into())
                }
            }
            AuthRequest::Cancel => continue,
            AuthRequest::StartSession { .. } => AuthEvent::SessionStarted,
        };

        if events.send(event).is_err() {
            return;
        }
    }
}
