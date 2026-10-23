# 2026-07-13 SerpAPI Key 保存完整性计划

## 背景

设置页支持最多五个轮询使用的 SerpAPI Key，并用掩码回传已有凭据。原保存实现静默过滤每个无效输入，随后无条件清空旧 Key；一次短 Key、未知掩码或错误 JSON 就可能让研发用户丢失全部在线检索配置，却仍看到保存成功。

## 范围

仅修改 `src/routes/settings.rs`。

- 不新增 crate、路由、数据库迁移或前端改动。
- 空数组仍表示用户明确主动清空所有 SerpAPI Key。
- 合法完整 Key 与可唯一恢复的既有掩码保持成功响应格式。

## 实施步骤

1. 在模块内新增纯解析函数，先验证 `api_keys` 必须是至多五项的数组。
2. 校验每一项为非空字符串、长度和允许字符符合 Key 规则；无法唯一恢复的掩码必须报错。
3. 仅在整个数组成功解析后，执行旧 Key 清空、新 Key 写入和内存配置更新。
4. 添加空数组清空、非数组/超限/短 Key/未知与歧义掩码拒绝、完整 Key 和唯一掩码成功的回归测试。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建正式二进制，并向 `/api/settings/serpapi` 提交无效短 Key，确认得到错误且不触发写入。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `6be94b6` (`fix: 防止无效 SerpAPI Key 清空配置`)
- **结果 / Result**: 无效输入不再清空现有在线检索配置；用户仍可显式清空配置，且掩码往返保持安全可用。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（305 passed, 1 ignored）、正式二进制构建和受控 HTTP 短 Key 拒绝验证均通过。
