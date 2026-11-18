//! 项目记忆与永久公理上下文汇编 / Project Charter & Memory Context
//!
//! 按 `AGENTS.md` §二 优先级读取：
//! 1. `AGENTS.md`（工程宪章核心章节）
//! 2. `docs/plans/product-vision.md`（全谱系创研六大永久公理与 §3 验收标尺）
//! 3. `docs/feedback.md`（近期用户反馈尾部）
//! 4. `docs/errors-digest.md`（≤4KB 高频踩坑铁律摘要）
//!
//! 总注入字符预算硬控在 `≤ 6000` 字符以内，缺失文件静默降级，严禁裸字节切片。

use std::fs;
use std::path::{Path, PathBuf};

/// 上下文总字符上限（硬门禁 ≤ 6000 字符）。
pub const CONTEXT_MAX_CHARS: usize = 6000;

const HEADER: &str =
    "> [创研台项目宪章与记忆上下文 · 自动汇编] 严守六大永久公理与防幻觉门禁，不覆盖用户输入材料原文。\n";
const TRUNCATION_SUFFIX: &str =
    "\n…（上下文预算已达 6000 字符上限，完整细则见 AGENTS.md 与 docs/）";

/// 从默认工作目录汇总项目宪章与记忆上下文。
pub fn build_agent_context() -> String {
    build_agent_context_from(Path::new("."))
}

/// 从指定根目录按优先级汇总上下文（便于单测隔离验证）。
pub fn build_agent_context_from(root: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();

    // 1. 最高优先级：AGENTS.md（取前 45 行，完整覆盖 §〇 六大公理 + §一～§三 工程红线）
    let agents = head_lines(&root.join("AGENTS.md"), 45);
    if !agents.is_empty() {
        parts.push(format!("## 工程宪章（AGENTS.md）\n{}", agents));
    }

    // 2. 产品灵魂：docs/plans/product-vision.md（取前 66 行，覆盖 §1～§3 验收口径）
    let vision = head_lines(&root.join("docs/plans/product-vision.md"), 66);
    if !vision.is_empty() {
        parts.push(format!("## 产品核心愿景（product-vision.md）\n{}", vision));
    }

    // 3. 近期用户反馈：追加式文件，取末尾 25 行
    let fb = tail_lines(&root.join("docs/feedback.md"), 25);
    if !fb.is_empty() {
        parts.push(format!("## 用户反馈（近期）\n{}", fb));
    }

    // 4. 高频踩坑摘要：docs/errors-digest.md（≤4KB 精炼铁律）
    let digest = head_lines(&root.join("docs/errors-digest.md"), 60);
    if !digest.is_empty() {
        parts.push(format!("## 高频踩坑铁律（errors-digest.md）\n{}", digest));
    }

    if parts.is_empty() {
        return String::new();
    }

    let full = format!("{}{}", HEADER, parts.join("\n\n"));
    enforce_char_budget(&full, CONTEXT_MAX_CHARS)
}

fn enforce_char_budget(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let suffix_chars = TRUNCATION_SUFFIX.chars().count();
    let keep = max_chars.saturating_sub(suffix_chars);
    let body: String = text.chars().take(keep).collect();
    format!("{}{}", body, TRUNCATION_SUFFIX)
}

fn head_lines(path: &PathBuf, n: usize) -> String {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    content.lines().take(n).collect::<Vec<&str>>().join("\n")
}

fn tail_lines(path: &PathBuf, n: usize) -> String {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let skip = lines.len().saturating_sub(n);
    lines
        .into_iter()
        .skip(skip)
        .collect::<Vec<&str>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_context_respects_budget_and_contains_six_axioms() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ctx = build_agent_context_from(&manifest);
        assert!(!ctx.is_empty(), "仓库上下文汇编不应为空");
        let char_count = ctx.chars().count();
        assert!(
            char_count <= CONTEXT_MAX_CHARS,
            "上下文总字符数 {} 超过上限 {}",
            char_count,
            CONTEXT_MAX_CHARS
        );
        assert!(ctx.contains("表层每张卡片独立自治"), "上下文必须注入公理 1");
        assert!(ctx.contains("四维全域情报检索"), "上下文必须注入公理 3");
        assert!(ctx.contains("审查与审稿三栖抗辩"), "上下文必须注入公理 5");
    }

    #[test]
    fn missing_files_degrade_silently_to_empty() {
        let dir = tempfile::tempdir().expect("create empty tempdir");
        let ctx = build_agent_context_from(dir.path());
        assert!(ctx.is_empty(), "缺失全部记忆文件时应静默返回空串");
    }

    #[test]
    fn oversized_inputs_are_strictly_capped_at_6000_chars() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let plans = dir.path().join("docs/plans");
        fs::create_dir_all(&plans).expect("mkdir docs/plans");
        let huge_line = "全谱系创新锻造台测试长行".repeat(30);
        let huge_doc = (0..40)
            .map(|_| huge_line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(dir.path().join("AGENTS.md"), &huge_doc).expect("write AGENTS.md");
        fs::write(plans.join("product-vision.md"), &huge_doc).expect("write vision");

        let ctx = build_agent_context_from(dir.path());
        assert!(
            ctx.chars().count() <= CONTEXT_MAX_CHARS,
            "超大输入截断后字符数 {} 仍不得超过 {}",
            ctx.chars().count(),
            CONTEXT_MAX_CHARS
        );
        assert!(ctx.ends_with(TRUNCATION_SUFFIX));
    }
}
