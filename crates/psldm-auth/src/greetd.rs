// SPDX-FileCopyrightText: 2022 The ReGreet Authors
// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Derived from ReGreet `src/client.rs`. See ATTRIBUTION.md.

//! The greetd backend.
//!
//! greetd runs as root. It owns the PAM conversation and it starts the user
//! session. This backend only moves messages between the user interface and
//! the greetd socket.

use std::env;

use greetd_ipc::codec::TokioCodec;
use greetd_ipc::{AuthMessageType, ErrorType, Request, Response};
use tokio::net::UnixStream;
use tracing::{error, info};

use crate::{AuthChannels, AuthError, AuthEvent, AuthHandle, AuthRequest, channel_pair};

/// The environment variable that holds the path of the greetd socket.
const GREETD_SOCK: &str = "GREETD_SOCK";

/// Connect to greetd and start the backend task.
///
/// The caller must run this inside a Tokio runtime.
pub async fn spawn() -> Result<AuthHandle, AuthError> {
    let path = env::var(GREETD_SOCK)
        .map_err(|_| AuthError::Greetd(format!("{GREETD_SOCK} is not set. Is greetd running?")))?;
    let socket = UnixStream::connect(&path)
        .await
        .map_err(|err| AuthError::Greetd(err.to_string()))?;

    let (handle, channels) = channel_pair();
    tokio::spawn(run(socket, channels));
    Ok(handle)
}

async fn run(mut socket: UnixStream, mut channels: AuthChannels) {
    while let Some(request) = channels.requests.recv().await {
        let session_start = matches!(request, AuthRequest::StartSession { .. });
        let message = match request {
            AuthRequest::Start { username } => {
                info!("Creating a greetd session for {username}");
                Request::CreateSession { username }
            }
            AuthRequest::Respond(response) => Request::PostAuthMessageResponse { response },
            AuthRequest::Cancel => Request::CancelSession,
            AuthRequest::StartSession {
                command,
                environment,
            } => Request::StartSession {
                cmd: command,
                env: environment,
            },
        };

        if let Err(err) = message.write_to(&mut socket).await {
            let _ = channels.events.send(AuthEvent::Closed(err.to_string()));
            return;
        }

        let response = match Response::read_from(&mut socket).await {
            Ok(response) => response,
            Err(err) => {
                let _ = channels.events.send(AuthEvent::Closed(err.to_string()));
                return;
            }
        };

        let event = match response {
            Response::Success if session_start => AuthEvent::SessionStarted,
            Response::Success => AuthEvent::Success,
            Response::AuthMessage {
                auth_message_type,
                auth_message,
            } => match auth_message_type {
                AuthMessageType::Secret => AuthEvent::Prompt {
                    text: auth_message,
                    secret: true,
                },
                AuthMessageType::Visible => AuthEvent::Prompt {
                    text: auth_message,
                    secret: false,
                },
                AuthMessageType::Info => AuthEvent::Info(auth_message),
                AuthMessageType::Error => AuthEvent::Error(auth_message),
            },
            Response::Error {
                error_type,
                description,
            } => match error_type {
                ErrorType::AuthError => {
                    // greetd refuses a new session while the failed one is
                    // open. Cancel it here, so the user interface only has to
                    // send `Start` again. The PAM backend behaves the same.
                    if let Err(err) = cancel(&mut socket).await {
                        let _ = channels.events.send(AuthEvent::Closed(err));
                        return;
                    }
                    AuthEvent::Failure(description)
                }
                ErrorType::Error => {
                    error!("greetd reported an error: {description}");
                    AuthEvent::Error(description)
                }
            },
        };

        if channels.events.send(event).is_err() {
            return;
        }
    }
}

/// Cancel the open greetd session and read the reply.
async fn cancel(socket: &mut UnixStream) -> Result<(), String> {
    Request::CancelSession
        .write_to(socket)
        .await
        .map_err(|err| err.to_string())?;
    Response::read_from(socket)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}
