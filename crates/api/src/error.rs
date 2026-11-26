use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use starstar_shared::ErrorBody;

use crate::prusalink::PrusaError;
use crate::store::StoreError;

/// Errors returned to API clients. Each maps to one HTTP status.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    /// No credentials, or credentials that are invalid or expired.
    Unauthorized(String),
    /// Authenticated, but the caller's role is too low.
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    /// 411: an upload arrived without `Content-Length`.
    LengthRequired(String),
    /// 413: an upload is larger than the configured limit.
    PayloadTooLarge(String),
    /// 429: too many attempts (login, setup, token).
    TooManyRequests(String),
    /// The printer answered with an error status that is passed on as-is
    /// (404, 409, 403 from PrusaLink). Distinct from [`ApiError::Forbidden`],
    /// which is about the caller's starstar role.
    Printer(StatusCode, String),
    /// 504: the printer did not answer in time.
    GatewayTimeout(String),
    /// The printer could not be reached or rejected the request.
    Upstream(String),
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::LengthRequired(_) => StatusCode::LENGTH_REQUIRED,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Printer(status, _) => *status,
            Self::GatewayTimeout(_) => StatusCode::GATEWAY_TIMEOUT,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(self) -> String {
        match self {
            Self::BadRequest(m)
            | Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::LengthRequired(m)
            | Self::PayloadTooLarge(m)
            | Self::TooManyRequests(m)
            | Self::Printer(_, m)
            | Self::GatewayTimeout(m)
            | Self::Upstream(m)
            | Self::Internal(m) => m,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorBody {
            error: self.message(),
        };
        (status, Json(body)).into_response()
    }
}

impl From<PrusaError> for ApiError {
    fn from(err: PrusaError) -> Self {
        Self::Upstream(err.to_string())
    }
}

impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::Duplicate(name) => {
                Self::Conflict(format!("printer `{name}` already exists"))
            }
            other => Self::Internal(other.to_string()),
        }
    }
}
