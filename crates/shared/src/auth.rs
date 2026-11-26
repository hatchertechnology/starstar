//! Request and response types for authentication, users and OAuth.
//!
//! Types that carry secrets (passwords, tokens, CSRF tokens, setup codes) do
//! not derive `Debug`, so they cannot end up in logs by accident.

use serde::{Deserialize, Serialize};

use crate::Role;

/// The pre-registered public OAuth client used by the starstar CLI.
pub const CLI_CLIENT_ID: &str = "starstar-cli";
/// Path of the CLI's loopback redirect.
pub const CLI_REDIRECT_PATH: &str = "/callback";
/// Path of the copy-code page, relative to the public URL.
pub const COPY_CODE_PATH: &str = "/oauth/code";
/// Name of the HttpOnly session cookie.
pub const SESSION_COOKIE: &str = "starstar_session";
/// Name of the JavaScript-readable CSRF cookie.
pub const CSRF_COOKIE: &str = "starstar_csrf";
/// Header that must echo the CSRF token on unsafe cookie-authenticated requests.
pub const CSRF_HEADER: &str = "x-csrf-token";

/// Usernames: `^[a-z0-9][a-z0-9._-]{0,31}$`.
pub fn is_valid_username(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=32).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes[1..].iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Body of `POST /api/setup`.
#[derive(Clone, Serialize, Deserialize)]
pub struct SetupRequest {
    pub setup_code: String,
    pub username: String,
    pub password: String,
}

/// Body of `POST /api/session`.
#[derive(Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// The signed-in browser session.
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub username: String,
    pub role: Role,
    pub csrf_token: String,
}

/// Body of `POST /api/users`.
#[derive(Clone, Serialize, Deserialize)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub role: Role,
}

/// Body of `PUT /api/me/password`.
#[derive(Clone, Serialize, Deserialize)]
pub struct ChangePassword {
    pub current_password: String,
    pub new_password: String,
}

/// Successful response of `POST /oauth/token` (RFC 6749 section 5.1).
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scope: String,
}

/// Response of `GET /api/me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhoAmI {
    pub username: String,
    pub role: Role,
}

/// A user as shown to admins. Never includes the password hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSummary {
    pub username: String,
    pub role: Role,
    pub disabled: bool,
    pub created_at: i64,
}

/// Body of `PATCH /api/users/{username}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateUser {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// A connected app: one OAuth grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppGrant {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub username: String,
    pub role: Role,
    pub resource: Option<String>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: i64,
}

/// What the consent screen needs to show for one pending authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRequest {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    /// True for dynamically registered clients, whose name is not verified.
    pub client_name_self_declared: bool,
    pub redirect_host: String,
    pub client_registered_at: i64,
    pub requested_role: Role,
    pub user_role: Role,
    pub default_role: Role,
    pub max_role: Role,
}

/// RFC 6749 error body used by the OAuth endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthErrorBody {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_usernames() {
        for ok in ["kerry", "a", "op-2", "j.doe_x"] {
            assert!(is_valid_username(ok), "{ok}");
        }
        for bad in ["", "Kerry", "-x", ".x", "a b", "a/b", &"a".repeat(33)] {
            assert!(!is_valid_username(bad), "{bad}");
        }
    }

    #[test]
    fn update_user_omits_absent_fields() {
        let json = serde_json::to_string(&UpdateUser {
            role: Some(Role::Viewer),
            disabled: None,
        })
        .unwrap();
        assert_eq!(json, r#"{"role":"viewer"}"#);
        assert_eq!(
            serde_json::from_str::<UpdateUser>("{}").unwrap(),
            UpdateUser {
                role: None,
                disabled: None
            }
        );
    }
}
