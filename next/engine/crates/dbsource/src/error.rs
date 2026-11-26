//! Structured errors: a kind the screen and an LLM can act on, and a message that never holds a password.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// The user name or password was refused.
    Auth,
    /// The host could not be reached.
    Network,
    /// TLS could not be set up or the certificate was not trusted.
    Tls,
    /// Connecting or the query took longer than the limit.
    Timeout,
    /// No such connection, table, column or file.
    NotFound,
    /// Not on this platform or for this database kind.
    Unsupported,
    /// The query definition or an argument is wrong.
    Invalid,
    /// The database refused the query.
    Sql,
    /// More rows or bytes than allowed.
    Limit,
    /// The credential store could not be used.
    Credentials,
    /// The server wants a password and neither this session (`dbUnlockConnection`) nor the credential store has one: the screen asks.
    NeedsPassword,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DbError {
    pub kind: Kind,
    pub message: String,
    /// TLS: the SHA-256 of the certificate the server showed (hex with colons), for "이 인증서 신뢰".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl DbError {
    pub fn new(kind: Kind, message: impl Into<String>) -> Self { DbError { kind, message: message.into(), fingerprint: None } }
    pub fn invalid(message: impl Into<String>) -> Self { Self::new(Kind::Invalid, message) }
    pub fn unsupported(message: impl Into<String>) -> Self { Self::new(Kind::Unsupported, message) }
    /// The message without the password, should a driver have repeated it (belt and braces: the drivers are never given it in a URL).
    pub fn scrub(mut self, secret: Option<&crate::Secret>) -> Self {
        if let Some(s) = secret.map(crate::Secret::expose).filter(|s| !s.is_empty()) {
            self.message = self.message.replace(s, "***");
        }
        self
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.message) }
}

pub type Result<T> = std::result::Result<T, DbError>;
