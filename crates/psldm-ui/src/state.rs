// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine of the login pane.
//!
//! The pane holds one user name, at most one prompt, and at most one message.
//! [`LoginState::on_event`] reads an [`AuthEvent`] and returns the action for
//! the caller. The caller owns the backend, so the state machine stays free of
//! input and output.

use psldm_auth::{AuthEvent, AuthRequest};

use crate::Mode;

/// What the screen shows.
///
/// macOS shows only the clock until the user presses a key. The login pane
/// appears after that first key, and it goes away again after a quiet period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The clock only.
    Idle,
    /// The clock, the avatar, and the field.
    Active,
}

/// A question from the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// The text that the backend sent, such as `Password:`.
    pub text: String,
    /// Hide the text in the field.
    pub secret: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Error,
}

/// A message under the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
}

/// What the caller must do after an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    /// Do nothing.
    None,
    /// Send [`AuthRequest::Start`] again, because the attempt failed.
    Restart,
    /// The user is authenticated. The locker hides the lock surface.
    Unlock,
    /// The user is authenticated. The greeter starts the session.
    StartSession,
    /// The greeter exits, and greetd starts the session.
    Exit,
}

/// The state of the login pane.
#[derive(Debug, Clone)]
pub struct LoginState {
    /// The mode decides which parts of the interface appear.
    pub mode: Mode,
    /// The system user name of the selected user.
    pub username: String,
    /// The open question, if the backend asked one.
    pub prompt: Option<Prompt>,
    /// The message under the field.
    pub message: Option<Message>,
    /// The backend works, so the field is read-only.
    pub busy: bool,
    /// Play the failure animation one time.
    pub shake: bool,
    /// The backend stopped.
    pub closed: bool,
    /// The clock only, or the whole pane.
    pub phase: Phase,
}

impl LoginState {
    /// Create the state for one mode and one user.
    pub fn new(mode: Mode, username: impl Into<String>) -> Self {
        Self {
            mode,
            username: username.into(),
            prompt: None,
            message: None,
            busy: false,
            shake: false,
            closed: false,
            phase: Phase::Idle,
        }
    }

    /// Show the login pane. Returns `true` if the screen must change.
    pub fn wake(&mut self) -> bool {
        let changed = self.phase == Phase::Idle;
        self.phase = Phase::Active;
        changed
    }

    /// Show the clock only. Returns `true` if the screen must change.
    ///
    /// The pane stays while the backend works, because an answer is on its
    /// way.
    pub fn sleep(&mut self) -> bool {
        if self.busy || self.phase == Phase::Idle {
            return false;
        }
        self.phase = Phase::Idle;
        self.message = None;
        true
    }

    /// Build the request that starts an attempt for the current user.
    ///
    /// The message stays on the screen. A failed attempt starts a new one at
    /// once, and the user must still see why the last one failed.
    pub fn start(&mut self) -> AuthRequest {
        self.prompt = None;
        self.busy = true;
        AuthRequest::Start {
            username: self.username.clone(),
        }
    }

    /// Build the request that answers the open prompt.
    ///
    /// Returns `None` when no prompt is open, so a second Enter key does
    /// nothing.
    pub fn submit(&mut self, answer: impl Into<String>) -> Option<AuthRequest> {
        self.prompt.take()?;
        self.busy = true;
        self.message = None;
        Some(AuthRequest::Respond(Some(answer.into())))
    }

    /// Read one event and report the action for the caller.
    pub fn on_event(&mut self, event: AuthEvent) -> UiAction {
        self.shake = false;
        match event {
            AuthEvent::Prompt { text, secret } => {
                self.prompt = Some(Prompt { text, secret });
                self.busy = false;
                UiAction::None
            }
            AuthEvent::Info(text) => {
                self.message = Some(Message {
                    kind: MessageKind::Info,
                    text,
                });
                UiAction::None
            }
            AuthEvent::Error(text) => {
                self.message = Some(Message {
                    kind: MessageKind::Error,
                    text,
                });
                UiAction::None
            }
            AuthEvent::Failure(text) => {
                self.prompt = None;
                self.busy = false;
                self.shake = true;
                self.message = Some(Message {
                    kind: MessageKind::Error,
                    text,
                });
                UiAction::Restart
            }
            AuthEvent::Success => {
                self.prompt = None;
                self.message = None;
                self.busy = true;
                match self.mode {
                    Mode::Lock => UiAction::Unlock,
                    Mode::Greet => UiAction::StartSession,
                }
            }
            AuthEvent::SessionStarted => UiAction::Exit,
            AuthEvent::Closed(text) => {
                self.closed = true;
                self.busy = false;
                self.message = Some(Message {
                    kind: MessageKind::Error,
                    text,
                });
                UiAction::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password_prompt() -> AuthEvent {
        AuthEvent::Prompt {
            text: "Password:".into(),
            secret: true,
        }
    }

    #[test]
    fn a_prompt_ends_the_busy_state() {
        let mut state = LoginState::new(Mode::Lock, "paul");
        state.start();
        assert!(state.busy);

        state.on_event(password_prompt());
        assert!(!state.busy);
        assert!(state.prompt.as_ref().unwrap().secret);
    }

    #[test]
    fn a_submit_needs_an_open_prompt() {
        let mut state = LoginState::new(Mode::Lock, "paul");
        state.start();
        assert!(state.submit("secret").is_none());

        state.on_event(password_prompt());
        assert!(state.submit("secret").is_some());
        assert!(state.submit("secret").is_none());
    }

    #[test]
    fn a_failure_asks_for_a_restart() {
        let mut state = LoginState::new(Mode::Lock, "paul");
        state.start();
        state.on_event(password_prompt());
        state.submit("wrong");

        let action = state.on_event(AuthEvent::Failure("auth error".into()));
        assert_eq!(action, UiAction::Restart);
        assert!(state.shake);
        assert_eq!(state.message.unwrap().kind, MessageKind::Error);
    }

    #[test]
    fn the_failure_message_stays_for_the_next_attempt() {
        let mut state = LoginState::new(Mode::Lock, "paul");
        state.start();
        state.on_event(password_prompt());
        state.submit("wrong");
        state.on_event(AuthEvent::Failure("Incorrect password".into()));

        state.start();
        assert_eq!(state.message.as_ref().unwrap().text, "Incorrect password");

        state.on_event(password_prompt());
        assert!(state.submit("second try").is_some());
        assert!(state.message.is_none());
    }

    #[test]
    fn the_pane_appears_on_the_first_key_and_goes_away_again() {
        let mut state = LoginState::new(Mode::Lock, "paul");
        assert_eq!(state.phase, Phase::Idle);

        assert!(state.wake());
        assert!(!state.wake());
        assert_eq!(state.phase, Phase::Active);

        state.start();
        state.on_event(password_prompt());
        assert!(state.sleep());
        assert_eq!(state.phase, Phase::Idle);
    }

    #[test]
    fn every_return_to_the_clock_needs_a_new_first_key() {
        let mut state = LoginState::new(Mode::Lock, "paul");

        // The caller starts the password with the key whenever `wake`
        // reports a change.
        for _ in 0..3 {
            assert!(state.wake());
            assert!(!state.wake());
            assert!(state.sleep());
            assert!(!state.sleep());
        }
    }

    #[test]
    fn the_pane_stays_while_the_backend_works() {
        let mut state = LoginState::new(Mode::Lock, "paul");
        state.wake();
        state.start();
        state.on_event(password_prompt());
        state.submit("secret");

        assert!(state.busy);
        assert!(!state.sleep());
        assert_eq!(state.phase, Phase::Active);
    }

    #[test]
    fn success_differs_between_the_two_modes() {
        let mut lock = LoginState::new(Mode::Lock, "paul");
        assert_eq!(lock.on_event(AuthEvent::Success), UiAction::Unlock);

        let mut greet = LoginState::new(Mode::Greet, "paul");
        assert_eq!(greet.on_event(AuthEvent::Success), UiAction::StartSession);
    }

    #[test]
    fn the_locker_hides_the_power_menu_and_the_user_picker() {
        let lock = Mode::Lock.chrome();
        assert!(!lock.power_menu);
        assert!(!lock.user_picker);

        let greet = Mode::Greet.chrome();
        assert!(greet.power_menu);
        assert!(greet.user_picker);
    }
}
