# Red→Green Evidence: Issue #30 — Upstream HTTP timeout (W1)

## Branch
`r20/w1-timeout` from `origin/main` (069a2b4)

## Red: Before fix — `reqwest::Client::new()` has no timeout

### 6 locations with bare `reqwest::Client::new()` (no timeout):

| File | Line | Context |
|------|------|---------|
| `src/routes/patent.rs` | 77 | SerpAPI patent details fetch |
| `src/routes/patent.rs` | 470 | `fetch_patent()` — EPO/USPTO fetch |
| `src/routes/auth.rs` | 289 | Google token refresh (ADC) |
| `src/routes/auth.rs` | 475 | Google OAuth code exchange |
| `src/routes/auth.rs` | 647 | Google token refresh (ensure_google_token) |
| `src/routes/idea.rs` | 2326 | AI model list probe |

### Red evidence (verbatim test output):

```
// Test: no_timeout_client_hangs_on_hanging_server
// Proves bare Client::new() does NOT time out on its own.

running 1 test
test no_timeout_client_hangs_on_hanging_server ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.00s
```

**Interpretation**: The test uses an *outer* `tokio::time::timeout(5s, ...)` to prove the
bare `Client::new()` request is still pending after 5 seconds. Without the outer timeout,
the request would hang indefinitely. This is the defect described in issue #30: when an
upstream server accepts the connection but never responds, the handler is stuck forever,
which can exhaust the worker pool and cause `ERR_EMPTY_RESPONSE` on other endpoints.

## Green: After fix — `crate::common::build_upstream_client()` applies timeout

### Fix applied:

1. **`src/common.rs`**: Added `upstream_timeout_secs()` and `build_upstream_client()`:
   - Reads `INNOFORGE_UPSTREAM_TIMEOUT_SECS` env var (default 30s, invalid/zero → default)
   - Builds `reqwest::Client::builder().timeout(Duration::from_secs(...)).build()`

2. **All 6 locations** replaced `reqwest::Client::new()` → `crate::common::build_upstream_client()`

### Green evidence (verbatim test output):

```
running 6 tests
test upstream_timeout_default_is_30s ... ok
test upstream_timeout_configurable_via_env ... ok
test upstream_timeout_zero_falls_back_to_default ... ok
test upstream_timeout_invalid_env_falls_back_to_default ... ok
test build_upstream_client_returns_valid_client ... ok
test upstream_client_times_out_on_hanging_server ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.00s
```

### Positive assertions in `upstream_client_times_out_on_hanging_server`:

1. `assert!(result.is_err())` — request to hanging server **must fail** (timeout error), not hang
2. `assert!(elapsed < Duration::from_secs(10))` — failure within bounded time (configured 2s + slack)
3. `assert!(elapsed >= Duration::from_millis(1500))` — takes at least ~1.5s (proves server accepted connection, not instant ConnectionRefused)

## Environment variable

| Variable | Default | Description |
|----------|---------|-------------|
| `INNOFORGE_UPSTREAM_TIMEOUT_SECS` | `30` | Timeout in seconds for upstream HTTP calls (patent APIs, Google OAuth, AI model probes) |
