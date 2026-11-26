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
    NotFound(String),
    Conflict(String),
    /// The printer could not be reached or rejected the request.
    Upstream(String),
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(self) -> String {
        match self {
            Self::BadRequest(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
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
