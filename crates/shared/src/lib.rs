//! Types and helpers shared by the starstar CLI and API.

/// Application name, used in banners and API responses.
pub const APP_NAME: &str = "starstar";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_starstar() {
        assert_eq!(APP_NAME, "starstar");
    }
}
