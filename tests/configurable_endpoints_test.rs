//! Issue #33: configurable OAuth callback URL and endpoints.
//!
//! These tests verify that `oauth_callback_url(port)` behaves correctly:
//! - `INNOFORGE_OAUTH_CALLBACK_URL` takes precedence (when non-empty)
//! - Otherwise the passed `listen_port` parameter determines the URL
//! - Default falls back to `http://localhost:{port}/api/auth/google/callback`
//!
//! No running server required — these are pure function tests.

#[cfg(test)]
mod tests {
    use innoforge::common::oauth_callback_url;

    fn save_env() -> Option<String> {
        std::env::var("INNOFORGE_OAUTH_CALLBACK_URL").ok()
    }

    fn restore_env(saved: Option<String>) {
        match saved {
            Some(v) => std::env::set_var("INNOFORGE_OAUTH_CALLBACK_URL", v),
            None => std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL"),
        }
    }

    /// Default callback URL when no env vars are set.
    #[test]
    fn default_callback_url_is_localhost_3000() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");

        let url = oauth_callback_url(3000);
        assert_eq!(
            url, "http://localhost:3000/api/auth/google/callback",
            "default callback URL should be localhost:3000"
        );

        restore_env(saved);
    }

    /// Callback URL follows the passed listen_port.
    #[test]
    fn callback_url_follows_listen_port() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");

        let url = oauth_callback_url(3900);
        assert_eq!(
            url, "http://localhost:3900/api/auth/google/callback",
            "callback URL should use port 3900 when listen_port=3900"
        );

        restore_env(saved);
    }

    /// Callback URL changes when listen_port changes — the core assertion
    /// that proves the URL is not hardcoded to 3000.
    #[test]
    fn callback_url_changes_with_port() {
        let saved = save_env();
        std::env::remove_var("INNOFORGE_OAUTH_CALLBACK_URL");

        let url_3000 = oauth_callback_url(3000);
        let url_8080 = oauth_callback_url(8080);

        assert_ne!(
            url_3000, url_8080,
            "callback URL must change when listen_port changes"
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

        let url = oauth_callback_url(3000);
        assert_eq!(
            url, "https://example.com/auth/callback",
            "explicit INNOFORGE_OAUTH_CALLBACK_URL should override listen_port"
        );

        restore_env(saved);
    }

    /// Empty INNOFORGE_OAUTH_CALLBACK_URL falls back to listen_port.
    #[test]
    fn empty_callback_url_falls_back_to_port() {
        let saved = save_env();
        std::env::set_var("INNOFORGE_OAUTH_CALLBACK_URL", "");

        let url = oauth_callback_url(4321);
        assert_eq!(
            url, "http://localhost:4321/api/auth/google/callback",
            "empty INNOFORGE_OAUTH_CALLBACK_URL should fall back to listen_port"
        );

        restore_env(saved);
    }
}
