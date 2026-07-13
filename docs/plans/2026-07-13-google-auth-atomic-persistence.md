# 2026-07-13 Google 认证状态原子持久化收尾计划

## 背景

Google 认证支持 gcloud CLI、ADC 文件与 OAuth 授权码三种来源。此前各路径会先更新运行期内存，随后通过多次独立 `set_setting()` 尽力保存 Token、过期时间、Refresh Token 与认证模式；其中一次失败会使当前会话和重启后的状态分叉，或留下部分新值和部分旧值。

当前工作区已存在 `src/routes/auth.rs` 的未提交实现：新增 `persist_google_auth_state()`，用 `set_settings_batch()` 将认证状态放入单个 SQLite 事务，并在成功提交后更新内存。该实现需要完成文档、测试与质量门禁闭环。

## 范围

- 保留并审查 `src/routes/auth.rs` 当前未提交的原子持久化改动。
- 为认证状态持久化补充成功路径、模式切换与字段保留语义测试。
- 将 `auth.rs`、`ai.rs`、`idea.rs`、`patent.rs` 的测试模块移至模块末尾，修复 `clippy::items_after_test_module`；仅移动测试代码，不调整生产逻辑。
- 更新状态文档和本计划的完成记录；将复盘记录写入项目外 memory，避免提交 AI 分析产物。

## 非目标

- 不新增 crate、API 路由、数据库 schema 或迁移。
- 不修改认证协议、前端页面、Google OAuth 请求格式。
- 不改变已有 token 刷新的 60 秒超时或 provider 策略。

## 实施步骤

1. 检查 `persist_google_auth_state()` 的调用点，确保 gcloud、ADC、OAuth 初次授权及后台刷新均先事务持久化、后写运行时内存。
2. 测试以下语义：
   - OAuth 会一次写入 access token、expiry、refresh token、`oauth` 模式；
   - gcloud/ADC 初次认证会清空遗留 refresh token，并写入 `gcloud` 模式；
   - 后台刷新传入 `None` 时，只替换 access token/expiry，保留 refresh token 和认证模式。
3. 将 4 个路由文件中的 `#[cfg(test)] mod tests` 统一放至文件末尾，恢复严格 Clippy。
4. 运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
5. 更新 `docs/plans/STATUS.md`、本计划完成记录；不提交当前会话创建的 GPT 交接草稿。

## 风险与回退

- SQLite 事务失败时，`set_settings_batch()` 自动回滚；调用方不得写入新内存状态。
- 测试仅使用内存数据库，不触碰用户配置、真实 Token 或 `.env`。
- 测试模块重排属于布局变更；若行为验证失败，保留生产实现并先回退测试移动。

## 验收标准

- gcloud、ADC、OAuth 三种路径均使用同一事务性持久化函数。
- 模式切换不会保留不应存在的 refresh token。
- 后台刷新不会意外清空 refresh token 或认证模式。
- `cargo fmt --check` 通过。
- `cargo clippy --all-targets -- -D warnings` 零警告。
- `cargo test` 全部通过。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成，待用户终审与提交 / Completed, pending user final review and commit
- **实现 / Implementation**:
  - `persist_google_auth_state()` 统一覆盖 gcloud、ADC、OAuth 初次授权及三类后台刷新路径；全部先调用 `set_settings_batch()` 事务持久化，提交成功后才更新运行时内存。
  - OAuth 初次授权原子写入 access token、expiry、refresh token 与 `oauth` 模式；gcloud/ADC 初次认证明确清空旧 refresh token 并写入 `gcloud` 模式；后台刷新传入 `None` 时仅更新 access token/expiry。
  - 为恢复全仓严格 Clippy，将 `auth.rs`、`ai.rs`、`idea.rs`、`patent.rs` 的测试模块移至各文件末尾，未改变生产逻辑。
- **新增测试 / Added tests**:
  - OAuth 完整状态原子保存；
  - OAuth → gcloud 模式切换清空遗留 refresh token；
  - 后台刷新保留 refresh token 与认证模式。
- **验证 / Verification**:
  - `cargo fmt --check` ✅
  - `cargo clippy --all-targets -- -D warnings` ✅
  - `cargo test` ✅（137 单元测试 + 6 编排集成测试 + 37 集成测试通过；1 个 doctest 按设计忽略）
  - `git diff --check` ✅
- **代码提交 / Code commit**: 待用户终审后填写 / To be filled after user final review
