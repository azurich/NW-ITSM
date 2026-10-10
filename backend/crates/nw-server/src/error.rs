//! API errors and their JSON representation.

use std::time::Duration;

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("authentication required")]
    Unauthenticated,
    #[error("invalid login or password")]
    InvalidCredentials,
    #[error("too many failed attempts, try again later")]
    TooManyAttempts { retry_after: Duration },
    #[error("this account is disabled, removed or outside its validity period")]
    AccountDisabled,
    #[error("the password has expired and must be changed")]
    PasswordExpired,
    #[error("the password has not expired")]
    PasswordNotExpired,
    #[error("no profile allows this account to log in")]
    NoProfile,
    #[error("the password of this account is managed by an external directory")]
    ExternalAccount,
    #[error("the current password is wrong")]
    WrongPassword,
    #[error("the new password must be different from the current one")]
    SamePassword,
    #[error("the new password does not follow the password policy")]
    WeakPassword(Vec<&'static str>),
    #[error("the connection bridge with the classic interface is not configured")]
    BridgeDisabled,
    #[error("access denied")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Database(#[from] nw_db::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Error payload: `{"error": {"code": "...", "message": "..."}}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable, machine-readable code (`unauthenticated`, `not_found`...).
    pub code: &'static str,
    pub message: String,
    /// More codes, for example the broken rules of `weak_password`
    /// (`too_short`, `missing_digit`, `missing_lowercase`,
    /// `missing_uppercase`, `missing_symbol`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<&'static str>,
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            Self::TooManyAttempts { .. } => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            Self::AccountDisabled => (StatusCode::FORBIDDEN, "account_disabled"),
            Self::PasswordExpired => (StatusCode::FORBIDDEN, "password_expired"),
            Self::PasswordNotExpired => (StatusCode::BAD_REQUEST, "password_not_expired"),
            Self::NoProfile => (StatusCode::FORBIDDEN, "no_profile"),
            Self::ExternalAccount => (StatusCode::FORBIDDEN, "external_account"),
            Self::WrongPassword => (StatusCode::BAD_REQUEST, "wrong_password"),
            Self::SamePassword => (StatusCode::BAD_REQUEST, "same_password"),
            Self::WeakPassword(_) => (StatusCode::BAD_REQUEST, "weak_password"),
            Self::BridgeDisabled => (StatusCode::NOT_FOUND, "bridge_disabled"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Database(_) | Self::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
    }

    /// Stable code of the error, as sent to clients.
    pub fn code(&self) -> &'static str {
        self.status_and_code().1
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self, "request failed");
            "internal server error".to_owned()
        } else {
            self.to_string()
        };
        let retry_after = match &self {
            Self::TooManyAttempts { retry_after } => Some(retry_after.as_secs().max(1)),
            _ => None,
        };
        let details = match self {
            Self::WeakPassword(details) => details,
            _ => Vec::new(),
        };
        let mut response = (
            status,
            Json(ErrorBody {
                error: ErrorDetail {
                    code,
                    message,
                    details,
                },
            }),
        )
            .into_response();
        if let Some(seconds) = retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
