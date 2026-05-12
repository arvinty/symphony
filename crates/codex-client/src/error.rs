use serde_json::Value;
use thiserror::Error;

pub type RequestId = Value;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("transport closed")]
    TransportClosed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid json on {role}: {source}")]
    Decode {
        role: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("server error {code}: {message}")]
    JsonRpc { code: i64, message: String },
    #[error("request {0:?} dropped before response")]
    OneshotDropped(RequestId),
    #[error("server sent unknown method: {0}")]
    UnknownMethod(String),
}

pub type ClientResult<T> = Result<T, ClientError>;
