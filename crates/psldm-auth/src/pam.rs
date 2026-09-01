// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PAM backend for the locker.
//!
//! PAM is blocking, and one attempt must stay on one thread. This backend runs
//! the attempt on its own thread. The conversation callbacks block on the
//! request channel while they wait for the user.
//!
//! The locker does not need root. `pam_unix` calls the setuid helper
//! `unix_chkpwd` when it cannot read `/etc/shadow`.

use std::ffi::{CStr, CString};
use std::sync::{Arc, Mutex};

use pam::{Client, Conversation};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info, warn};

use crate::{AuthChannels, AuthEvent, AuthHandle, AuthRequest, channel_pair};

/// Start the PAM backend on a new thread.
///
/// `service` is the file name in `/etc/pam.d`, such as `psldm`.
pub fn spawn(service: impl Into<String>) -> AuthHandle {
    let service = service.into();
    let (handle, channels) = channel_pair();
    std::thread::Builder::new()
        .name("psldm-pam".into())
        .spawn(move || run(service, channels))
        .expect("cannot start the PAM thread");
    handle
}

fn run(service: String, channels: AuthChannels) {
    let AuthChannels { requests, events } = channels;
    let requests = Arc::new(Mutex::new(requests));

    loop {
        // Wait for the user interface to start an attempt.
        let username = {
            let mut rx = requests.lock().expect("the PAM channel lock is poisoned");
            match rx.blocking_recv() {
                Some(AuthRequest::Start { username }) => username,
                Some(_) => continue,
                None => return,
            }
        };

        let conversation = UiConversation {
            username: username.clone(),
            events: events.clone(),
            requests: Arc::clone(&requests),
        };

        let mut client = match Client::with_conversation(&service, conversation) {
            Ok(client) => client,
            Err(err) => {
                let _ = events.send(AuthEvent::Closed(format!(
                    "cannot open the PAM service {service}: {err}"
                )));
                return;
            }
        };

        info!("Starting a PAM attempt for {username}");
        let event = match client.authenticate() {
            Ok(()) => AuthEvent::Success,
            Err(err) => {
                warn!("The PAM attempt failed: {err}");
                AuthEvent::Failure(err.to_string())
            }
        };

        let done = matches!(event, AuthEvent::Success);
        if events.send(event).is_err() || done {
            return;
        }
    }
}

/// The bridge between the PAM callbacks and the user interface.
struct UiConversation {
    username: String,
    events: UnboundedSender<AuthEvent>,
    requests: Arc<Mutex<UnboundedReceiver<AuthRequest>>>,
}

impl UiConversation {
    /// Show a prompt, then block until the user interface answers.
    fn ask(&mut self, message: &CStr, secret: bool) -> Result<CString, ()> {
        let text = message.to_string_lossy().into_owned();
        self.events
            .send(AuthEvent::Prompt { text, secret })
            .map_err(|_| ())?;

        let mut rx = self.requests.lock().map_err(|_| ())?;
        loop {
            match rx.blocking_recv() {
                Some(AuthRequest::Respond(Some(answer))) => {
                    return CString::new(answer).map_err(|_| ());
                }
                Some(AuthRequest::Respond(None)) => return CString::new("").map_err(|_| ()),
                Some(AuthRequest::Cancel) => return Err(()),
                // A second `Start` or a `StartSession` has no meaning here.
                Some(_) => continue,
                None => return Err(()),
            }
        }
    }
}

impl Conversation for UiConversation {
    fn prompt_echo(&mut self, message: &CStr) -> Result<CString, ()> {
        // PAM asks for the user name. The locker knows it, so answer without
        // a prompt. Any other visible question goes to the user.
        if is_login_prompt(message) {
            return CString::new(self.username.clone()).map_err(|_| ());
        }
        self.ask(message, false)
    }

    fn prompt_blind(&mut self, message: &CStr) -> Result<CString, ()> {
        self.ask(message, true)
    }

    fn info(&mut self, message: &CStr) {
        let _ = self
            .events
            .send(AuthEvent::Info(message.to_string_lossy().into_owned()));
    }

    fn error(&mut self, message: &CStr) {
        let _ = self
            .events
            .send(AuthEvent::Error(message.to_string_lossy().into_owned()));
    }
}

/// Report whether PAM asks for the user name.
fn is_login_prompt(message: &CStr) -> bool {
    let text = message.to_string_lossy().to_lowercase();
    text.contains("login") || text.contains("username") || text.contains("user name")
}
