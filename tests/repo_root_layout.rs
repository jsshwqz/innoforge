//! 根目录是「门面」：只放用户与构建的入口，不放开发机上的东西。
//!
//! 背景：README、E2E 脚本、模板校验脚本、设计稿、手工排查脚本曾全部摊在根目录，
//! 外部读者第一眼看到的是开发机而不是一个产品。这条守卫把「根目录白名单」变成 CI：
//! 新文件想放根目录，必须显式改这里并在 PR 里说明理由。
//!
//! 白名单只针对**已跟踪文件**的顶层条目（`git ls-files`），不受本机 `target/`、
//! `data/` 等未跟踪产物影响。

use std::collections::BTreeSet;
use std::process::Command;

/// 根目录允许出现的顶层条目（目录或文件）。
const ALLOWED: &[&str] = &[
    // GitHub / 工具链
    ".dockerignore",
    ".env.example",
    ".gitattributes",
    ".githooks",
    ".github",
    ".gitignore",
    // 文档入口
    "AGENTS.md",
    "CHANGELOG.md",
    "CLAUDE.md",
    "CONTRIBUTING.md",
    "LICENSE",
    "README.md",
    // 构建
    "Cargo.lock",
    "Cargo.toml",
    "Dockerfile",
    "build.rs",
    "rust-toolchain.toml",
    // 前端工具链（模板校验 / E2E 依赖）
    "eslint.config.js",
    "package-lock.json",
    "package.json",
    // 源码与资源
    "docs",
    "scripts",
    "src",
    "start.bat",
    "static",
    "templates",
    "tests",
    "tools",
];

fn tracked_top_level() -> BTreeSet<String> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("需要 git 枚举已跟踪文件（CI 的 checkout 自带 git；源码压缩包内不跑测试）");
    assert!(
        out.status.success(),
        "git ls-files 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut tops = BTreeSet::new();
    for raw in out.stdout.split(|b| *b == 0) {
        if raw.is_empty() {
            continue;
        }
        let path = String::from_utf8_lossy(raw);
        if let Some(top) = path.split('/').next() {
            tops.insert(top.to_string());
        }
    }
    tops
}

#[test]
fn repo_root_has_only_expected_entries() {
    let tops = tracked_top_level();
    let extras: Vec<&String> = tops
        .iter()
        .filter(|t| !ALLOWED.contains(&t.as_str()))
        .collect();

    assert!(
        extras.is_empty(),
        "根目录出现未登记条目：{extras:?}\n\
         根目录只放用户/构建入口：开发脚本进 scripts/，手工排查脚本进 scripts/manual/，\n\
         设计文档进 docs/design/，过程记录进 docs/。\n\
         若确实是新的用户入口，请更新本测试的 ALLOWED 并在 PR 说明理由。"
    );
}

#[test]
fn repo_root_keeps_the_essentials() {
    let tops = tracked_top_level();
    for required in ["README.md", "LICENSE", "Cargo.toml", "src", "docs"] {
        assert!(
            tops.contains(required),
            "根目录缺少必需入口 `{required}`——清理时别把用户/构建入口一起搬走"
        );
    }
}

#[test]
fn negative_control_unregistered_top_level_is_caught() {
    let mut fake_tops = tracked_top_level();
    fake_tops.insert("unexpected_garbage_in_root".to_string());
    let extras: Vec<&String> = fake_tops
        .iter()
        .filter(|t| !ALLOWED.contains(&t.as_str()))
        .collect();
    assert_eq!(extras, vec![&"unexpected_garbage_in_root".to_string()]);
}
