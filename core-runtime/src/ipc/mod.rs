//! IPC module for CORE Runtime.
//!
//! Handles named pipe/Unix socket communication with authenticated callers.
//! This is the ONLY external interface - no HTTP/REST/WebSocket allowed.

mod auth;
pub mod encoding;
mod handler;
pub mod protocol;

pub use auth::{AuthError, SessionAuth, SessionToken};
pub use encoding::{get_encoder, TokenEncoder, V1Encoder};
pub use handler::{IpcHandler, IpcHandlerConfig};
pub use protocol::{
    decode_message, encode_message, InferenceRequest, InferenceResponse, IpcMessage,
    ProtocolError, ProtocolVersion, RequestId,
};
