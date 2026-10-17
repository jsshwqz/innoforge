# Red→Green Evidence: PR#48 / Issue #33 — Configurable OAuth callback URL (W2)

## Branch
`r20/w2-configurable-endpoints` from `origin/main` (069a2b4)

## Red: Before fix — hardcoded REDIRECT_URI

### The defect (PR#48 review point 2):
`src/routes/auth.rs:22` had:
```rust
const REDIRECT_URI: &str = "http://localhost:3000/api/auth/google/callback";
```

This constant was used in 3 places:
- Line 400: `urlencoding::encode(REDIRECT_URI)` — building Google OAuth auth URL
- Line 404: `"redirect_uri": REDIRECT_URI` — returning to frontend
- Line 481: `form.insert("redirect_uri", REDIRECT_URI.to_string())` — token exchange

**Red condition**: When `INNOFORGE_PORT=3900`, the redirect_uri was still `http://localhost:3000/api/auth/google/callback` — OAuth silently fails because Google Cloud Console requires exact match.

### Red evidence (PR#48 CI failure):
```
tests/configurable_endpoints_test.rs:167
panic: connect and read from server: Os { code: 111, kind: ConnectionRefused }
```
The old test tried to connect to a running server (which doesn't exist in CI), causing ConnectionRefused.

## Green: After fix — `crate::common::oauth_callback_url()`

### Fix applied:

1. **`src/common.rs`**: Added `pub fn oauth_callback_url()` with priority:
   - `INNOFORGE_OAUTH_CALLBACK_URL` env var (non-empty) → use directly
   - `INNOFORGE_PORT` env var → `http://localhost:{port}/api/auth/google/callback`
   - Default → `http://localhost:3000/api/auth/google/callback`

2. **`src/routes/auth.rs`**: Replaced all 3 `REDIRECT_URI` usages with `crate::common::oauth_callback_url()`

3. **`tests/configurable_endpoints_test.rs`**: 6 tests, no running server needed (pure function tests)

### Green evidence (verbatim test output):

```
running 6 tests
test tests::callback_url_follows_innoforge_port ... ok
test tests::default_callback_url_is_localhost_3000 ... ok
test tests::empty_callback_url_falls_back_to_port ... ok
test tests::explicit_callback_url_overrides_port ... ok
test tests::invalid_port_falls_back_to_default ... ok
test tests::callback_url_changes_with_port ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Key positive assertion — `callback_url_changes_with_port`:
```rust
std::env::set_var("INNOFORGE_PORT", "3000");
let url_3000 = oauth_callback_url();
std::env::set_var("INNOFORGE_PORT", "8080");
let url_8080 = oauth_callback_url();
assert_ne!(url_3000, url_8080, "callback URL must change when INNOFORGE_PORT changes");
```
This assertion would **fail** with the old hardcoded `REDIRECT_URI` constant (both would be `http://localhost:3000/...`).

## Documentation

- `README.md`: Added "Environment Variables" table with `INNOFORGE_PORT`, `INNOFORGE_OAUTH_CALLBACK_URL`, `INNOFORGE_UPSTREAM_TIMEOUT_SECS`
- `docs/API.md`: Added "环境变量 / Environment Variables" section with same three variables
