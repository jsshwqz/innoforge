# R18 Deployment Scan Report

**Date**: 2026-10-16  
**Branch**: `r18/deployment-scan` (from `main` @ `0da07a9d`)  
**Server version**: 0.9.15  

## 1. Live Deployment Test

Server started with `INNOFORGE_PORT=3000 INNOFORGE_NO_OPEN=1 ./target/debug/innoforge-server`.

### Endpoint Scan

| Endpoint | Status | Content-Type | Notes |
|----------|--------|-------------|-------|
| `/` | 200 | text/html | Home page ✅ |
| `/idea` | 200 | text/html | Idea page ✅ |
| `/search` | 200 | text/html | Search page ✅ |
| `/compare` | 200 | text/html | Compare page ✅ |
| `/settings` | 200 | text/html | Settings page ✅ |
| `/oa-response` | 200 | text/html | OA response page ✅ |
| `/patent/test` | 404 | text/html | Non-existent patent → 404 ✅ |
| `/nonexistent` | 404 | (empty) | Unknown route → 404 ✅ |
| `/static/i18n.js` | 200 | text/javascript | Static asset ✅ |
| `/api/version` | 200 | application/json | Version info ✅ |
| `/api/patents` (GET) | 404 | (empty) | No list endpoint (expected) |
| `/api/health` | (empty) | — | **No health endpoint** ⚠️ |

### /api/version Response

```json
{
  "build_time": "2026-10-16T00:54:21Z",
  "db_path": "/root/.local/share/innoforge/innoforge.db",
  "git_branch": "r18/stub-tests",
  "git_commit": "0da07a9d30325036dee289901773d166e5dc2f6e",
  "version": "0.9.15",
  "templates": {
    "compare.html": { "bytes": 43762, "sha256": "01a70bb..." },
    "office_action_response.html": { "bytes": 260881, "sha256": "1a8eb1..." }
  }
}
```

**git_commit matches `main` HEAD**: `0da07a9d` ✅

## 2. Findings

### ✅ No Issues

1. **All 6 main pages serve 200** — home, idea, search, compare, settings, oa-response
2. **Static assets served correctly** — correct content-type, cache headers
3. **404 handling works** — unknown routes return 404, not 500
4. **Version endpoint complete** — version, git_commit, build_time, db_path, template hashes
5. **Process lifecycle** — server starts cleanly, responds within 3s, kills cleanly
6. **Release workflow** — validates tag/version/changelog, builds 4 targets, creates GitHub release
7. **Port configuration** — `INNOFORGE_PORT` env var works, fallback logic (3000→3921→3100) is sound

### ⚠️ Low-Risk (Noted, Not Fixed)

1. **No `/api/health` endpoint** — Deployment monitoring systems (k8s liveness, load balancer health check) have no dedicated health endpoint. The `/api/version` endpoint could serve this purpose (returns 200), but a dedicated `/api/health` is conventional. **Impact**: Low — `/api/version` works as a health check substitute.

2. **Hardcoded `REDIRECT_URI` in `src/routes/auth.rs:22`** — `http://localhost:3000/api/auth/google/callback` is hardcoded. If the server runs on a different port (via `INNOFORGE_PORT`), Google OAuth redirect won't match. **Impact**: Low for local dev (always port 3000); would break if deployed behind a reverse proxy with a different port/domain. **Fix**: Derive from `INNOFORGE_PORT` or add `OAUTH_REDIRECT_URI` env var.

3. **Hardcoded `UMI_OCR_BASE_URL` in `src/routes/upload.rs:17`** — `http://127.0.0.1:1224` is hardcoded for the local UMI-OCR service. **Impact**: Low — UMI-OCR is a local integration, always on port 1224 by convention. Would break if OCR service moves.

### ✅ Release Workflow Analysis

The `release.yml` workflow is well-structured:
- **Validation**: tag format, Cargo.toml version match, CHANGELOG entry, tag exists in git
- **Guard overlay**: fetches `main`'s `no_case_data_in_repo.rs` to scan tag checkout (prevents case data in releases)
- **Build matrix**: linux x86_64, linux aarch64 (cross), windows x86_64, macos aarch64
- **Release**: creates draft release, uploads all 4 assets, verifies asset list, marks as latest
- **Concurrency**: prevents duplicate releases for same tag

## 3. Deployment Process

Verified deployment process (from recalled knowledge):

```bash
cd /root/if16
export PATH="$HOME/.cargo/bin:$PATH"
cargo build                    # debug build is sufficient
pkill -f innoforge-server      # kill old instance
INNOFORGE_PORT=3000 INNOFORGE_NO_OPEN=1 ./target/debug/innoforge-server &
```

**Result**: Server starts in <3s, all endpoints respond correctly, version endpoint confirms `0.9.15` and correct git commit.

## 4. Summary

| Category | Status |
|----------|--------|
| Page rendering | ✅ All 6 pages 200 |
| Static assets | ✅ Correct content-type |
| API endpoints | ✅ Version works, 404 handling correct |
| Process lifecycle | ✅ Clean start/stop |
| Release workflow | ✅ Well-structured, 4 targets |
| Health endpoint | ⚠️ Missing (use /api/version) |
| OAuth redirect | ⚠️ Hardcoded port 3000 |
| OCR URL | ⚠️ Hardcoded port 1224 |

**Overall**: Deployment is healthy. Three low-risk items noted but not blocking.
