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

const FORBIDDEN: &str = "unwrap_or_else(|_| reqwest::Client::new())";

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
                    if line.contains(FORBIDDEN) {
                        hits.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            lineno + 1,
                            line.trim()
                        ));
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
        "发现静默无超时客户端兜底（issue #55 收口条件被破坏）：\n{}",
        hits.join("\n")
    );
}
