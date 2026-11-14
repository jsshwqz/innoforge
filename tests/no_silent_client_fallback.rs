//! issue #55 静态防线：禁止上游 HTTP 客户端的「静默无超时兜底」。
//!
//! `reqwest::ClientBuilder::build()` 失败时（TLS 后端初始化等进程级故障），
//! 旧写法 `.unwrap_or_else(|_| reqwest::Client::new())` 有两层问题：
//! 1. `Client::new()` 内部同样是 `build().expect(...)`——同场景一样 panic，
//!    但信息更差，且代码形态暗示存在「可用的无超时兜底」；
//! 2. 即便不 panic，无超时客户端正是 #30 的失效形态（请求悬挂）。
//!
//! 收口（issue #55）：兜底必须 `expect` 明确失败（panic 信息点名 issue）
//! 或返回 `Result` 给调用方可见错误。本守卫静态扫描 `src/`，
//! `unwrap_or_else(|_| reqwest::Client::new())` 必须为空——
//! 新增上游客户端一律走 `expect` 明确失败或 Result 传播。

use std::path::Path;

/// 三种被禁形态（issue #55 审计后扩展——首版只抓 reqwest 全限定形态，
/// 漏掉了 `use reqwest::Client` 后的短形态与裸无超时客户端）：
/// 1. `.unwrap_or_else(|_| Client::new())`（含 reqwest 全限定写法）
///    —— builder 失败后的静默兜底；
/// 2. 裸 `Client::new()` / `reqwest::blocking::Client::new()`——无超时直连，
///    #30/#74 的失效形态。
/// 合法豁免：注释行（以 // 开头；文档与示例里的字面量不算）。
const FORBIDDEN_PATTERNS: &[&str] = &[
    "unwrap_or_else(|_| Client::new())",
    "unwrap_or_else(|_| reqwest::Client::new())",
    "Client::new()",
];

fn walk(dir: &Path, hits: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, hits);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for (lineno, line) in content.lines().enumerate() {
                    let trimmed = line.trim_start();
                    // 注释行不算（本守卫自身与代码文档都会出现这些字面量）
                    if trimmed.starts_with("//") {
                        continue;
                    }
                    for pat in FORBIDDEN_PATTERNS {
                        if line.contains(pat) {
                            hits.push(format!(
                                "{}:{}: {}",
                                path.display(),
                                lineno + 1,
                                line.trim()
                            ));
                            break;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn no_silent_no_timeout_client_fallback() {
    let mut hits = Vec::new();
    walk(Path::new("src"), &mut hits);
    assert!(
        hits.is_empty(),
        "发现无超时/静默兜底客户端（issue #55 + errors.md #74 收口被破坏）：\n{}\n\n修复方式：builder 链上显式 .timeout(...) 与 .connect_timeout(...)；构建失败用 .expect(明确信息)——Client::new() 在同场景一样 panic，只是更不诚实。",
        hits.join("\n")
    );
}
