# 2026-07-13 AI 单次调用 60 秒上限修复计划

## 背景

项目规约要求单次 AI 调用上限为 60 秒。目前 `src/ai/client.rs` 的分级超时分别为 90/180/300 秒，默认 HTTP 客户端和全局 `tokio::time::timeout` 也是 300 秒；OA 流式路径使用分析级 180 秒。这会让单个卡住的模型请求长时间占用用户会话和服务资源。

## 范围

仅修改以下 Rust 文件及相应单元测试：

- `src/ai/client.rs`
- `src/ai/chat.rs`
- `src/routes/mod.rs`

不新增 crate、不修改路由、公开 API、数据库 schema 或前端模板。

## 实施步骤

1. 将 Chat、Analysis、Enrichment 的单次调用超时统一为 60 秒，并让默认 HTTP 客户端与 `GLOBAL_TIMEOUT_SECS` 同样遵守 60 秒。
2. 保留现有超时类型和调用结构，避免影响调用方；更新流式 OA 路径的过期说明，使其明确同样受 60 秒单次上限约束。
3. 更新既有全局超时回归，并添加常量级回归，防止将来把任一 AI 单次调用窗口重新扩大到 60 秒以上。
4. 保留现有用户友好的超时错误与上游失败降级语义；重试不会突破包裹整个调用的 60 秒全局守卫。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`

完成后检查所有 `AiClient` HTTP/流式/多模态入口均使用 60 秒或更短的单次守卫，随后更新 CHANGELOG、STATUS、错误复盘和本计划并按规范提交。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `cade390` (`fix: 限制 AI 单次调用为 60 秒`)
- **结果 / Result**: Chat、Analysis、Enrichment、默认 HTTP、全局守卫和 OA 流式客户端均为 60 秒；重试和用户可见的错误降级仍然保留，且不会突破单次调用时限。
- **验证 / Verification**: 新增时钟上限回归；`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（285 passed, 1 ignored）通过。构建正式二进制后首页与 OA 答复页均为 HTTP 200。
