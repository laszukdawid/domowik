//! The typed error every command returns.
//!
//! The CLI flattens it into a message and an exit code; the hosted service
//! maps [`ErrorKind`] to an HTTP status and sends the rest as JSON. Both read
//! the same fields, so a refusal means the same thing on either front end.

use serde::Serialize;
use uuid::Uuid;

/// What kind of refusal this is. Stable: the service sends it as `code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// The request was malformed: a reference in no recognised form, nothing
    /// to change, contradictory arguments. The CLI's exit 2.
    Usage,
    /// A reference named nothing in the plan.
    NotFound,
    /// A reference named more than one thing; `candidates` lists them.
    Ambiguous,
    /// The plan or the core refused the edit (a rule, a conflict, a degraded
    /// plan).
    Refused,
}

/// One of the things an ambiguous reference could mean.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: Uuid,
    /// How commands name it back: `W3 #4e1a92`, `D2 #91c0de`.
    pub label: String,
    /// The level's name, when the candidates span more than one level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommandError {
    #[serde(rename = "code")]
    pub kind: ErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
}

impl CommandError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            candidates: Vec::new(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Usage, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Refused, message)
    }

    pub fn ambiguous(message: impl Into<String>, candidates: Vec<Candidate>) -> Self {
        Self {
            kind: ErrorKind::Ambiguous,
            message: message.into(),
            candidates,
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<domowik_edit::EditError> for CommandError {
    fn from(e: domowik_edit::EditError) -> Self {
        Self::refused(e.to_string())
    }
}

impl From<domowik_document::session::CommandError> for CommandError {
    fn from(e: domowik_document::session::CommandError) -> Self {
        Self::refused(e.to_string())
    }
}

impl From<domowik_container::ContainerError> for CommandError {
    fn from(e: domowik_container::ContainerError) -> Self {
        Self::refused(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CommandError>;
