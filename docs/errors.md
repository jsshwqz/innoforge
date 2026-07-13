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

## [2026-07-13] 无效 SerpAPI Key 会被静默丢弃后清空已有配置
- **严重程度**: HIGH
- **涉及文件**: `src/routes/settings.rs`
- **现象**: 设置页提交短 Key、非法字符、未知掩码或错误 JSON 结构时，服务端会把无效项从数组中静默过滤，随后清空现有 SerpAPI Key 并返回“保存成功”；用户会在一次输错后丢失可用搜索配置。
- **根因**: 输入解析使用 `filter_map`，将校验失败折叠为空数组，且清空持久化操作在任何“至少一个合法 Key”或“请求确实是数组”的验证之前执行。
- **修复**: 使用纯解析函数先完成数组形态、数量上限、每项格式、掩码唯一恢复和完整 Key 校验；任一项失败直接返回错误，绝不进入清空、写入或内存更新。空数组继续作为明确的主动清空请求。
- **预防**: 会替换或删除用户配置的批量更新必须采用“全量校验 → 再变更”的两阶段模式；禁止静默过滤无效输入并把部分结果当作完整请求。掩码值必须能唯一恢复，否则要求用户输入完整凭据。
- **提交**: `6be94b6`

## [2026-07-13] 静态正则初始化失败会在生产路径 panic，转义职责迁移需防止 XSS 回退
- **严重程度**: HIGH
- **涉及文件**: `src/routes/idea.rs`, `src/routes/patent.rs`
- **现象**: 创意报告行内 Markdown、专利说明书 HTML 清理和 Sogou 法律状态页面解析将 `Regex::new` 直接接 `expect()`；若正则定义被后续改坏，请求会 panic。将 HTML 转义从调用处收拢到行内 Markdown 函数时，若首个替换仍读取原始文本，还会重新输出未转义 HTML。
- **根因**: 代码假定静态正则永远有效，并把初始化错误排除在 API 的正常错误分支之外；转义职责从多个调用点迁移到公共函数时，缺少同时覆盖格式渲染和危险 HTML 输入的回归测试。
- **修复**: `OnceLock` 缓存 `Result<Regex, regex::Error>` 而非仅缓存 `Regex`。Markdown 初始化异常记录日志并保留已转义原文；专利富文本清理返回友好 JSON；法律状态解析返回受控错误进入既有降级链。新增 Markdown 标签渲染和 `<script>` 转义测试。
- **预防**: 生产路径的编译器、正则、模板和文件格式初始化均须显式处理 `Result`，不得 `unwrap`/`expect`；任何安全职责（如转义）迁移都必须以正常功能测试和攻击性输入测试成对验证。
- **提交**: `26e20b2`

## [2026-07-13] DOCX 导出在 ZIP 写入失败时可能 panic，非可信字段可破坏 Word XML
- **严重程度**: HIGH
- **涉及文件**: `src/docx_export/export.rs`, `src/routes/ai.rs`
- **现象**: OA 答复书导出在任何 `ZipWriter::start_file`、`write_all` 或 `finish` I/O 失败时会调用 `unwrap()`，使请求处理路径崩溃；专利号、申请人和审查意见类型直接插入 `word/document.xml`，其中的 `&`、`<`、`>` 会使导出的 Word 文件 XML 无效。
- **根因**: DOCX 写入实现将内存 ZIP 操作误当作不会失败，且只对答复正文进行 XML 文本转义，遗漏了同一 XML 模板中的其余外部字段。
- **修复**: `generate_docx` 改为返回 `Result<Vec<u8>, String>`，统一传播 ZIP 写入与收尾错误；复用 XML 转义函数处理四个外部文本字段。API 记录内部错误并返回用户友好的导出失败消息；新增内存 ZIP 回归测试校验 `word/document.xml` 的四字段转义。
- **预防**: 所有文件格式导出必须将 I/O 失败作为正常错误分支处理，生产路径不得 `unwrap`/`expect`；每个插值进 XML/HTML/CSV 等结构化格式的外部字段都必须按其上下文转义，并用实际解包后的产物验证。
- **提交**: `b8b6e89`

## [2026-07-13] 移动端 FFI 服务生命周期在失败或重复调用时可能 panic 或丢失句柄
- **严重程度**: HIGH
- **涉及文件**: `src/lib.rs`
- **现象**: 移动端嵌入服务器的 Tokio runtime 在工作线程内以 `unwrap()` 创建，资源不足时会 panic；全局服务状态锁中毒也会跨 FFI 边界 panic。重复启动会直接替换既有线程和关闭发送端，可能遗留无法管理的服务器线程。
- **根因**: 启动状态未在创建服务器前受控同步，运行时与线程创建错误未纳入 `Result`，并假定 Mutex 永不损坏；关闭时也在锁表达式中直接操作句柄。
- **修复**: 在启动线程前创建 runtime，并使用可返回错误的 `std::thread::Builder::spawn`；对 Mutex 获取、重复启动与所有创建失败返回 FFI 错误码 `1`。关闭路径只在短暂锁作用域中 `take` 句柄，随后在锁外关闭和 `join`。
- **预防**: 所有 FFI 边界必须把资源创建与同步失败转换为显式返回码或错误，禁止 `unwrap`/`expect` 和后台线程静默失败；全局句柄的替换、关闭和等待必须明确其并发与所有权语义。
- **提交**: `6f193ec`

## [2026-07-13] 专利图片代理无响应大小和媒体类型边界
- **严重程度**: HIGH
- **涉及文件**: `src/routes/patent.rs`
- **现象**: 即使 URL 已限制为可信专利图片域名，上游仍可返回任意 MIME 和任意长度的响应；`resp.bytes()` 会一次性将完整内容读入内存，缺失或虚假的长度声明可造成内存压力。原实现还把动态 MIME 字符串 `leak()` 为静态引用，每次代理请求都会永久占用内存。
- **根因**: URL 出站校验完成后，响应入站路径未建立类型、声明长度和实际流长度的边界，且为满足 Axum 静态响应头类型使用了不安全的字符串泄漏。
- **修复**: 禁用环境代理；将 MIME 标准化为静态的安全栅格类型白名单；在读取前检查 `Content-Length`，并以 `chunk()` 累积检查实际字节数，任一超过 20 MiB 即返回受控 502 错误。
- **预防**: 每个代理/下载入口都必须同时限制目标地址、允许的响应媒体类型、声明长度、实际流式长度和重定向/代理行为；不得以 `String::leak` 将外部响应数据提升为静态生命周期。
- **提交**: `421df26`

## [2026-07-13] 本地 PDF 上传仅校验扩展名，伪装文件可进入解析与外部工具
- **严重程度**: HIGH
- **涉及文件**: `src/routes/upload.rs`
- **现象**: `.txt` 或 HTML 内容只要命名为 `.pdf`，即可进入本地文件存储、PDF 文本提取、OCR 或 AI 视觉回退路径；无效文件会消耗解析资源并产生误导性的“提取失败”错误。
- **根因**: 远程专利 PDF 下载已校验 `%PDF-` 文件签名，但四个本地上传入口只根据客户端可伪造的文件扩展名分支，没有在进入处理路径前复用同一签名校验。
- **修复**: 复用 `has_pdf_header`，在 PDF 存储、对比上传、通用提取和专利专用提取入口中验证前 1024 字节内的 `%PDF-`。专利专用入口在直传与远程下载汇合后统一验证，确保所有来源遵循同一边界。
- **预防**: 文件扩展名和 MIME 声明均视为不可信提示；每个二进制文件入口必须在落盘、解析或传给外部工具前验证适用的内容签名，并以纯函数测试覆盖有效、伪装和边界位置。
- **提交**: `8170576`

## [2026-07-13] 单次 AI 调用可等待 90 至 300 秒
- **严重程度**: HIGH
- **涉及文件**: `src/ai/client.rs`, `src/ai/chat.rs`, `src/routes/mod.rs`
- **现象**: 聊天、专利分析、OA 流式讨论及多模态请求的单次上游调用可使用 90/180/300 秒超时；上游卡住时，研发用户长时间无反馈，服务资源也被持续占用。
- **根因**: 分级超时设计将复杂请求的 HTTP 与全局守卫放宽到 180 或 300 秒，且默认提供商客户端与流式路径未被 60 秒项目规约统一约束。
- **修复**: 保留分级常量和调用接口，但将其全部收紧为 60 秒；默认 HTTP、流式 HTTP 与 `GLOBAL_TIMEOUT_SECS` 同步为 60 秒，以全局守卫限制重试总时长。
- **预防**: 新增 AI 入口必须复用不超过 60 秒的时钟并在代码审查中检查 HTTP、流式和外层 `tokio::time::timeout` 三层；常量回归测试不得允许任何单次 AI 时钟大于 60 秒。
- **提交**: `cade390`

## [2026-07-13] AI 聊天历史角色与外部材料可提升为提示词指令
- **严重程度**: HIGH
- **涉及文件**: `src/routes/ai.rs`
- **现象**: 客户端可在聊天历史中伪造 `system` 角色并直接送入上游模型；OA 讨论、专利记录和联网搜索材料也会被直接拼进提示词，使材料中的伪指令可能与固定任务规则混淆。
- **根因**: 历史角色未做白名单校验，且不同 AI 路径各自拼接外部字符串，没有统一的数据边界或闭合标签逃逸处理；原始自定义角色还会直接替换系统提示词。
- **修复**: 限制历史角色为 `user`/`assistant`，并在所有外部参考材料周围使用转义的 `<user_input>` 边界。原始自定义角色改为受限偏好，只有服务端预设可以作为可信系统角色；压缩摘要不再使用 `system`。
- **预防**: 任何新增 AI prompt 都必须复用统一的边界工具；不得信任来自 HTTP、数据库、搜索或模型输出的角色和标签。只允许服务端生成 `system` 消息，并为标签逃逸与角色提升添加回归。
- **提交**: `be815cb`

## [2026-07-13] 系统临时目录与 PID 命名导致 C 盘占用和并发串扰
- **严重程度**: HIGH
- **涉及文件**: `src/routes/upload.rs`
- **现象**: PDF 提取、视觉回退和 OCR 会在 Windows `TEMP`/`TMP` 写入文件；默认系统临时目录位于 C 盘，且多个路径仅以进程 PID 命名，使并发请求可能覆盖同一材料。部分外部工具失败或提前返回时还可能留下临时文件。
- **根因**: 外部工具调用各自临时实现，直接使用 `std::env::temp_dir()`、`File::create()` 和分散的手工清理；Umi-OCR 还写入了请求流未使用的副本。
- **修复**: 统一受控的 `RuntimeTempFiles` 守卫：以 UUID 和 `OpenOptions::create_new(true)` 在项目 `data/runtime-temp` 创建文件，守卫析构时反向清理输入和输出路径。`pdftotext` 捕获标准输出，Umi-OCR 改为直接使用内存请求体。
- **预防**: 任何新增的外部文件工具必须复用受控临时文件守卫，路径只能基于应用工作目录，不得采用系统临时目录、进程 PID 或用户提供的文件名；新增路径须覆盖成功、失败和提前返回的清理测试。
- **提交**: `dcb446d`

## [2026-07-13] 远程专利 PDF 下载缺少 SSRF 与容量边界

- **严重程度**: HIGH
- **涉及文件**: `src/routes/upload.rs`
- **现象**: 按专利 ID 提取 PDF 正文时，数据库中的 `pdf_url` 会被直接请求，自动跟随
  重定向并将整个响应读入内存；攻击者可通过导入记录把请求引向内网或让超大响应耗尽内存。
- **根因**: 将搜索/导入来源的 URL 当成可信地址，未做结构化 URL、DNS 公网地址、代理、
  重定向、响应状态与流式大小校验。
- **修复**: 只允许 HTTPS 主机名、默认 443 和无凭据 URL；解析并固定全部公网 DNS
  地址，禁用代理/重定向，拒绝非 2xx、声明或流式超过 20 MB 的响应，并校验 PDF
  签名。
- **预防**: 任何由数据库或第三方数据提供的 URL 都必须视为不可信输入；外部下载须在
  发请求前完成 URL/DNS 校验与地址固定，并同时具备声明长度和实际流式长度上限。
- **提交**: `2fd64ab`

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
  3. `src/ai/patent.rs` → OA 上下文截断从 120K/80K/120K → 300K/200K/300K
  4. 服务已重启（PID 4860）
- **二次修复** (2026-07-05):
  - 发现 DeepSeek v4-flash 流式 SSE 将可见文本放在 `delta.reasoning_content` 而非 `delta.content`，服务端 SSE 解析器漏读所有 `reasoning_content` 块，导致讨论 API 一直返回空内容（仅 `event:done`）
  - `ResponseMessage` 结构体新增 `reasoning_content: Option<String>`
  - SSE 流式解析器新增 `reasoning_content` 读取兜底（优先 `content`，回退 `reasoning_content`）
  - 提交: `8a50d8d`
  - 浏览器是否缓存旧页面 HTML（编译嵌入的模板）
  - 前端 `renderMarkdown()` 函数是否有未发现的字符转换
  - SSE 流式拼接时是否有数据丢失
  - `parseOASections()` 切分逻辑是否误切
- **预防**: 每次变更模板文件后用 Puppeteer 全量 e2e 测试
- **提交**: `c39dd20`（完整修复包）+ `5ff5945`（上下文溢出修复）+ `7fc1aec`（clippy 清理）
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

### [2026-07-13] OA 前端上下文被静默截断 / OA frontend context silently truncated
- **严重程度 / Severity**: HIGH
- **涉及文件 / File**: `templates/office_action_response.html`
- **现象 / Symptom**: 修改校验与讨论流程只向 AI 发送 OA、本专利、对比文件或既有分析结果的前 1500~3000 字，长材料尾部证据静默丢失。
  Amendment checking and discussion sent only the first 1,500–3,000 characters of OA and patent context, silently dropping evidence near the end of long documents.
- **根因 / Root cause**: 为控制请求体大小在数据构造处直接使用 `.slice(0, N)`，混淆了显示截断与数据截断。
  Request-size control was implemented with `.slice(0, N)` inside data construction, conflating display truncation with data truncation.
- **修复 / Fix**: 移除 6 个进入 AI 请求的正文截断表达式；保留日期、协议解析等非数据用途的 `slice`。
  Removed six body truncations that feed AI requests while retaining slices used only for dates and protocol parsing.
- **验证 / Verification**: 定制 Puppeteer 回归确认长文本尾部标记完整进入修改校验请求、讨论初始上下文和讨论消息；245 项 Rust 测试通过。
  A focused Puppeteer regression confirmed end markers survive in amendment and discussion payloads; all 245 Rust tests passed.
- **预防 / Prevention**: 对所有新增截断标注数据用途或显示用途；数据容量限制应在后端显式校验并返回用户可见错误，禁止静默截断。
  Label every new truncation as display-only or data-bearing; enforce capacity on the backend with visible validation errors instead of silent truncation.
- **提交 / Commit**: `f496648`

### [2026-07-13] 搜索页在函数定义前调用 / Search called a function before its definition
- **严重程度 / Severity**: MEDIUM
- **涉及文件 / File**: `templates/search.html`
- **现象 / Symptom**: 搜索页面初始化时在后续脚本块定义 `updatePdfFileList()` 前调用它，浏览器报 `ReferenceError`，使该页面的加载期错误屏障触发。
  Search initialization called `updatePdfFileList()` before a later script block defined it, producing a browser `ReferenceError` and triggering the page's load-time error barrier.
- **根因 / Root cause**: 模板将一个依赖后续 `<script>` 声明的函数调用放在前一个 `<script>` 的同步执行路径中。
  The template placed a call that depends on a later `<script>` declaration on an earlier script's synchronous execution path.
- **修复 / Fix**: 将调用移至函数声明之后，未改变 PDF 元数据恢复或上传逻辑。
  Moved the call after the function declaration without altering PDF metadata restoration or upload logic.
- **预防 / Prevention**: 跨脚本块调用前确认声明顺序；浏览器回归必须把 `pageerror` 与 console error 视为失败，而不是只检查 HTTP 200。
  Verify declaration order across script blocks; browser regressions must fail on `pageerror` and console errors rather than checking HTTP 200 alone.
- **提交 / Commit**: `aad56d5`

### [2026-07-13] OA “导出结论”语义与可审计性不足 / OA “Export Conclusions” lacked clear semantics and auditability
- **严重程度 / Severity**: MEDIUM
- **涉及文件 / File**: `templates/office_action_response.html`
- **现象 / Symptom**: 按钮名称暗示会导出最终结论，但实际会向 AI 提交讨论消息并在页面追加二次摘要；用户无法下载完整讨论过程交给其他 AI 或人工复核。
  The button name implied exporting a final conclusion, but it actually submitted discussion messages to AI and appended a second summary in the page; users could not download the complete discussion for external AI or human review.
- **根因 / Root cause**: “AI 摘要”和“原始讨论记录”两类不同证据等级的产物共用“导出结论”这一模糊文案，且不存在本地记录导出实现。
  Two outputs with different evidentiary value—an AI summary and the original discussion record—shared the ambiguous “Export Conclusions” label, and no local-record export existed.
- **修复 / Fix**: 将既有功能更名为“AI 总结结论”，并新增本地 UTF-8 Markdown 完整记录导出；记录包含完整上下文、角色、ISO 时间、原文和“未经 AI 二次改写”声明，且不调用 AI。
  Relabelled the existing function “AI Summary” and added a local UTF-8 Markdown full-record export containing full context, roles, ISO time, source text, and a “not AI-rewritten” statement without calling AI.
- **预防 / Prevention**: 面向复核的功能必须区分“原始记录”和“模型摘要”；任何可审计导出都必须保留全文、标记生成方式，并由浏览器测试验证导出内容和零外部 AI 请求。
  Features intended for review must distinguish “original record” from “model summary”; any auditable export must preserve full text, label its generation method, and be browser-tested for content and zero external AI requests.
- **提交 / Commit**: `5588968`

### [2026-07-13] Windows start.bat CMD 解析失败 / Windows start.bat CMD parse failure
- **严重程度 / Severity**: HIGH
- **涉及文件 / File**: `start.bat`
- **现象 / Symptom**: 从桌面快捷方式启动时，在 debug 构建分支报“此时不应有 ...”，服务未启动。
  Launching from the desktop shortcut failed in the debug build branch with a CMD parse error before the server started.
- **根因 / Root cause**: 括号代码块内的 `echo` 文本包含未转义圆括号，CMD 将其解释为控制语法。
  An `echo` line inside a parenthesized block contained unescaped parentheses that CMD parsed as control syntax.
- **修复 / Fix**: 将该提示改为不含圆括号的文本；未改变构建和启动逻辑。
  Reworded the message without parentheses; build and launch logic is unchanged.
- **验证 / Verification**: 实际运行 `start.bat`，debug 编译完成，服务启动，首页和 OA 页面均 HTTP 200。
  Ran `start.bat` end to end; debug build completed, the server started, and both the home and OA pages returned HTTP 200.
- **提交 / Commit**: `fec45b9`

### [2026-07-13] OA 后端静默截断 / OA backend silent truncation
- **严重程度 / Severity**: HIGH
- **涉及文件 / Files**: `src/routes/ai.rs`, `src/ai/patent.rs`, `src/ai/client.rs`
- **现象 / Symptom**: 超长 OA 分析、讨论历史或 OA 原文在讨论和答复书生成前被静默截掉尾部，用户无法知道 AI 未看到完整材料。
  Oversized OA analysis, discussion history, or office-action text lost its tail before discussion or response-letter generation without user visibility.
- **根因 / Root cause**: 服务端把 provider 容量保护实现为 `safe_truncate_chars`，将“可显示的容量限制”误实现为数据丢弃。
  Provider capacity protection used `safe_truncate_chars`, turning a visible capacity limit into data loss.
- **修复 / Fix**: 改为按 Unicode 字符数的显式容量校验；超限时通过既有 JSON/SSE 错误通道返回字段名、实际字符数和上限，且不启动 AI 请求。
  Replaced truncation with Unicode-character capacity validation; oversized input returns field, actual count, and limit through the existing JSON/SSE error path without starting an AI request.
- **预防 / Prevention**: 数据用途文本禁止截断；所有新增 AI 上下文上限必须提供可见错误或可追踪分段，并覆盖 Unicode 边界测试。
  Data-bearing text must not be truncated; new AI context limits require visible errors or traceable chunking and Unicode-boundary tests.
- **提交 / Commit**: `ce303d2`

### [2026-07-13] 本地服务 CORS 全开放 / Local-service CORS was open to all origins
- **严重程度 / Severity**: HIGH
- **涉及文件 / Files**: `src/common.rs`
- **现象 / Symptom**: 本地服务对任意 Origin 返回跨域许可，局域网或嵌入式来源一旦可访问服务即可从浏览器发起 API 请求。
  The local service granted CORS access to every Origin, allowing any browser origin that could reach it to issue API requests.
- **根因 / Root cause**: 路由中直接使用 `CorsLayer::allow_origin(Any)`，未区分本机桌面服务与显式受信任的扩展来源。
  The router used `CorsLayer::allow_origin(Any)` directly, without distinguishing local desktop access from explicitly trusted additional origins.
- **修复 / Fix**: 默认限制为两个本机来源；额外来源仅由 `INNOFORGE_CORS_ORIGINS` 提供，并验证为无路径/查询/片段/用户信息的 HTTP/HTTPS Origin。
  Restricted defaults to two local origins; additional origins come only from `INNOFORGE_CORS_ORIGINS` and are validated as HTTP/HTTPS origins with no path/query/fragment/user-info.
- **预防 / Prevention**: 不得重新引入 `allow_origin(Any)`；新增 WebView 或移动端来源须显式加入环境变量，并覆盖允许与拒绝来源预检。
  Do not reintroduce `allow_origin(Any)`; new WebView/mobile origins must be added explicitly through the environment variable and covered by allowed/rejected-origin preflight checks.
- **提交 / Commit**: `15f134a`

### [2026-07-13] 图片代理主机字符串匹配可被伪装 / Image-proxy host string matching was spoofable
- **严重程度 / Severity**: HIGH
- **涉及文件 / Files**: `src/routes/patent.rs`
- **现象 / Symptom**: 图片代理通过字符串前缀判断 URL，无法明确限制 URL 凭据、端口与重定向；如未加固，白名单主机可能成为访问非预期目标的跳板。
  The image proxy used URL string-prefix checks and did not explicitly constrain credentials, ports, or redirects; without hardening, an allowlisted host could become a pivot to unintended targets.
- **根因 / Root cause**: 未对 URL 进行结构化解析，且 reqwest 默认可跟随重定向。
  URLs were not parsed structurally, and reqwest followed redirects by default.
- **修复 / Fix**: 使用现有 `reqwest::Url` 校验 HTTPS、精确白名单主机、默认端口和无凭据；禁用重定向，客户端构建失败返回友好 502。
  Use existing `reqwest::Url` to validate HTTPS, exact allowlisted hosts, default port, and no credentials; disable redirects and return a friendly 502 on client construction failure.
- **预防 / Prevention**: 任何外部 URL 代理必须做结构化 scheme/host/port/credential 校验，白名单请求默认禁止重定向，并测试伪装 URL。
  Every external URL proxy must structurally validate scheme/host/port/credentials, disable redirects by default for allowlisted requests, and test spoofed URLs.
- **提交 / Commit**: `1ee38f1`

*最后更新 / Last updated: 2026-07-13*
