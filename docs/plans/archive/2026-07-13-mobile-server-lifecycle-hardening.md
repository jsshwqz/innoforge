# 2026-07-13 移动端嵌入服务生命周期加固计划

## 背景

`src/lib.rs` 的 Android/iOS FFI 入口负责启动本地 Axum 服务。原实现在工作线程内 `unwrap()` 创建 Tokio runtime，并对全局服务句柄的 Mutex 使用 `unwrap()`；任何资源创建失败或锁中毒都会跨 FFI 边界 panic。重复启动还会替换现有句柄，使旧服务线程无法再被正确关闭。

## 范围

仅修改 `src/lib.rs`：

- 不新增 crate、路由、数据库迁移或前端改动。
- 保持既有 ABI、端口 3000，以及 `0` 成功、`1` 失败的 FFI 返回码约定。

## 实施步骤

1. 在线程启动前创建 Tokio runtime，并使用返回 `Result` 的线程构建器，使创建失败回传给 FFI 调用方。
2. 将启动与关闭中的 Mutex 获取改为受控错误分支，不允许 panic。
3. 在启动前检查并拒绝已有服务句柄，防止重复调用覆盖旧线程和关闭通道。
4. 关闭时在短暂锁作用域中取出句柄，在锁外发送关闭信号并等待线程退出。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建正式二进制并验证桌面端 `/` 和 `/oa-response` 页面可访问。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `6f193ec` (`fix: 加固移动端服务生命周期`)
- **结果 / Result**: runtime、线程、锁和重复启动失败均转换为 FFI 返回码 `1`；原服务句柄不再被覆盖，关闭在锁外等待线程。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（293 passed, 1 ignored）通过。正式二进制的 `/` 与 `/oa-response` 均返回 HTTP 200。
