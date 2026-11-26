use axum::Json;
use axum::http::header::{RETRY_AFTER, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use starstar_shared::ErrorBody;

use crate::prusalink::PrusaError;
use crate::store::StoreError;

/// The challenge on a 403 for a too-low role. The auth layer adds the
/// `resource_metadata` parameter to it.
pub const INSUFFICIENT_SCOPE_CHALLENGE: &str = r#"Bearer error="insufficient_scope""#;

/// Errors returned to API clients. Each maps to one HTTP status.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    /// No credentials, or credentials that are invalid or expired.
    Unauthorized(String),
    /// 403 for a check that is not about roles: CSRF, setup code, current password.
    Forbidden(String),
    /// 403 because the caller's role is below what the action needs. Like
    /// [`ApiError::Forbidden`], plus a `WWW-Authenticate: Bearer
    /// error="insufficient_scope"` challenge (RFC 6750 section 3.1).
    InsufficientRole(String),
    NotFound(String),
    Conflict(String),
    /// 411: an upload arrived without `Content-Length`.
    LengthRequired(String),
    /// 413: an upload is larger than the configured limit.
    PayloadTooLarge(String),
    /// 429: too many attempts (login, setup, token).
    TooManyRequests(String),
    /// 503 with `Retry-After`: the server is too busy right now (argon2
    /// permits exhausted).
    ServiceUnavailable(String),
    /// The printer answered with an error status that is passed on as-is
    /// (404, 409, 403 from PrusaLink). Distinct from [`ApiError::InsufficientRole`],
    /// which is about the caller's starstar role.
    Printer(StatusCode, String),
    /// 504: the printer did not answer in time.
    GatewayTimeout(String),
    /// The printer could not be reached or rejected the request.
    Upstream(String),
    Internal(String),
}

impl ApiError {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) | Self::InsufficientRole(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::LengthRequired(_) => StatusCode::LENGTH_REQUIRED,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Printer(status, _) => *status,
            Self::GatewayTimeout(_) => StatusCode::GATEWAY_TIMEOUT,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub(crate) fn message(self) -> String {
        match self {
            Self::BadRequest(m)
            | Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::InsufficientRole(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::LengthRequired(m)
            | Self::PayloadTooLarge(m)
            | Self::TooManyRequests(m)
            | Self::ServiceUnavailable(m)
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
        let insufficient = matches!(self, Self::InsufficientRole(_));
        let busy = matches!(self, Self::ServiceUnavailable(_));
        let body = ErrorBody {
            error: self.message(),
        };
        let mut response = (status, Json(body)).into_response();
        if insufficient {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                HeaderValue::from_static(INSUFFICIENT_SCOPE_CHALLENGE),
            );
        }
        if busy {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("5"));
        }
        response
    }
}

impl From<PrusaError> for ApiError {
    fn from(err: PrusaError) -> Self {
        let (status, detail) = match &err {
            PrusaError::Status { status, detail } => (*status, detail.as_deref()),
            PrusaError::Timeout => {
                return Self::GatewayTimeout("printer did not respond in time".to_owned());
            }
            PrusaError::Transport(_) | PrusaError::Decode(_) => {
                return Self::Upstream(err.to_string());
            }
        };
        let with_detail = |message: String| match detail {
            Some(d) => format!("{message}: {d}"),
            None => message,
        };
        match status.as_u16() {
            404 => Self::Printer(status, with_detail("not found on the printer".to_owned())),
            409 => Self::Printer(
                status,
                with_detail("printer is busy or the target is in use".to_owned()),
            ),
            403 => Self::Printer(
                status,
                with_detail("printer refused the request".to_owned()),
            ),
            408 => Self::GatewayTimeout(with_detail("printer timed out".to_owned())),
            401 => Self::Upstream(with_detail(
                "printer rejected the stored credentials".to_owned(),
            )),
            503 => Self::Upstream(with_detail("printer service unavailable".to_owned())),
            other => Self::Upstream(with_detail(format!("printer responded with HTTP {other}"))),
        }
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
