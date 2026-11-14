//! issue #56 需求 3 的 Rust 包装：运行 tests/i18n_runtime_56.mjs。
//!
//! 该脚本加载**真实的** static/i18n.js（最小 DOM/localStorage 桩），在两种
//! i18nLang 下实际调用 t()——缺失键回退到 key 本身（非 undefined）、真实键
//! 中英双语各自渲染、语言切换持久化。node 缺失时优雅跳过（与 dom_render_106
//! 同一模式）。

use std::process::Command;

#[test]
fn i18n_runtime_check_via_node() {
    let node_check = Command::new("node").arg("--version").output();
    if node_check
        .as_ref()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!("node not available, skipping runtime i18n test for #56");
        return;
    }

    let output = Command::new("node")
        .arg("tests/i18n_runtime_56.mjs")
        .output()
        .expect("failed to run i18n_runtime_56.mjs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("stdout:\n{}", stdout);
    if !stderr.trim().is_empty() {
        println!("stderr:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "i18n 运行时检查未通过（issue #56 需求 3）——详见上方输出"
    );
    assert!(
        stdout.contains("i18n runtime check PASSED"),
        "输出必须含 PASSED 标记"
    );
}
