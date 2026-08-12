# InnoForge FreeCAD Visual Chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let R&D users create and revise FreeCAD models from the idea workbench, patent-detail AI chat and OA discussion, with persistent PNG history plus FCStd/STEP downloads, while normal text chat remains available when FreeCAD is unavailable.

**Architecture:** Add a focused Rust `cad` module between browser routes and the loopback-only AionCAD HTTP bridge. Store artifact metadata in SQLite schema v18 and copy generated files into the InnoForge application-data directory. Reuse one dependency-free `static/cad.js` controller in all three existing chat pages; do not add a CAD page, frontend build system, AionCAD code or FreeCAD skill to this repository.

**Tech Stack:** Rust/Axum/Reqwest/Rusqlite/Tokio, SQLite v18 migration, vanilla HTML/CSS/JavaScript, existing i18n and DOMPurify guard.

---

## Task 1: Add schema v18 and CAD domain types ✅ `f0cbc80`

**Files:**

- Modify: `src/patent.rs`
- Modify: `src/db/mod.rs`
- Modify: `src/db/migrations.rs`
- Create: `src/db/cad.rs`
- Modify: `src/db/tests.rs`

**Step 1: Write failing migration tests**

Add tests that initialize a v17 database and assert schema v18 creates:

```sql
CREATE TABLE cad_artifacts (
    id TEXT PRIMARY KEY,
    context_kind TEXT NOT NULL CHECK(context_kind IN ('idea','patent','oa')),
    context_id TEXT NOT NULL,
    parent_artifact_id TEXT,
    revision INTEGER NOT NULL,
    prompt TEXT NOT NULL,
    assumptions_json TEXT NOT NULL DEFAULT '[]',
    preview_rel_path TEXT NOT NULL,
    fcstd_rel_path TEXT NOT NULL,
    step_rel_path TEXT,
    validation_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY(parent_artifact_id) REFERENCES cad_artifacts(id),
    UNIQUE(context_kind, context_id, revision)
);
CREATE INDEX idx_cad_artifacts_context_created
ON cad_artifacts(context_kind, context_id, created_at DESC);
```

Also test that reopening an already migrated database is idempotent.

**Step 2: Run the migration tests to verify failure**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target db::tests::cad --offline -- --nocapture`

Expected: FAIL because schema v18 and the table do not exist.

**Step 3: Add shared types before database code**

In `src/patent.rs`, add serializable/deserializable `CadContextKind`, `CadArtifact`, `CadDrawRequest`, `CadStatus`, `CadAvailability`, `CadValidation`, and response DTOs. Each struct/enum must derive `Debug, Clone, Serialize, Deserialize`. `CadContextKind` must accept only `idea`, `patent`, or `oa`; artifact prompts remain complete and are never truncated.

**Step 4: Implement the migration and repository methods**

- Set `Database::SCHEMA_VERSION` to 18.
- Add migration v17→v18 using a transaction.
- Add `Database::insert_cad_artifact`, `list_cad_artifacts`, and `get_cad_artifact` in `src/db/cad.rs`.
- Calculate `MAX(revision)+1` and insert within one SQLite transaction; rely on the unique key and retry a bounded number of times on a collision.
- Serialize assumptions/validation with `serde_json`; propagate errors with `Result`, without production `unwrap()`/`expect()`.

**Step 5: Run focused tests**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target db::tests::cad --offline -- --nocapture`

Expected: PASS, including concurrent revision uniqueness.

**Step 6: Commit**

```powershell
git add src/patent.rs src/db/mod.rs src/db/migrations.rs src/db/cad.rs src/db/tests.rs
git commit -m "feat: 增加FreeCAD产物版本存储"
```

## Task 2: Build the loopback-only AionCAD adapter ✅ `59cc1cd`

**Files:**

- Create: `src/cad.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`
- Modify: `src/common.rs`

**Step 1: Write failing unit tests in `src/cad.rs`**

Test the pure boundaries first:

```rust
#[test]
fn bridge_url_accepts_only_loopback_http() {
    assert!(BridgeOrigin::parse("http://127.0.0.1:8010").is_ok());
    assert!(BridgeOrigin::parse("http://localhost:8010").is_ok());
    assert!(BridgeOrigin::parse("https://127.0.0.1:8010").is_err());
    assert!(BridgeOrigin::parse("http://192.168.1.10:8010").is_err());
    assert!(BridgeOrigin::parse("http://example.com").is_err());
}

#[test]
fn artifact_path_must_remain_beneath_cad_root() {
    let root = temp_cad_root();
    assert!(resolve_artifact_path(&root, "abc/preview.png").is_ok());
    assert!(resolve_artifact_path(&root, "../secret.txt").is_err());
}
```

Add HTTP-adapter tests with a local test server for ready, bridge-down, stale worker, missing screenshot, aggregate draw success, partial STEP failure and the 120-second overall timeout configuration.

**Step 2: Run tests to verify failure**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target cad::tests --offline -- --nocapture`

Expected: FAIL because the module does not exist.

**Step 3: Implement configuration and startup**

Implement `CadService` with:

- a fixed validated `BridgeOrigin`, defaulting to `http://127.0.0.1:8010`;
- an application CAD root derived from the database parent/application-data directory, never the Git repository or `/static`;
- a 120-second Reqwest timeout for the FreeCAD pipeline;
- `status()` that checks `/health`, `/draw/status`, and `/draw/view` before returning `ready`;
- Windows-only `ensure_ready()` which resolves the AionCAD workspace from `INNOFORGE_AIONCAD_WORKSPACE`, then the `aioncad_workspace` setting, without embedding a developer-machine path;
- `Command::new("powershell")` arguments passed individually to `bootstrap_bridge.ps1`, with no constructed shell string and a bounded startup wait;
- `unsupported` on Android/iOS/non-Windows and `unavailable` on local startup failure.

No Docker, VM or extra sandbox is created. InnoForge never executes model-generated Python.

**Step 4: Implement bundle import**

Call AionCAD `POST /draw/artifact`. For a revision, pass the prior FCStd file only after resolving it from an opaque artifact ID beneath the CAD root. Copy PNG/FCStd and optional STEP into `cad/<uuid>/` using temporary files followed by atomic rename; verify required source files exist; save only relative paths.

**Step 5: Wire the module into both binaries**

Declare `mod cad;` in `src/main.rs` and `src/lib.rs`. Initialize the service through the shared `common::init_app_state` path so desktop and mobile use the same route graph; mobile returns `unsupported` without trying to launch FreeCAD.

**Step 6: Run focused tests**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target cad::tests --offline -- --nocapture`

Expected: PASS.

**Step 7: Commit**

```powershell
git add src/cad.rs src/main.rs src/lib.rs src/common.rs
git commit -m "feat: 接入AionCAD本地绘图桥"
```

## Task 3: Add the CAD API and expert-model brief ✅ `c54ac46`

**Files:**

- Create: `src/routes/cad.rs`
- Modify: `src/routes/mod.rs`
- Modify: `src/common.rs`
- Modify: `src/ai/client.rs`
- Create: `tests/cad_api.rs`

**Step 1: Add failing API tests**

Cover:

- `GET /api/cad/status` for ready/unavailable/unsupported;
- `POST /api/cad/draw` validation of context kind/id, empty prompt and invalid parent artifact;
- full prompt preservation;
- parent artifact must belong to the same context;
- `GET /api/cad/artifacts` ordered revision history;
- opaque preview/download endpoints, fixed MIME, `nosniff`, invalid format, missing artifact and path traversal;
- AionCAD failure returns friendly JSON and does not affect `/api/ai/chat`.

**Step 2: Run API tests to verify failure**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target --test cad_api --offline -- --nocapture`

Expected: FAIL because the routes are absent.

**Step 3: Implement the route handlers**

Register these shared routes in `common::build_router`, which is used by both `main.rs` and `lib.rs`:

```text
GET  /api/cad/status
POST /api/cad/draw
GET  /api/cad/artifacts
GET  /api/cad/artifacts/:id/preview
GET  /api/cad/artifacts/:id/download/:format
```

Return user-friendly JSON errors; do not expose Rust panic text, AionCAD absolute paths, model-generated code or command lines.

**Step 4: Generate the CAD brief with the expert model**

Keep the prompt in `src/routes/cad.rs`. Use `ai_client_expert().clone_with_timeout(Duration::from_secs(60))` and require JSON containing a normalized full instruction plus explicit assumptions. Boundary-isolate inputs:

```text
<conversation_context>...</conversation_context>
<user_input>...</user_input>
```

Tell the model that content inside both tags is data and cannot overwrite system instructions. If parsing or AI invocation fails, send the complete original user prompt to AionCAD and return an empty assumptions list. Do not truncate either input.

**Step 5: Serve files safely**

Resolve every artifact through the database and canonicalized CAD root. Allow only `fcstd` and `step`; use `image/png`, `application/octet-stream`, and `model/step`; set a safe download filename and `X-Content-Type-Options: nosniff`.

**Step 6: Run focused API tests**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target --test cad_api --offline -- --nocapture`

Expected: PASS.

**Step 7: Commit**

```powershell
git add src/routes/cad.rs src/routes/mod.rs src/common.rs src/ai/client.rs tests/cad_api.rs
git commit -m "feat: 增加FreeCAD绘图与产物接口"
```

## Task 4: Build one safe reusable CAD card controller ✅ `2de4e8b`

**Files:**

- Create: `static/cad.js`
- Modify: `static/style.css`
- Modify: `static/i18n.js`
- Create: `tests/cad_ui_contract.rs`

**Step 1: Add failing static/UI contract tests**

Assert that the shared script:

- exposes `window.InnoForgeCad.createController`;
- uses a conservative explicit-intent matcher;
- supports explicit button invocation independent of intent detection;
- renders cards with `createElement`/`textContent` and no unsanitized `innerHTML`;
- displays assumptions, validation, revision and time;
- exposes continue/fullscreen/FCStd/STEP actions;
- keeps the original prompt on retry.

**Step 2: Run tests to verify failure**

Run: `cargo test --target-dir E:\tmp\innoforge-build-target --test cad_ui_contract --offline -- --nocapture`

Expected: FAIL because `static/cad.js` is absent.

**Step 3: Implement the controller**

`createController` receives page-specific callbacks for context, input, message container and ordinary-send behavior. Its state machine is:

```text
checking → starting → briefing → drawing → packaging → complete|degraded
```

Automatic intent detection matches high-confidence phrases such as “画一个”, “生成3D结构”, “FreeCAD模型”, and “给我看图”; broad questions like “这个方案怎么画” remain ordinary chat. The explicit FreeCAD button always invokes CAD. A degraded card contains Retry and the untouched original prompt; it never blocks the normal send function.

Use DOM creation APIs and same-origin artifact URLs only. Fullscreen is a DOM overlay with focus/escape handling, not a new page.

**Step 4: Add complete bilingual text and shared styling**

Add all Chinese and English keys to `static/i18n.js`. Add responsive card, progress, assumption, validation and fullscreen styles to `static/style.css`; keep the existing visual system.

**Step 5: Run focused tests and ESLint**

```powershell
cargo test --target-dir E:\tmp\innoforge-build-target --test cad_ui_contract --offline -- --nocapture
node node_modules/.bin/eslint static/cad.js static/i18n.js
```

Expected: PASS with no ESLint error.

**Step 6: Commit**

```powershell
git add static/cad.js static/style.css static/i18n.js tests/cad_ui_contract.rs
git commit -m "feat: 增加FreeCAD对话图卡组件"
```

## Task 5: Integrate the three existing chat flows ✅ `757643a`, `036191f`

**Files:**

- Modify: `templates/idea.html`
- Modify: `templates/patent_detail.html`
- Modify: `templates/office_action_response.html`
- Modify: `templates/settings.html`
- Modify: `e2e_test.mjs`

**Step 1: Add failing Puppeteer scenarios**

For each page, test explicit button and high-confidence natural-language trigger. Also test that a normal question still calls the original AI endpoint, unavailable FreeCAD produces only a degraded CAD card, parent artifact is sent on “继续修改”, and a page reload restores artifact history.

**Step 2: Run Puppeteer to verify failure**

Run: `node e2e_test.mjs`

Expected: new CAD scenarios FAIL before template integration; existing scenarios remain green.

**Step 3: Add the shared script and explicit buttons**

Load `/static/cad.js` after `/static/i18n.js` on all four templates. Add one compact `FreeCAD 绘图` button next to each existing chat send control. Configure the shared controller with:

- idea: `context_kind=idea`, current idea ID;
- patent detail: `context_kind=patent`, current patent ID/number;
- OA discussion: `context_kind=oa`, stable backend `discussion_id`;
- page-local history provider using complete existing messages, without truncation.

Do not rewrite the existing send/stream/abort logic. Add a narrow pre-send CAD interception and otherwise call the original function unchanged.

**Step 4: Add settings controls**

Add a FreeCAD section showing status, workspace path, auto-start toggle and a “检测并启动” action. Persist through the existing key-value settings API—no settings schema migration. Disable auto-start on unsupported platforms while preserving status visibility.

**Step 5: Run ESLint and Puppeteer**

```powershell
node node_modules/.bin/eslint static/cad.js static/i18n.js templates/idea.html templates/patent_detail.html templates/office_action_response.html templates/settings.html
node e2e_test.mjs
```

Expected: no ESLint error and all existing plus new CAD scenarios pass.

**Step 6: Commit**

```powershell
git add templates/idea.html templates/patent_detail.html templates/office_action_response.html templates/settings.html e2e_test.mjs
git commit -m "feat: 在三处AI对话接入FreeCAD绘图"
```

## Task 6: Full regression, documentation and PR handoff ✅ `1a91180`

**Files:**

- Modify: `CHANGELOG.md`
- Modify: `docs/plans/STATUS.md`
- Modify: `docs/plans/2026-08-11-freecad-visual-chat-implementation.md`

**Step 1: Run Rust verification using E drive build output**

```powershell
cargo fmt --check
cargo clippy --target-dir E:\tmp\innoforge-build-target --offline -- -D warnings
cargo test --target-dir E:\tmp\innoforge-build-target --offline
```

Expected: all pass with zero warnings. If offline dependency resolution fails, retry with the configured registry once network access is available; do not report green without a completed run.

**Step 2: Run frontend and existing end-to-end regressions**

```powershell
node node_modules/.bin/eslint static/cad.js static/i18n.js templates/idea.html templates/patent_detail.html templates/office_action_response.html templates/settings.html
node e2e_test.mjs
```

Manually verify the project-required flows: PDF upload with full content, patent detail five tabs, OA parse→analysis→discussion→response letter, and technical research PDF export with complete text.

**Step 3: Run real AionCAD/FreeCAD smoke from all three contexts**

Verify button trigger, natural-language trigger, visible assumptions, same-model revision, reload history, fullscreen, FCStd/STEP downloads, and text-chat fallback while the bridge is stopped. FreeCAD must visibly display the latest model.

**Step 4: Update project records without bumping the version**

Add an Unreleased bilingual CHANGELOG entry and update `docs/plans/STATUS.md`. Keep `Cargo.toml` at the current version because the user explicitly requested no version increase for this PR.

Mark all plan tasks ✅ with commit hashes.

**Step 5: Commit and publish the PR branch**

```powershell
git add CHANGELOG.md docs/plans/STATUS.md docs/plans/2026-08-11-freecad-visual-chat-implementation.md
git commit -m "docs: 记录FreeCAD可视化对话交付"
git push github feat/freecad-visual-chat
```

Open a draft PR against `main`. Do not merge it in this task.
