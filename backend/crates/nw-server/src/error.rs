//! API errors and their JSON representation.

use axum::{
    Json,
    http::StatusCode,
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
    #[error("this account is disabled, removed or outside its validity period")]
    AccountDisabled,
    #[error("the password has expired and must be changed")]
    PasswordExpired,
    #[error("no profile allows this account to log in")]
    NoProfile,
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
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthenticated => (StatusCode::UNAUTHORIZED, "unauthenticated"),
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            Self::AccountDisabled => (StatusCode::FORBIDDEN, "account_disabled"),
            Self::PasswordExpired => (StatusCode::FORBIDDEN, "password_expired"),
            Self::NoProfile => (StatusCode::FORBIDDEN, "no_profile"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Database(_) | Self::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
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
        (
            status,
            Json(ErrorBody {
                error: ErrorDetail { code, message },
            }),
        )
            .into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
