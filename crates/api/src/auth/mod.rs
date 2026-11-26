//! Authentication and authorization.
//!
//! **Interface contract (stable across parts):**
//!
//! - Handlers declare the minimum role they need by taking one of the
//!   extractors [`Viewer`], [`Operator`] or [`Admin`] as a parameter. Each one
//!   rejects the request with 401 (no or bad credentials) or 403 (role too low)
//!   before the handler runs, and gives access to the authenticated [`Caller`].
//! - Non-handler code (MCP tools) calls [`Caller::require`] with the role it needs.
//! - [`router`] holds the auth-owned routes (setup, session, OAuth, users, apps)
//!   and is merged into the main router by [`crate::router`].
//!
//! SCAFFOLD: until the auth part lands, every extractor accepts every request
//! as an anonymous admin. Do not deploy this build.

use axum::Router;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use starstar_shared::Role;

use crate::AppState;
use crate::error::ApiError;

/// The authenticated identity behind a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// Username of the signed-in user.
    pub username: String,
    /// Effective role: the lower of the token's grant and the user's current role.
    pub role: Role,
}

impl Caller {
    /// Returns 403 unless this caller has at least `min`.
    pub fn require(&self, min: Role) -> Result<(), ApiError> {
        if self.role >= min {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!(
                "this action needs the {min} role, you have {}",
                self.role
            )))
        }
    }
}

/// Resolves the caller from the request (bearer token or session cookie).
///
/// SCAFFOLD: always returns an anonymous admin.
pub async fn authenticate(_parts: &mut Parts, _state: &AppState) -> Result<Caller, ApiError> {
    Ok(Caller {
        username: "anonymous".to_owned(),
        role: Role::Admin,
    })
}

macro_rules! role_extractor {
    ($name:ident, $role:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone)]
        pub struct $name(pub Caller);

        impl FromRequestParts<AppState> for $name {
            type Rejection = ApiError;

            async fn from_request_parts(
                parts: &mut Parts,
                state: &AppState,
            ) -> Result<Self, Self::Rejection> {
                let caller = authenticate(parts, state).await?;
                caller.require($role)?;
                Ok(Self(caller))
            }
        }
    };
}

role_extractor!(Viewer, Role::Viewer, "Requires at least the viewer role.");
role_extractor!(
    Operator,
    Role::Operator,
    "Requires at least the operator role."
);
role_extractor!(Admin, Role::Admin, "Requires the admin role.");

/// Auth-owned routes. SCAFFOLD: empty.
pub fn router(state: AppState) -> Router {
    Router::new().with_state(state)
}
