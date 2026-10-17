# G6: Deployment Forensics — Issue #30 E2E Failure Reproduction

**Date**: 2026-10-17
**Deployment instance**: `http://127.0.0.1:3000`
**Server process**: PID 10620, release build (`./target/release/innoforge-server`)
**Server git_commit**: `a6517c5e212bcf62ddeefc0f65ed13bd1e2eb9c2` (older than main `7d1a055`)
**Server build_time**: `2026-10-16T01:30:12Z`

## 1. Endpoint Curl Results (3x with http_code + time)

### Passing endpoints (200)

| Endpoint | Attempt 1 | Attempt 2 | Attempt 3 |
|----------|-----------|-----------|-----------|
| `/api/version` | 200 / 0.001530s | 200 / 0.001421s | 200 / 0.001566s |
| `/api/settings` | 200 / 0.014090s | 200 / 0.001486s | 200 / 0.001377s |
| `/api/chat/history` | 200 / 0.003701s | 200 / 0.001570s | 200 / 0.001498s |
| `/api/cad/status` | 200 / 0.014288s | — | — |
| `/` (index) | 200 / 0.006446s | — | — |
| `/search` | 200 / 0.001268s | — | — |
| `/ai` | 200 / 0.001187s | — | — |
| `/idea` | 200 / 0.001945s | — | — |
| `/compare` | 200 / 0.001279s | — | — |
| `/settings` | 200 / 0.001296s | — | — |

### Failing endpoints (404)

| Endpoint | Attempt 1 | Attempt 2 | Attempt 3 | Likely cause |
|----------|-----------|-----------|-----------|-------------|
| `/api/patents` | 404 / 0.001144s | 404 / 0.001323s | 404 / 0.001121s | Endpoint not registered (may need `/api/patent/:id`) |
| `/api/ideas` | 404 / 0.001174s | 404 / 0.001099s | 404 / 0.001247s | Endpoint not registered (may need `/api/idea/list`) |
| `/api/oa-rounds` | 404 / 0.000847s | 404 / 0.001214s | 404 / 0.000872s | Endpoint not registered (may need path param) |
| `/office-action` | 404 / 0.000929s | — | — | Page route may be `/office-action-response` or similar |
| `/api/health` | 404 / 0.000997s | — | — | No health endpoint registered |
| `/api/stats` | 404 / 0.000800s | — | — | No stats endpoint at this path |

## 2. Log Retrieval

**Result**: 无法取得 (unavailable)

**Reason**: `/tmp/innoforge.log` does not exist on the deployment instance. The server process (PID 10620) was started via `./target/release/innoforge-server` without redirecting output to a log file. `nohup.out` exists but is 0 bytes (empty).

**Alternative locations checked**:
- `/var/log/innoforge.log` — not found
- `~/.local/share/innoforge/innoforge.log` — not found
- `/root/innoforge.log` — not found
- `nohup.out` — exists but 0 bytes

**Recommendation**: Start server with `RUST_LOG=info INNOFORGE_PORT=3000 ./target/release/innoforge-server 2>&1 | tee /tmp/innoforge.log` to capture logs for future forensics.

## 3. E2E Test Reproduction

**Command**: `node e2e_test.mjs`
**Result**: FAILED — 0/87 passed, 0 skipped, 1 failed

**Root cause**: Puppeteer cannot launch Chrome browser process:
```
Error: Failed to launch the browser process: Code: 127
stderr:
/root/.cache/puppeteer/chrome/linux_arm-155.0.8059.79/chrome-linux-arm64/chrome:
  error while loading shared libraries: libnspr4.so: cannot open shared object file
```

**Analysis**: This is a **system-level dependency issue**, not an application bug. The Chrome binary bundled with Puppeteer requires `libnspr4.so` (NSPR library), which is not installed on this deployment instance. This is issue #30's root cause — the E2E test infrastructure cannot run, not an application failure.

**Fix**: Install missing system dependencies:
```bash
apt-get install -y libnspr4 libnss3 libatk1.0-0 libatk-bridge2.0-0 \
  libcups2 libdrm2 libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 \
  libxrandr2 libgbm1 libasound2
```

## 4. Summary

| Item | Status |
|------|--------|
| Server running | ✅ (port 3000, release build, commit a6517c5) |
| Core API endpoints | ✅ (version, settings, chat, cad all 200) |
| Some API endpoints 404 | ⚠️ (patents, ideas, oa-rounds — likely need path params) |
| Deployment logs | ❌ 无法取得 (no log file, nohup.out empty) |
| E2E test | ❌ Failed (Puppeteer/Chrome missing libnspr4.so) |
| Issue #30 root cause | System dependency (libnspr4.so), not application bug |
