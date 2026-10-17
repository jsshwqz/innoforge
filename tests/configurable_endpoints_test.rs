//! Issue #33: configurable OAuth callback URL and endpoints.
//!
//! These tests verify that `oauth_callback_url()` responds to environment variables:
//! - `INNOFORGE_OAUTH_CALLBACK_URL` takes precedence (when non-empty)
//! - `INNOFORGE_PORT` derives the callback URL with the configured port
//! - Default falls back to `http://localhost:3000/api/auth/google/callback`
//!
//! No running server required — these are pure function tests.

#[cfg(test)]
mod tests {
    use innoforge::common::oauth_callback_url;

    fn save_env() -> (Option<String>, Option<String>) {
        (
            std::env::var("INNOFORGE_OAUTH_CALLBACK_URL").ok(),
            std::env::var("INNOFORGE_PORT").ok(),
        )
    }

    fn restore_env(saved: (Option<String>, Option<String>)) {
        match saved.0 {
            Some(v) => std::env::set_var("INNOFORGE_OAUTH_CALLBACK_URL", v),
            None => std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL"),
        }
        match saved.1 {
            Some(v) => std::env::set_var("INNOFORGE_PORT", v),
            None => std::env::remove_var("INNOFORGE_PORT"),
        }
    }

    /// Default callback URL when no env vars are set.
    #[test]
    fn default_callback_url_is_localhost_3000() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
        std::env::remove_var("INNOFORGE_PORT");

        let url = oauth_callback_url();
        assert_eq!(
            url, "http://localhost:3000/api/auth/google/callback",
            "default callback URL should be localhost:3000"
        );

        restore_env(saved);
    }

    /// Callback URL follows INNOFORGE_PORT when set.
    #[test]
    fn callback_url_follows_innoforge_port() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
        std::env::set_var("INNOFORGE_PORT", "3900");

        let url = oauth_callback_url();
        assert_eq!(
            url, "http://localhost:3900/api/auth/google/callback",
            "callback URL should use port 3900 when INNOFORGE_PORT=3900"
        );

        restore_env(saved);
    }

    /// Callback URL changes when INNOFORGE_PORT changes — the core assertion
    /// that proves the URL is not hardcoded to 3000.
    #[test]
    fn callback_url_changes_with_port() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");

        std::env::set_var("INNOFORGE_PORT", "3000");
        let url_3000 = oauth_callback_url();

        std::env::set_var("INNOFORGE_PORT", "8080");
        let url_8080 = oauth_callback_url();

        assert_ne!(
            url_3000, url_8080,
            "callback URL must change when INNOFORGE_PORT changes"
        );
        assert!(url_3000.contains(":3000"), "url_3000 should contain :3000");
        assert!(url_8080.contains(":8080"), "url_8080 should contain :8080");

        restore_env(saved);
    }

    /// Explicit INNOFORGE_OAUTH_CALLBACK_URL overrides everything.
    #[test]
    fn explicit_callback_url_overrides_port() {
        let saved = save_env();
        std::env::set_var(
            "INNOFORGE_OAUTH_CALLBACK_URL",
            "https://example.com/auth/callback",
        );
        std::env::set_var("INNOFORGE_PORT", "3900");

        let url = oauth_callback_url();
        assert_eq!(
            url, "https://example.com/auth/callback",
            "explicit INNOFORGE_OAUTH_CALLBACK_URL should override INNOFORGE_PORT"
        );

        restore_env(saved);
    }

    /// Empty INNOFORGE_OAUTH_CALLBACK_URL falls back to INNOFORGE_PORT.
    #[test]
    fn empty_callback_url_falls_back_to_port() {
        let saved = save_env();
        std::env::set_var("INNOFORGE_OAUTH_CALLBACK_URL", "");
        std::env::set_var("INNOFORGE_PORT", "4321");

        let url = oauth_callback_url();
        assert_eq!(
            url, "http://localhost:4321/api/auth/google/callback",
            "empty INNOFORGE_OAUTH_CALLBACK_URL should fall back to INNOFORGE_PORT"
        );

        restore_env(saved);
    }

    /// Invalid INNOFORGE_PORT falls back to default 3000.
    #[test]
    fn invalid_port_falls_back_to_default() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");
        std::env::set_var("INNOFORGE_PORT", "not-a-port");

        let url = oauth_callback_url();
        assert_eq!(
            url, "http://localhost:3000/api/auth/google/callback",
            "invalid INNOFORGE_PORT should fall back to default 3000"
        );

        restore_env(saved);
    }
}
