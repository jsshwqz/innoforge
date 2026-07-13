# 2026-07-13 AI 配置原子保存计划

## 背景

`api_save_ai` 原先先修改运行时内存配置，再逐项“尽力”写入 SQLite；任一写入失败只会记录警告，接口仍返回成功。这会使当前会话与重启后的配置不一致，也可能留下部分新值和部分旧值的混合状态。

## 范围

仅修改 `src/db/settings.rs` 和 `src/routes/settings.rs`：

- 不新增 crate、路由、数据库 schema 迁移或前端改动。
- 保持现有 API 成功响应格式与 `.env` 桌面备份职责。
- SQLite 仍是 Android 与桌面端共同使用的主存储。

## 实施步骤

1. 在数据库设置模块增加单连接的批量 upsert 方法，用 SQLite 事务包住全部写入。
2. 在 AI 配置处理器中先构建全部八项设置（含 `GEMINI_CLI_ENABLED`），事务失败时记录内部诊断并直接返回友好错误。
3. 仅在事务提交成功后切换内存配置；随后逐项更新可选 `.env` 备份，备份失败记录 warning 但不推翻主存储成功。
4. 增加内存数据库回归测试，覆盖已有值更新与多个设置一批持久化。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建正式二进制，并对页面可用性和无效 AI 配置请求执行不修改数据的 HTTP 验证。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `7628984` (`fix: 原子保存 AI 配置`)
- **结果 / Result**: AI 配置不会再在主存储失败后提前生效；全部设置要么一起提交，要么维持原状。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（307 passed, 1 ignored）、二进制构建及受控 HTTP 回归均通过。
