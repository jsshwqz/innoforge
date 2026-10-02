# 2026-07-13 专利图片代理响应边界加固计划

## 背景

专利图片代理已限制 HTTPS、精确可信域名、默认端口和无重定向，避免请求被引向任意内网地址。但响应读取仍使用无上限的 `resp.bytes()`，会透传任意 `Content-Type`，并以 `String::leak` 为动态 MIME 创建永久内存占用。可信上游异常、受攻击或返回错误内容时，仍可能造成内存耗尽或同源内容类型风险。

## 范围

仅修改 `src/routes/patent.rs` 及其现有 `patent_image_proxy_tests`：

- 不新增 crate、路由、数据库迁移或前端改动。
- 保留现有的可信 URL、30 秒超时和禁止重定向语义。

## 实施步骤

1. 为单张上游图片定义 20 MiB 上限，并在声明 `Content-Length` 超限时提前拒绝。
2. 用 `resp.chunk()` 替代 `resp.bytes()`，在每一块累计时强制实际长度上限，以覆盖缺失或伪造的长度头。
3. 将响应 `Content-Type` 去参数、忽略大小写并映射到静态安全栅格 MIME 白名单；拒绝缺失类型、SVG、HTML 和其它类型，移除 `String::leak`。
4. 关闭环境代理，防止代理配置改变已验证 URL 的实际出站路径。
5. 补充纯函数回归：安全 MIME、危险/缺失 MIME、声明长度边界、流式累计边界。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建并启动正式二进制，验证首页和 OA 页可访问；以不可信图片 URL 验证代理在网络请求前返回 403。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `421df26` (`fix: 限制专利图片代理响应`)
- **结果 / Result**: 图片代理的 URL 出站边界之外，新增 20 MiB 声明长度与流式长度边界、安全栅格 MIME 白名单、`no_proxy()`，并移除响应 MIME 的字符串泄漏。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（293 passed, 1 ignored）通过。正式二进制的 `/`、`/oa-response` 返回 HTTP 200，不可信图片 URL 返回 HTTP 403。
