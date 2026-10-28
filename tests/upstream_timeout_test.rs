//! Issue #30: verify that upstream HTTP calls have configurable timeouts.
//!
//! These tests prove:
//! 1. `upstream_timeout_secs()` reads from `INNOFORGE_UPSTREAM_TIMEOUT_SECS` env var.
//! 2. `build_upstream_client()` produces a client that times out within the configured duration.
//! 3. When an upstream server hangs (accepts but never responds), the client returns an error
//!    within the timeout — not hanging indefinitely. This is the positive assertion: the
//!    degraded path is exercised and completes in bounded time.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Every test below mutates the process-global `INNOFORGE_UPSTREAM_TIMEOUT_SECS`
/// env var, and cargo runs tests in parallel threads within this binary. Without
/// serialization, a sibling test's `remove_var`/`set_var` can land between
/// `set_var("2")` and `build_upstream_client()` in
/// `upstream_client_times_out_on_hanging_server`, so the client is built with
/// the default 30s timeout and the bounded-time assertion fails with
/// `elapsed = 30.00xs` (observed on CI). All env-touching tests must hold this
/// lock while reading/mutating the env var.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn lock_env() -> MutexGuard<'static, ()> {
    // Recover from poisoning: a panicked sibling must not cascade failures.
    ENV_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Verify the default timeout is 30s when env var is unset.
#[test]
fn upstream_timeout_default_is_30s() {
    let _guard = lock_env();
    // Save and restore env var
    let saved = std::env::var("INNOFORGE_UPSTREAM_TIMEOUT_SECS").ok();
    std::env::remove_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS");

    let secs = innoforge::common::upstream_timeout_secs();
    assert_eq!(
        secs, 30,
        "default upstream timeout should be 30s when env var is unset"
    );

    if let Some(v) = saved {
        std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", v);
    }
}

/// Verify the timeout is configurable via environment variable.
#[test]
fn upstream_timeout_configurable_via_env() {
    let _guard = lock_env();
    let saved = std::env::var("INNOFORGE_UPSTREAM_TIMEOUT_SECS").ok();
    std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", "5");

    let secs = innoforge::common::upstream_timeout_secs();
    assert_eq!(
        secs, 5,
        "upstream timeout should be 5 when INNOFORGE_UPSTREAM_TIMEOUT_SECS=5"
    );

    if let Some(v) = saved {
        std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", v);
    } else {
        std::env::remove_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS");
    }
}

/// Verify that invalid env var values fall back to default.
#[test]
fn upstream_timeout_invalid_env_falls_back_to_default() {
    let _guard = lock_env();
    let saved = std::env::var("INNOFORGE_UPSTREAM_TIMEOUT_SECS").ok();
    std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", "not-a-number");

    let secs = innoforge::common::upstream_timeout_secs();
    assert_eq!(secs, 30, "invalid env var should fall back to default 30s");

    if let Some(v) = saved {
        std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", v);
    } else {
        std::env::remove_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS");
    }
}

/// Verify that zero env var value falls back to default.
#[test]
fn upstream_timeout_zero_falls_back_to_default() {
    let _guard = lock_env();
    let saved = std::env::var("INNOFORGE_UPSTREAM_TIMEOUT_SECS").ok();
    std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", "0");

    let secs = innoforge::common::upstream_timeout_secs();
    assert_eq!(secs, 30, "zero env var should fall back to default 30s");

    if let Some(v) = saved {
        std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", v);
    } else {
        std::env::remove_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS");
    }
}

/// Positive assertion: when an upstream server accepts the connection but never
/// responds, the client times out within the configured duration (not hanging
/// forever). This proves the timeout is actually applied to the HTTP client.
///
/// We start a TCP listener that accepts connections but never writes a response,
/// then measure how long the client takes to return an error.
#[tokio::test]
async fn upstream_client_times_out_on_hanging_server() {
    use tokio::io::AsyncReadExt;

    // Start a "black hole" server: accepts connections, never responds.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}/hang", addr);

    tokio::spawn(async move {
        loop {
            if let Ok((mut socket, _)) = listener.accept().await {
                // Read the request but never write a response — simulates a hung upstream.
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = socket.read(&mut buf).await;
                    // Intentionally do not write a response.
                    // Keep the connection open until the client times out.
                    std::future::pending::<()>().await;
                });
            }
        }
    });

    // Set a short timeout for the test (2 seconds). Hold ENV_LOCK across
    // save/set/build so no sibling test can mutate the env var in between
    // (see ENV_LOCK docs — that race made the client use the 30s default).
    let saved;
    let client = {
        let _guard = lock_env();
        saved = std::env::var("INNOFORGE_UPSTREAM_TIMEOUT_SECS").ok();
        std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", "2");
        innoforge::common::build_upstream_client()
    };
    let start = Instant::now();

    let result = client.get(&server_url).send().await;

    let elapsed = start.elapsed();

    // Restore env var
    {
        let _guard = lock_env();
        if let Some(v) = saved {
            std::env::set_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS", v);
        } else {
            std::env::remove_var("INNOFORGE_UPSTREAM_TIMEOUT_SECS");
        }
    }

    // Positive assertion: the request must fail (timeout error), not hang forever.
    assert!(
        result.is_err(),
        "request to hanging server must fail with timeout error, not hang indefinitely"
    );

    // Positive assertion: the failure must happen within a bounded time.
    // Allow some slack: timeout is 2s, so it should complete well under 10s.
    assert!(
        elapsed < Duration::from_secs(10),
        "timeout must fire within bounded time; elapsed = {:?}",
        elapsed
    );

    // Positive assertion: the elapsed time should be at least close to the timeout
    // (not instant failure like connection refused — the server accepted the connection).
    assert!(
        elapsed >= Duration::from_millis(1500),
        "timeout should take at least ~1.5s (configured 2s); elapsed = {:?}",
        elapsed
    );
}

/// Verify that `build_upstream_client()` succeeds and returns a usable client.
#[test]
fn build_upstream_client_returns_valid_client() {
    let client = innoforge::common::build_upstream_client();
    // The client should be usable (we can't easily inspect timeout, but
    // we can verify it doesn't panic and is a valid Client).
    let _ = client.get("http://127.0.0.1:0/nonexistent");
}
