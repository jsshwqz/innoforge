# InnoForge 错误复盘数据库 / Error Retrospective Database

> 所有 Agent 和贡献者在 **修完 Bug 后必须在此追加一条记录**。
> 格式见下方模板。目的是防止同类错误反复出现。

---

## 条目格式 / Entry Template

```markdown
## [YYYY-MM-DD] 错误简述
- **严重程度**: CRITICAL / HIGH / MEDIUM / LOW
- **涉及文件**: 文件路径:行号
- **现象**: 用户看到什么 / 出了什么问题
- **根因**: 为什么发生
- **修复**: 怎么修的
- **预防**: 下次怎么避免
- **提交**: commit hash
```

---

## 2026-05 — 全面审计发现的 18 个 Bug

以下条目来自 2026-05-21 的全面代码审计，修复集中在 3 个提交中。

### [2026-05-21] C1: Google Client Secret 掩码处理缺失
- **严重程度**: CRITICAL
- **涉及文件**: `src/routes/settings.rs`
- **现象**: 前端保存设置时若传回 Google Client Secret 的掩码值（`****`），后端会将其直接覆盖保存为 `"****"`，导致 OAuth 认证失败。与已修复的 AI API Key 掩码逻辑不一致。
- **根因**: 只对 `ai_api_key` 做了掩码检测（`contains("****")`），遗漏了 `google_client_secret` 的相同处理逻辑。
- **修复**: 给 `google_client_secret` 增加同样的 `contains("****")` 检测 + 域名变更拦截逻辑。
- **预防**: 所有凭据字段的保存 handler 必须统一处理掩码值。新增凭据字段时参考现有掩码处理模式。
- **提交**: `ab6695e`

### [2026-05-21] C2: SerpAPI 多 Key 前端回传后丢失
- **严重程度**: CRITICAL
- **涉及文件**: `src/routes/settings.rs`
- **现象**: 前端 GET 拿到掩码后的 Key 列表（`sk****abcd`），PUT 回后端时因掩码格式与校验规则（需 ≥20 字符且仅含特定字符）冲突，导致所有 Key 被跳过，实际保存为空列表。
- **根因**: SerpAPI 保存 handler 缺少掩码反查逻辑，未将掩码值还原为原始 Key。
- **修复**: 读取当前内存中的真实 Key 列表，对前端传来的掩码值做前缀/后缀匹配（`split("****")` → `starts_with` + `ends_with`），匹配到则使用原始 Key。
- **预防**: 前端传回掩码值的场景必须做反查映射。所有凭据保存 handler 统一处理掩码 → 原值转换。
- **提交**: `ab6695e`

### [2026-05-21] C3: Token 过期默认值错误
- **严重程度**: CRITICAL
- **涉及文件**: `src/routes/mod.rs`
- **现象**: 数据库无 `google_token_expiry` 记录时，`unwrap_or(false)` 将过期判断默认设为"未过期"，导致使用过期令牌请求 AI API，返回 401/403。
- **根因**: `google_token_expiry` 是 Option，读取 DB 时为 `None` 时 `unwrap_or(false)` 意味着"没有过期时间 = 永不过期"，但实际应是"没有过期时间 = 已过期"。
- **修复**: `unwrap_or(false)` → `unwrap_or(true)`，默认认为令牌已过期，触发刷新流程。
- **预防**: Bool 类型 unwrap_or 的默认值必须仔细推敲：名字是"expired"时，`None` 应默认为 `true`（已过期安全）。
- **提交**: `38f8bf0`

### [2026-05-21] H1: Google Token Expiry 缺少环境变量后备
- **严重程度**: HIGH
- **涉及文件**: `src/routes/mod.rs`
- **现象**: `google_token_expiry` 仅从 DB 读取，未从 `.env` 文件读取。当 .env 中有过期时间但 DB 中没有时（例如从旧版本升级），令牌不会被刷新。
- **根因**: DB 配置优先策略未覆盖 `google_token_expiry` 字段，读取 DB 后没有 `or_else` 回退到 `std::env::var`。
- **修复**: 在 `google_token_expiry` 的读取链中增加 `.or_else(|| std::env::var("GOOGLE_TOKEN_EXPIRY").ok().and_then(|v| v.parse::<i64>().ok()))`。
- **预防**: 所有配置项加载时统一做 DB → env var 链式回退，新增配置项时参照已有模式。
- **提交**: `38f8bf0`

### [2026-05-21] H2: 切换非 Google 服务商后 OAuth 令牌残留
- **严重程度**: HIGH
- **涉及文件**: `src/routes/settings.rs`
- **现象**: 用户从 Google Gemini 切换到 DeepSeek 后，内存中仍保留 Google OAuth 的 access_token/refresh_token，导致 `effective_api_key()` 仍尝试使用 OAuth 认证。
- **根因**: 切换 AI 服务商时未清除旧服务商的 OAuth 凭据。
- **修复**: `api_save_ai` 中当 `base_url` 不含 `"googleapis"` 时，清除 `google_access_token`、`google_token_expiry`、`google_refresh_token`、`google_auth_mode`。
- **预防**: 切换服务商时必须清理上一个服务商的会话凭据。类似逻辑应推广到所有凭据切换场景。
- **提交**: `38f8bf0`

### [2026-05-21] H3: auth.rs 大量死代码
- **严重程度**: HIGH
- **涉及文件**: `src/routes/auth.rs`
- **现象**: `impl AppState` 中包含约 150 行未使用的函数（`ensure_google_token()`、`refresh_gcloud_token()`、`refresh_oauth_token()`），带有 `#[allow(dead_code)]` 抑制警告。
- **根因**: 重构后将 token 刷新逻辑移到了 `mod.rs`，但旧代码未删除。
- **修复**: 删除整个 `impl AppState` 块中所有未使用的函数。
- **预防**: 重构后执行全量搜索，确认旧函数不再被引用后删除。避免使用 `#[allow(dead_code)]` 掩盖死代码。
- **提交**: `38f8bf0`

### [2026-05-21] H4: 阻塞的 std::process::Command 未包装在 spawn_blocking 中
- **严重程度**: HIGH
- **涉及文件**: `src/routes/auth.rs`
- **现象**: `get_token_from_gcloud_cli()` 调用 `std::process::Command::output()` 是同步阻塞调用，在 async handler 中直接调用会阻塞 tokio 工作线程。
- **根因**: 不熟悉 async Rust 的阻塞/非阻塞边界。
- **修复**: 将 `get_token_from_gcloud_cli` 用 `tokio::task::spawn_blocking` 包装。同时 clippy 指出 `spawn_blocking(|| get_token_from_gcloud_cli())` 应简化为 `spawn_blocking(get_token_from_gcloud_cli)`。
- **预防**: 任何 `std::process::Command`、`std::thread::sleep`、同步 IO 调用在 async 上下文中必须用 `spawn_blocking` 包装。
- **提交**: `69980ab`

### [2026-05-21] H5: 生产路径使用 unwrap()
- **严重程度**: HIGH
- **涉及文件**: `src/routes/ai.rs`、`src/routes/idea.rs`
- **现象**: `src/routes/ai.rs` 中对 `req["patents"]` 直接调用 `.unwrap()`，`src/routes/idea.rs` 中对 `query_map` 结果调用 `.unwrap()`。当请求缺少字段时会导致 panic，返回 500。
- **根因**: 违反 CLAUDE.md 2.7 节"生产路径禁止 unwrap()"的规定。
- **修复**: ai.rs: 改用 `match req["query"].as_str()` 和 `match req["patents"].as_array()` 并在缺失时返回友好错误。idea.rs: `query_map` 改用 `match conn.prepare(...)`，`as_object_mut().unwrap()` 改用 `if let Some(obj) = c.as_object_mut()`。
- **预防**: 所有 API handler 必须使用 `Result` + `?` 传播错误或返回 JSON 错误响应。用 grep 搜索 `.unwrap()` 发现遗漏。
- **提交**: `38f8bf0`

### [2026-05-21] H6: lib.rs 缺少 reset_stale_analyzing() 启动调用
- **严重程度**: HIGH
- **涉及文件**: `src/lib.rs`
- **现象**: `lib.rs`（Android/iOS FFI 入口）在 Database::init() 后未调用 `reset_stale_analyzing()`，导致移动端重启后上次中断的创意永久卡在 `analyzing` 状态。
- **根因**: `main.rs` 有该调用但 `lib.rs` 遗漏，两个入口不同步。
- **修复**: 在 `lib.rs` 的 `Database::init()` 后增加相同的 `reset_stale_analyzing()` 调用。
- **预防**: `main.rs` 和 `lib.rs` 的初始化逻辑必须保持一致。修改启动流程时同时检查两处。
- **提交**: `38f8bf0`

### [2026-05-21] H7: API Key 长度检查在掩码检测之前
- **严重程度**: HIGH
- **涉及文件**: `src/routes/settings.rs`
- **现象**: 前端传回掩码值 `"sk****abcd"`（8 字符）时，先执行 `api_key.len() < 8` 检查，返回"长度不合法"错误。
- **根因**: 验证顺序错误：应先检测掩码（跳过格式校验），再检查普通 Key 的长度。
- **修复**: 在长度检查前增加 `!api_key.contains("****")` 条件，使掩码值跳过格式验证。
- **预防**: 所有凭据保存 handler 中，掩码检测必须优先于格式/长度校验。
- **提交**: `ab6695e`

### [2026-05-21] M1: update_env_file() 非原子写入
- **严重程度**: MEDIUM
- **涉及文件**: `src/routes/settings.rs`
- **现象**: `update_env_file` 直接 `std::fs::write` 到 `.env` 文件，多线程并发保存设置时存在数据丢失风险。
- **根因**: 缺乏原子写入模式。
- **修复**: 改为先写临时文件 `.env.tmp`，再 `std::fs::rename` 覆盖原文件（rename 在 Windows 和 Linux 上都是原子操作）。
- **预防**: 文件写入操作优先用"写临时文件 + rename"的原子模式。
- **提交**: `38f8bf0`（settings.rs 改动）

### [2026-05-21] M2: api_serpapi_balance() 无超时
- **严重程度**: MEDIUM
- **涉及文件**: `src/routes/settings.rs`
- **现象**: `api_serpapi_balance` 使用 `std::thread::spawn` + `mpsc::channel`，但 `rx.recv()` 无超时，当 SerpAPI 无响应时请求会无限挂起。
- **根因**: 线程通信缺少超时机制。
- **修复**: `rx.recv()` → `rx.recv_timeout(Duration::from_secs(30))`，超时返回友好错误。
- **预防**: 所有跨线程通信、网络请求必须有超时机制。
- **提交**: `38f8bf0`

### [2026-05-21] M3: extract_domain() 对 IP 地址解析错误
- **严重程度**: MEDIUM
- **涉及文件**: `src/routes/settings.rs`
- **现象**: 当 AI base_url 使用 IP 地址（如 `http://192.168.1.1:11434`）时，`extract_domain` 返回 `"168.1"`（取倒数两段），导致域名变更检测逻辑出错。
- **根因**: `split('.').collect()` 后固定取 `parts[n-2].parts[n-1]`，未考虑 IP 地址格式。
- **修复**: 在域名分割前检测是否为 IP（所有字符是数字或 `.`），如果是则返回完整 IP。
- **预防**: URL 解析函数必须考虑 IP 地址、localhost 等特殊格式。
- **提交**: `ab6695e`

### [2026-05-21] M4: AI 客户端连接 localhost 失败时提示不清晰
- **严重程度**: MEDIUM
- **涉及文件**: `src/ai/client.rs`
- **现象**: AI 未配置时（base_url 指向 localhost 但无服务监听），错误信息是底层网络错误（"Connection refused"），用户无法理解。
- **根因**: 缺少对 localhost 连接失败的友好提示逻辑。
- **修复**: 在 `try_provider_with_body()` 中，当 `e.is_connect()` 且 `provider.base_url.contains("localhost")` 时，返回中文友好提示"AI 未配置。请打开设置页面配置云端 AI 服务"。
- **预防**: 所有网络连接失败场景应考虑用户体验，用友好信息替代底层错误。
- **提交**: `69980ab`

### [2026-05-24 16:30] OA 页面 DOMPurify 缺失导致 JS 崩溃全黑
- **严重程度**: CRITICAL
- **涉及文件**: `templates/office_action_response.html`
- **现象**: OA 页面打开后全黑，无文字、无法操作。控制台报 `ReferenceError: DOMPurify is not defined`。
- **根因**: 新增 OA 答复功能时在 JS 中使用了 `DOMPurify.sanitize()`，但页面 `<head>` 中没有加载 DOMPurify CDN。其他页面（ai.html、idea.html）都有，OA 页面是新创建的，遗漏了。一旦 JS 抛出 ReferenceError，整个 script 块中断执行，`applyI18nCommon()` 无法运行，所有 data-i18n 元素为空，页面呈现全黑。
- **修复**: 在 `<head>` 中加入 `<script src="https://cdnjs.cloudflare.com/ajax/libs/dompurify/3.0.6/purify.min.js">`，并加 CDN 加载失败的降级保护。
- **预防**: 新建页面时必须检查是否依赖 DOMPurify。创建新 HTML 页面时参考现有页面（ai.html）的 head 部分，确保所有依赖一致。JS 中引用外部库前先做 `typeof` 检查。
- **提交**: `1646f14`
- **严重程度**: MEDIUM
- **涉及文件**: `src/ai/client.rs`
- **现象**: 某些 AI 服务商返回 HTTP 200 但响应中包含 `error` 对象（如配额超限），`ChatResponse` 结构体未解析该字段，导致逻辑认为成功但实际无内容。
- **根因**: ChatResponse 定于缺少 `error` 字段，`choices` 为空时直接返回错误而非检查 error 对象的具体内容。
- **修复**: 在 `ChatResponse` 中增加 `error: Option<Value>` 字段，`extract_chat_content()` 中当 choices 为空时检查 resp.error。
- **预防**: API 响应结构体应尽可能覆盖所有可能的返回字段，特别是错误场景。
- **提交**: `69980ab`

### [2026-05-21] L1: src/lib.rs 中 Response::builder().unwrap()
- **严重程度**: LOW
- **涉及文件**: `src/lib.rs`
- **现象**: `serve_static()` 中 `Response::builder().body(...)` 后直接 `.unwrap()`，理论上可能 panic（极低概率：body 太大超过 HTTP 限制）。
- **根因**: 违反 CLAUDE.md 2.7 生产路径禁止 unwrap。
- **修复**: 改为 `match Response::builder()...` → `Ok(resp) => resp, Err(_) => Response::new(Body::from(...))`。
- **预防**: 全仓 grep `\.unwrap()` 检查生产路径。
- **提交**: `38f8bf0`

### [2026-05-21] L2: src/main.rs 中 Response::builder().unwrap()
- **严重程度**: LOW
- **涉及文件**: `src/main.rs`
- **现象**: 同 L1，`serve_static()` 中相同模式。
- **根因**: 同 L1。
- **修复**: 同 L1。
- **预防**: 同 L1。
- **提交**: `38f8bf0`

---

## v0.5.x 发布质量事故（2026-04-18）

以下条目来自 `docs/release-incident-v0.5x.md` 复盘。

### [2026-04-18] 发布资产不完整 — GitHub Release 缺少平台包
- **严重程度**: CRITICAL
- **涉及文件**: （发布流程）`.github/workflows/release.yml`
- **现象**: v0.5.1 ~ v0.5.4 的 GitHub Release 资产数为 0，用户无法下载对应版本的可执行文件。
- **根因**: 发布流程仅验证代码通过测试，未做发布资产核验。`release.yml` 矩阵构建的任务未正确产出并上传资产。
- **修复**: 新增 `docs/release-gate-manual-e2e.md` 全清单检查。发布后必须核对 GitHub Release 资产至少包含 5 个平台包。
- **预防**: 发布前执行 release-gate 清单；发布后做双端（GitHub/Gitee）资产一致性复查。
- **提交**: 流程修复，未对应单一代码提交。

### [2026-04-18] v0.5.4 提交范围与预期差距大
- **严重程度**: HIGH
- **涉及文件**: （发布管理）
- **现象**: v0.5.4 标签提交 `346e426` 仅改动 4 个文件（CHANGELOG.md、Cargo.toml、README.md、src/lib.rs），与"多终端完整可用"用户预期明显不符。
- **根因**: 标签打在分支的中间状态而非完整功能点。缺少"发布前确认提交范围"的门禁。
- **修复**: 发布前逐项核对版本计划中的所有任务是否已完成且包含在标签中。
- **预防**: 发布打标签前 diff 检查变更范围是否符合版本规划。
- **提交**: 流程修复。

### [2026-04-18] 测试通过不等于端到端体验达标
- **严重程度**: MEDIUM
- **涉及文件**: （测试策略）
- **现象**: v0.5.0 和 v0.5.3 的 `cargo test` 均通过，但用户报告本地体验严重退化。
- **根因**: 单元/集成测试覆盖的是代码逻辑，不覆盖页面功能与交互流程。缺少端到端（E2E）测试。
- **修复**: 新增 `docs/release-gate-manual-e2e.md` 手工 E2E 测试清单。
- **预防**: 发布前除了 `cargo test`，必须执行 E2E 清单测试。考虑未来引入自动化 E2E 测试。
- **提交**: 流程修复。

---

## v0.5.4 发布交付复盘（2026-04-18）

以下条目来自 `docs/release-retrospective-v0.5.4.md` 复盘。

### [2026-04-18] 三端远端仓库不存在导致发布失败
- **严重程度**: HIGH
- **涉及文件**: （发布流程）
- **现象**: desktop/ios/harmony 三端远程仓库在 GitHub/Gitee 上未创建，git push 返回 `404 not found`。
- **根因**: 三端仓仅完成了本地 git init，未在远端创建仓库。
- **修复**: 通过 GitHub/Gitee API 手动创建三端远端仓库，添加为 remote，推送代码。
- **预防**: 发布前先执行 `git ls-remote <remote> refs/heads/main` 确认远端存在。
- **提交**: 流程修复。

### [2026-04-18] gh.exe 在 Windows 平台不可执行
- **严重程度**: MEDIUM
- **涉及文件**: （工具链）
- **现象**: 本机 `gh.exe` 是 Linux 二进制（WSL 残留），在 Windows 下无法执行，导致 GitHub CLI 方案失败。
- **根因**: 工具链缺乏平台兼容性检查。
- **修复**: 降级为直接使用 GitHub API + `git push` 流程。
- **预防**: 准备工具链降级预案：当 `gh` 不可用时，自动切换到 API + git 推送流程。
- **提交**: 流程修复。

### [2026-04-18] 三端仓缺少 GitHub 远端
- **严重程度**: MEDIUM
- **涉及文件**: （版本控制）
- **现象**: desktop/ios/harmony 三端本地仓仅配置了 Gitee 远端（`origin`），缺少 GitHub 远端，不满足双端发布规则。
- **根因**: 初始搭建时只配置了 Gitee，未同时配置 GitHub。
- **修复**: 为三端仓添加 GitHub 远端（命名为 `github`），推送并确认 commit SHA 一致。
- **预防**: 新建仓库时必须同时配置双远端。在初始化脚本中强制检查。
- **提交**: 流程修复。

---

## 踩过的坑（历史累积）

以下条目来自 `docs/handover-briefing.md` 及其他历史复盘文档。

### [2026-04-01] Windows BAT 脚本中文乱码
- **严重程度**: MEDIUM
- **涉及文件**: `start-innoforge.bat`
- **现象**: Windows 下执行 BAT 脚本时中文显示为乱码。
- **根因**: Windows 终端默认使用 GBK 编码，BAT 脚本中使用的中文字符为 UTF-8。
- **修复**: 改用英文 label 替代中文。
- **预防**: Windows BAT 脚本避免使用中文字符。
- **提交**: （较早提交）

### [2026-04-01] AI 分析全部失败 — 凭据无效或限速
- **严重程度**: HIGH
- **涉及文件**: `src/ai/client.rs`
- **现象**: 用户报告所有 AI 分析功能不可用，页面显示错误。
- **根因**: API Key 无效（401 Unauthorized）或超出免费配额（429 Rate Limited）。
- **修复**: 已改进错误提示，引导用户去设置页检查/切换 AI 服务商。
- **预防**: 设置页增加 AI 连接测试按钮，让用户能主动验证配置（待实现）。所有凭据失效场景给出明确指引。
- **提交**: 多次迭代修复。

### [2026-04-01] cargo build access denied — 进程锁住二进制
- **严重程度**: LOW
- **涉及文件**: （构建环境）

---

## [2026-07-04] OA 流式输出乱码 `*` / `D` / `★`
- **严重程度**: HIGH
- **涉及文件**: `src/ai/chat.rs` `static/purify.min.js`
- **现象**: OA 分析结果中文字全变为 `*` 字符，表格内容变为 `D`/`A`/`★`/`—` 单字符缩写。AI 讨论回复全是 `*`。
- **根因**: 双重原因——
  1. DeepSeek API 请求缺 `max_tokens` 参数，模型默认输出 token 过少（约 4K），生成到一半被截断。
  2. `static/purify.min.js` 文件损坏（18KB vs 正确 21KB），导致 `Uncaught SyntaxError`，`DOMPurify.sanitize()` 不可用。虽 i18n.js 有回退守卫，但部分 `innerHTML` 赋值行为异常。
- **修复**: 
  1. `src/ai/chat.rs` → OpenAI 兼容流式请求添加 `"max_tokens": 16384`
  2. `static/purify.min.js` → 从 CDN 重新下载完整版（20931 字节）
  3. 服务已重启（PID 4860）
- **未确认**: 修复后 API 层（curl 测试）返回正确中文，但用户浏览器仍显示 `*`。需排查：
  - 浏览器是否缓存旧页面 HTML（编译嵌入的模板）
  - 前端 `renderMarkdown()` 函数是否有未发现的字符转换
  - SSE 流式拼接时是否有数据丢失
  - `parseOASections()` 切分逻辑是否误切
- **预防**: 每次变更模板文件后用 Puppeteer 全量 e2e 测试
- **提交**: dev 分支 `7fc1aec` + 本地未提交的 purify.min.js 替换
- **现象**: `cargo build` 失败，提示"拒绝访问"（Access Denied）。
- **根因**: `innoforge.exe` 正在运行中，Windows 锁定文件导致编译无法覆写。
- **修复**: 执行 `taskkill //F //IM innoforge.exe` 后重新编译。
- **预防**: 构建前检查旧进程是否运行。在开发脚本中自动 kill 旧进程。
- **提交**: 流程改进。

### [2026-04-01] GitHub Release 重复 Changelog
- **严重程度**: MEDIUM
- **涉及文件**: `.github/workflows/release.yml`
- **现象**: 每次 Release 的 Release Notes 中 Changelog 重复出现多次。
- **根因**: `release.yml` 中 `generate_release_notes: true` 在 5 个矩阵构建任务中各追加一次 Release Notes。
- **修复**: 将 Release Notes 生成移出矩阵构建循环，改为只生成一次。
- **提交**: `.github/workflows/release.yml` 的修复提交。

### [2026-04-01] 设置页面下拉框不回显
- **严重程度**: MEDIUM
- **涉及文件**: `templates/settings.html`、`static/js/settings.js`
- **现象**: 刷新设置页面后，AI 模型下拉框显示为空（默认值），而非已保存的值。
- **根因**: 页面加载时 `updateAiFields()` 在所有设置项加载前执行，覆盖了已保存的值。
- **修复**: 调整初始化顺序，确保设置值加载完成后再调用 `updateAiFields()`。
- **预防**: 页面初始化逻辑必须保证数据加载 → UI 渲染的先后顺序。避免异步竞态。
- **提交**: （较早提交）

### [2026-04-01] git push 用错 remote 名
- **严重程度**: MEDIUM
- **涉及文件**: （版本控制）
- **现象**: 提交推送到错误的目标仓库。
- **根因**: 仓库有两个 remote：`origin`（GitHub）和 `gitee`（Gitee）。开发者误用 remote 名称。
- **修复**: 明确文档化 remote 命名约定。
- **预防**: 提交前 `git remote -v` 确认当前 remote。推送时显式指定目标 remote。
- **提交**: 流程改进。

### [2026-04-18] Gemini URL 双斜杠导致 404
- **严重程度**: MEDIUM
- **涉及文件**: `src/ai/client.rs`
- **现象**: Gemini API 请求因 URL 中存在双斜杠（`/openai//chat/completions`）返回 404。
- **根因**: AI client URL 拼接时未 trim base_url 的尾斜杠。
- **修复**: URL 拼接前 trim 尾部 `/`。
- **预防**: URL 拼接代码必须处理斜杠统一（确保 base_url 和 path 之间只有一个斜杠）。
- **提交**: `fefdcd6`

---

## 书写规范

1. **每修一个 Bug 追加一条**，不要等积累多了再写
2. 严重程度从用户视角判断：用户数据损失/功能完全不可用 → CRITICAL
3. "预防"字段是最重要的——它是未来 Agent 的规约补充
4. 提交 hash 用前 7 位即可

---

### [2026-06-22] 系统代理拦截 DeepSeek API 请求
- **严重程度**: CRITICAL
- **涉及文件**: `src/ai/client.rs`
- **现象**: OA 分析功能调用 DeepSeek API 时返回 "AI 响应读取失败（连接中断）"，浏览器显示 "分析失败: Failed to fetch"。服务器日志显示请求通过 `proxy(http://127.0.0.1:10808)` 发送，45 秒后超时取消。
- **根因**: 
  1. Windows 系统环境变量 `HTTP_PROXY=http://127.0.0.1:10808` 被 reqwest 库自动读取
  2. 本地代理工具（Clash/v2ray）拦截了 DeepSeek 的 HTTPS 请求，但未正确转发
  3. 请求建立连接后等待响应，代理无响应导致 45 秒超时，重试 3 次后失败
- **修复**: 在 `Client::builder()` 链中增加 `.no_proxy()`，强制 reqwest 忽略系统代理设置
- **预防**: 
  1. 所有 HTTP 客户端创建处应显式控制代理行为，不依赖系统环境变量
  2. 生产路径增加 `.no_proxy()` 确保不受代理干扰
  3. 新增网络请求相关代码时检查是否有代理干扰
- **提交**: `c250f50`

### [2026-06-22] OA 页面 DOMPurify CDN 遗漏
- **严重程度**: MEDIUM
- **涉及文件**: `templates/office_action_response.html`
- **现象**: 用户打开 OA 页面，点击分析后提示 "分析失败: Failed to fetch"。实际原因是 DOMPurify 未加载导致 JS 崩溃，后续所有 fetch 调用未执行。
- **根因**: v0.6.2 的 DOMPurify CDN→本地迁移覆盖了 6 个模板但遗漏了 `office_action_response.html`。该页面仍引用 CDN（`cdnjs.cloudflare.com`），网络差时加载失败 → JS 停止执行。
- **修复**: 将 CDN URL 替换为 `/static/purify.min.js`，移除 CDN 降级回退脚本块
- **预防**: 
  1. 批量替换时用 grep 确认所有文件已覆盖（`grep -r "cdnjs" templates/`）
  2. 新增模板时检查前端依赖引用方式
- **提交**: `c250f50`

### [2026-06-22] AI HTTP 超时 45s 不足以处理 OA 长 prompt
- **严重程度**: HIGH
- **涉及文件**: `src/ai/client.rs`
- **现象**: DeepSeek v4-pro 推理模型处理 OA 分析 prompt（约 3000 tokens 输入）需要 40-90 秒，但 HTTP 客户端超时为 45 秒，导致连接在第 45 秒被强制取消（CANCEL frame），重试 3 次后仍失败。
- **根因**: `PROVIDER_HTTP_TIMEOUT_SECS = 45` 是针对普通对话场景设定的，未考虑推理模型处理长 prompt 的额外耗时。
- **修复**: 超时从 45 秒提升至 180 秒（与全局 `GLOBAL_TIMEOUT_SECS` 保持一致）
- **预防**: 
  1. 推理模型（deepseek-reasoner 等）的 HTTP 超时应显著高于普通模型
  2. 新增 AI 功能时根据最大预期响应时间调整超时
  3. 考虑用流式响应（SSE）替代等待完整响应
- **提交**: `c250f50`

### [2026-06-22] start.bat 因 cargo 不在系统 PATH 中无法启动
- **严重程度**: MEDIUM
- **涉及文件**: `start.bat`、`dev.bat`
- **现象**: 用户双击 `start.bat`，cmd 窗口一闪而过，服务器未启动。浏览器访问 `http://127.0.0.1:3000` 显示 ERR_CONNECTION_REFUSED。
- **根因**: `start.bat` 执行 `cargo build --release --bin innoforge`，但 `cargo` 仅存在于 Git Bash 的 PATH（`C:\Users\Administrator\.cargo\bin`），不在 Windows 系统环境变量 PATH 中。从 Windows Explorer 双击 .bat 文件时由 cmd.exe 执行，找不到 cargo 命令。
- **修复**: 
  1. 在 `start.bat` 和 `dev.bat` 开头加入 `set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"`
  2. 优化 `start.bat`：已有 release 二进制时跳过编译（从 3-4 分钟减为 0 秒）
  3. 去掉末尾 `pause >nul` 让错误可见
- **预防**: 
  1. .bat 文件应自行处理 PATH 依赖，不假设系统环境变量
  2. 新增脚本文件时加上 `set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"`
- **提交**: `6b713f7` / `20de7e9`

### [2026-05-25] DOMPurify CDN 依赖导致断网全黑 — 彻底修复
- **严重程度**: MEDIUM
- **涉及文件**: `static/purify.min.js`、`templates/*.html`（6文件）、`start.bat`
- **现象**: 上轮修复仅添加了 CDN 引用 + 降级保护。但 CDN 在中国网络/离线环境下仍可能加载失败，且旧版 `start.bat` 不重新编译导致二进制与源码不一致，用户看到空白页后误以为"闪退"。
- **根因**: 
  1. DOMPurify 依赖 CDN（`cdnjs.cloudflare.com`），网络差时 JS 崩溃 → `applyI18nCommon()` 不执行 → 全黑无文字
  2. `start.bat` 中存在"旧二进制直接启动"的短路逻辑（`if exist binary → run`），用户修改源码后不重新编译，永远运行旧版
- **修复**: 
  1. 下载 DOMPurify 3.0.6 完整库写入 `static/purify.min.js`，通过 `rust_embed` 编译进二进制，完全消除网络依赖
  2. 所有 6 个模板（index、idea、ai、settings、patent_detail、office_action_response）将 CDN URL 替换为 `/static/purify.min.js`
  3. `start.bat` 重写：删除"旧二进制跳过编译"短路逻辑，改为每次启动都执行 `cargo build --release`，确保二进制与源码一致
- **预防**: 
  1. 新页面引用 JS 库时优先使用本地 `/static/` 而非 CDN
  2. `start.bat` / `dev.bat` 应保持"先编译后启动"语义，不允许跳过编译
  3. 新增前端依赖时检查是否已本地化，未本地化的同步下载到 `static/`
- **提交**: `90731fc`（v0.6.2 包含 6 模板，v0.6.3 补 OA 页 `c250f50`）

---

### [2026-06-24] 上下文截断用字节计数导致中文字丢失
- **严重程度**: HIGH
- **涉及文件**: `src/ai/`（截断逻辑）
- **现象**: 上下文截断按字节计数，30k 字节的中文 prompt 因每字 3 字节被过早截断，丢失关键上下文，AI 分析跑偏。
- **根因**: 截断阈值以字节为单位，未考虑 UTF-8 中文每字 3 字节的膨胀，实际可用字数远小于阈值表面值。
- **修复**: 截断从字节改为按字符数计数，30k 中文字不再被误截；同时将分析上限放宽到 6 万字、讨论 4 万字、OA 1.5 万字作为安全网（正常使用不触达）。
- **预防**: 凡是面向「用户可见内容长度」的阈值（截断/校验/计数），一律按 `chars().count()` 而非 `len()`（字节）。新增阈值时明确单位。
- **提交**: `060e6e2` / `04c31ec`

### [2026-06-24] 讨论历史传裸 JSON 导致 AI 回复跑题
- **严重程度**: MEDIUM
- **涉及文件**: OA 讨论相关模板 / 后端讨论历史构造
- **现象**: OA 讨论把历史以裸 JSON 形式喂给 AI，AI 无法区分发明人与 AI 的发言，回复跑题、自问自答。
- **根因**: 历史消息未格式化为对话样式，AI 没有清晰的发言者边界。
- **修复**: 讨论历史从裸 JSON 改为「发明人：… / AI：…」对话格式，AI 回复不再跑题。
- **预防**: 喂给 AI 的多轮历史必须显式标注发言者角色，不要直接序列化原始 JSON。
- **提交**: `bf61a28`

### [2026-06-24] DOMPurify 安全包装器遗漏 + 专利号/日期未自动识别
- **严重程度**: MEDIUM
- **涉及文件**: `templates/*.html`（漏网页面引用 DOMPurify 处）
- **现象**: 部分 DOMPurify 调用点未走统一安全包装器，且专利号、日期未自动识别为可引用对象。
- **根因**: 早期 DOMPurify 本地化迁移覆盖不全，缺少统一包装函数；专利号/日期识别未纳入。
- **修复**: 统一 DOMPurify 安全包装器 + 修复漏网引用 + 增加专利号/日期自动识别。
- **预防**: 批量替换后用 `grep -r "DOMPurify" templates/` 核对全部调用点一致；前端 sanitize 一律走统一包装器。
- **提交**: `084a6b5`

### [2026-06-27] PDF 文本提取对中文扫描件质量不足 — 新增 MinerU 云端兜底
- **严重程度**: MEDIUM
- **涉及文件**: `src/routes/upload.rs`（`extract_pdf_text` 降级链、`extract_pdf_text_mineru`）
- **现象**: 中文专利扫描件 / 复杂多栏版式下，前 5 级 PDF 提取降级（含 Rust 原生逐页）文本质量仍不佳，影响后续 AI 分析。
- **根因**: 纯本地提取对扫描件 OCR 与版面还原能力有限。
- **修复**: 新增第 6 级降级 `extract_pdf_text_mineru`，调用 MinerU 云端 API（OCR + 版面还原），仅在前 5 级全失败时触发，不改变现有降级链顺序。
- **预防**: 文本提取质量不足时优先以「新增最低优先级兜底」方式扩展，不替换现有可用链路。
- **提交**: `4780356`

### [2026-06-29] 前端截断破坏数据完整性 — 技术调研 PDF 全文被截到 3000 字
- **严重程度**: HIGH
- **涉及文件**: `templates/index.html` line 447/465（两处 `.substring(0, 3000)`）、`src/routes/idea.rs` line 174（后端 10000 字限制）
- **现象**: PDF 文件上传后全文被截到 3000 字，导致 AI 分析残缺、导出报告不完整。叠加后端 1 万字限制，大 PDF 全部报错。
- **根因**: 做 localStorage 持久化时，为省存储空间顺手加了 `substring(0, 3000)`，没人发现这是数据截断而非显示截断。
- **修复**: 前端移除两处截断，后端上限 10000→200000。
- **预防**: 所有截断（`substring`/`slice`/`chars().take()`）必须区分「显示截断」和「数据截断」——数据截断一律禁止，显示截断必须标注注释 `// display only`。
- **提交**: `9a959c4`

### [2026-06-29] DOMPurify 崩溃导致加载全文、标签页、讨论全部失灵
- **严重程度**: HIGH
- **涉及文件**: `templates/patent_detail.html`（applyEnrichedData）、`templates/office_action_response.html`、`templates/*.html`（多处 DOMPurify.sanitize 直接调用）
- **现象**: purify.min.js 加载失败时，任何页面调用 DOMPurify.sanitize() 都会抛异常，导致 JS 函数链式中断——"加载全文"不更新内容、标签页空文本、OA 讨论报"分析失败"。
- **根因**: DOMPurify 从 CDN 改为本地嵌入后，未考虑加载失败的兜底。所有 DOMPurify.sanitize 调用点都没有 null 保护。
- **修复**: 每个页面 `<script>` 块第一行加全局保护（DOMPurify 未定义时自动创建 fallback sanitize 函数），CSAE 从安全增强变成必崩点。
- **预防**: 项目中任何外部依赖的调用点都必须考虑加载失败场景，设全局保护或 try-catch。改完后 `grep -r "DOMPurify\.sanitize" templates/` 确认无遗漏。
- **提交**: `1e1b6a0`

---

*最后更新: 2026-06-29*
