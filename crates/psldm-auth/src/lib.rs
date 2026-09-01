// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Authentication backends for PSLDM.
//!
//! The greeter and the locker use the same user interface. They differ in the
//! backend that answers the prompts. The greeter talks to greetd, which runs
//! PAM as root. The locker talks to PAM directly.
//!
//! Both backends use the same two channels. The user interface sends an
//! [`AuthRequest`]. The backend replies with one or more [`AuthEvent`] values.
//! The user interface never learns which backend is active.

pub mod demo;
pub mod greetd;
pub mod pam;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// A message from the user interface to the backend.
#[derive(Debug, Clone)]
pub enum AuthRequest {
    /// Start an attempt for this user.
    Start { username: String },
    /// Answer the last prompt. `None` answers with an empty response.
    Respond(Option<String>),
    /// Stop the current attempt.
    Cancel,
    /// Start the user session. The greetd backend uses this. The PAM backend
    /// ignores it, because the locker returns to a session that already runs.
    StartSession {
        command: Vec<String>,
        environment: Vec<String>,
    },
}

/// A message from the backend to the user interface.
#[derive(Debug, Clone)]
pub enum AuthEvent {
    /// Ask the user for a value. `secret` hides the text in the field.
    Prompt { text: String, secret: bool },
    /// Show a message from PAM.
    Info(String),
    /// Show an error message from PAM.
    Error(String),
    /// The user is authenticated.
    Success,
    /// The attempt failed. The user can try again.
    Failure(String),
    /// greetd accepted the session. The greeter must now exit.
    SessionStarted,
    /// The backend stopped. No more events arrive.
    Closed(String),
}

/// The two channels that connect the user interface to one backend.
pub struct AuthHandle {
    requests: UnboundedSender<AuthRequest>,
    events: UnboundedReceiver<AuthEvent>,
}

impl AuthHandle {
    /// Send a request to the backend.
    pub fn send(&self, request: AuthRequest) -> Result<(), AuthError> {
        self.requests.send(request).map_err(|_| AuthError::Closed)
    }

    /// Wait for the next event from the backend.
    pub async fn next_event(&mut self) -> Option<AuthEvent> {
        self.events.recv().await
    }

    /// Take the next event, or return `None` if no event waits.
    pub fn try_next_event(&mut self) -> Option<AuthEvent> {
        self.events.try_recv().ok()
    }

    /// Divide the handle into a sender and a receiver.
    ///
    /// A widget callback keeps a sender. The event loop keeps the receiver.
    pub fn split(self) -> (AuthSender, AuthReceiver) {
        (AuthSender(self.requests), AuthReceiver(self.events))
    }
}

/// The sender half of [`AuthHandle`]. Clone it into each callback.
#[derive(Clone)]
pub struct AuthSender(UnboundedSender<AuthRequest>);

impl AuthSender {
    pub fn send(&self, request: AuthRequest) -> Result<(), AuthError> {
        self.0.send(request).map_err(|_| AuthError::Closed)
    }
}

/// The receiver half of [`AuthHandle`].
pub struct AuthReceiver(UnboundedReceiver<AuthEvent>);

impl AuthReceiver {
    pub async fn next_event(&mut self) -> Option<AuthEvent> {
        self.0.recv().await
    }
}

/// The backend side of the two channels.
pub(crate) struct AuthChannels {
    pub requests: UnboundedReceiver<AuthRequest>,
    pub events: UnboundedSender<AuthEvent>,
}

/// Create one channel pair. The caller keeps the handle and starts a backend
/// with the channels.
pub(crate) fn channel_pair() -> (AuthHandle, AuthChannels) {
    let (request_tx, request_rx) = unbounded_channel();
    let (event_tx, event_rx) = unbounded_channel();
    (
        AuthHandle {
            requests: request_tx,
            events: event_rx,
        },
        AuthChannels {
            requests: request_rx,
            events: event_tx,
        },
    )
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("the authentication backend is closed")]
    Closed,
    #[error("cannot reach greetd: {0}")]
    Greetd(String),
    #[error("cannot start PAM: {0}")]
    Pam(String),
}
