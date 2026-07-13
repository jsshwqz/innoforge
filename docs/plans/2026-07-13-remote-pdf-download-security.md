# 2026-07-13 远程专利 PDF 下载安全修复计划

## 背景

`/api/patents/import` 可写入外部来源的 `Patent.pdf_url`。随后
`/api/patent/pdf/extract-text` 会按专利 ID 下载该地址的文件。现有下载逻辑
未校验地址、会跟随重定向，并把无上限响应整体读入内存，存在 SSRF、DNS
重绑定和内存耗尽风险。

## 范围

仅修改 `src/routes/upload.rs`（包含该文件内测试）。不新增 crate、路由、
公开 API 或数据库 schema。

## 实施步骤

1. 对数据库中的 `pdf_url` 使用结构化 URL 解析：只允许 HTTPS、默认 443
   端口、无用户凭据，并拒绝 IP 字面量。
2. 使用 `tokio::net::lookup_host` 解析主机名，拒绝空结果以及回环、私网、
   链路本地、未指定、多播、共享和保留 IP 地址（含 IPv4-mapped IPv6）。
   把已验证的公网 `SocketAddr` 用已有 `reqwest 0.11` 的
   `resolve_to_addrs` 固定到本次请求，并启用 `no_proxy()`，避免 DNS 重绑定
   与环境代理绕过。
3. 禁用重定向，保留 30 秒网络超时；拒绝非 2xx 状态及超过 20 MB 的
   `Content-Length`，再流式读取并累计执行同一硬上限，最后验证 `%PDF-`
   文件头。失败向前端返回可操作的通用错误，详细上游原因仅写日志。
4. 在同文件新增无网络依赖的单元回归，覆盖恶意 URL、私网/回环/保留 IPv4
   与 IPv6、正常公网地址、声明/流式超限和伪 PDF；另使用回环 mock 确认
   本地目标在出网前被拒绝。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`

三项均通过后，更新 CHANGELOG、STATUS、错误复盘和路线图状态，并按规范提交。

## 完成记录 / Completion record

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `2fd64ab` (`fix: 加固远程专利 PDF 下载安全`)
- **实现 / Implementation**: 数据库中的远程 PDF URL 仅接受 HTTPS 主机名、默认
  443 端口和无凭据地址；DNS 解析结果必须全部为公网地址，并被固定到该请求。
  请求禁用代理与重定向，检查 2xx、声明和流式 20 MB 上限，以及前 1024 字节内的
  PDF 签名。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings` 和
  `cargo test` 通过（273 passed, 1 ignored）；首页与 OA 页面在新版服务上均为
  HTTP 200。
