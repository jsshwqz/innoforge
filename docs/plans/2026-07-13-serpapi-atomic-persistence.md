# 2026-07-13 SerpAPI 多 Key 原子保存计划

## 背景

SerpAPI Key 已具备完整输入与掩码校验，但保存处理器仍按“先清空旧键位，再逐项写新键”的方式执行，并忽略 SQLite 写入错误。数据库失败会让运行时状态、持久化状态和重启后的状态发生分叉。

## 范围

仅修改 `src/routes/settings.rs`，复用现有批量事务接口：

- 不新增 crate、路由、schema 迁移或前端改动。
- 保持空数组代表显式清空、完整 Key 和唯一掩码恢复的既有语义。
- 保留 `SERPAPI_KEY` 兼容槽位与 `SERPAPI_KEY_1` 至 `_5` 编号槽位。

## 实施步骤

1. 用固定六个兼容/编号槽位构造完整替换批次，未使用槽位写入空值以移除陈旧配置。
2. 在内存或 `.env` 发生任何修改前，将完整批次提交给 SQLite 事务。
3. 事务失败时记录服务端错误并返回友好 JSON；事务成功后才更新内存 Key 列表。
4. 将 `.env` 明确降级为提交后的桌面备份，对每个失败写入 warning，且日志不含凭据值。
5. 添加批次形状回归测试，验证旧槽位会清空、当前槽位按序填入。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建二进制，并对页面可用性和非法 SerpAPI 请求执行无副作用 HTTP 验证。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `5bbedbc` (`fix: 原子保存 SerpAPI 配置`)
- **结果 / Result**: SerpAPI Key 的删除、减少和替换要么整体持久化成功，要么保持旧配置，不会再出现半成品配置或错误成功响应。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（309 passed, 1 ignored）、二进制构建及受控 HTTP 回归均通过。
