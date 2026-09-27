//! One error type for the whole backend.
//!
//! Every Tauri command returns `AppResult<T>`. Errors cross the IPC boundary as
//! a plain `{ code, message }` object so the frontend can branch on `code`
//! without string-matching, and so internal detail (SQL text, file paths) never
//! leaks into a message a headteacher might read.

use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),

    #[error("Not found")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("You are not signed in.")]
    Unauthenticated,

    #[error("You do not have permission to do that.")]
    Forbidden,

    #[error("That username or password is not correct.")]
    BadCredentials,

    #[error("This account is locked. Try again later or ask a School Admin to reset it.")]
    AccountLocked,

    #[error("Setup has not been completed yet.")]
    NotProvisioned,

    #[error("Database error")]
    Database(#[from] rusqlite::Error),

    #[error("File error")]
    Io(#[from] std::io::Error),

    #[error("Spreadsheet could not be read")]
    Spreadsheet(String),

    #[error("Something went wrong")]
    Internal(String),
}

impl AppError {
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }

    pub fn not_found(what: impl Into<String>) -> Self {
        AppError::NotFound(what.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        AppError::Internal(msg.into())
    }

    /// Stable machine-readable code. The frontend branches on this.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Unauthenticated => "unauthenticated",
            AppError::Forbidden => "forbidden",
            AppError::BadCredentials => "bad_credentials",
            AppError::AccountLocked => "account_locked",
            AppError::NotProvisioned => "not_provisioned",
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::Spreadsheet(_) => "spreadsheet",
            AppError::Internal(_) => "internal",
        }
    }

    /// Text safe to put in front of a user.
    pub fn user_message(&self) -> String {
        match self {
            AppError::NotFound(what) => format!("{what} could not be found."),
            AppError::Spreadsheet(detail) => {
                format!("That spreadsheet could not be read: {detail}")
            }
            // Database/Io/Internal deliberately fall back to the terse Display
            // text so paths and SQL never reach the screen. The detail is logged.
            other => other.to_string(),
        }
    }
}

#[derive(Serialize)]
struct WireError {
    code: &'static str,
    message: String,
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Log the full internal detail on the way out; send only the safe part.
        match self {
            AppError::Database(e) => log::error!("database error: {e}"),
            AppError::Io(e) => log::error!("io error: {e}"),
            AppError::Internal(m) => log::error!("internal error: {m}"),
            _ => {}
        }

        WireError {
            code: self.code(),
            message: self.user_message(),
        }
        .serialize(serializer)
    }
}
