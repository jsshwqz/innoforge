# SerpAPI 代理问题修复指南

## 问题
系统代理 (http://127.0.0.1:10808) 导致 Rust reqwest 无法连接 SerpAPI

## 解决方案 1：临时禁用代理测试

在启动服务器前设置环境变量：
```powershell
$env:no_proxy="*"
$env:NO_PROXY="*"
cargo run --release
```

## 解决方案 2：修改代码禁用代理

在 `src/routes.rs` 第 226 行，将：
```rust
let client = reqwest::Client::new();
```

替换为：
```rust
let client = reqwest::Client::builder()
    .no_proxy()
    .danger_accept_invalid_certs(true)
    .build()
    .unwrap_or_else(|_| reqwest::Client::new());
```

## 解决方案 3：配置代理例外

在你的代理软件（Clash/V2Ray）中，将 `serpapi.com` 添加到直连列表。

## 验证

搜索"王青芝"应该返回中国专利结果，而不是空列表。
