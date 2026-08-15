//! 项目记忆上下文 / Project Memory Context

//!

//! 从仓库中的记忆文件（反馈、错误复盘、状态）自动汇总一段有字符预算的

//! 上下文片段，注入到 AI 系统提示词，让 agent 每次对话都能拿到近期记忆。

//! 文件缺失或不可读时静默返回空字符串，不影响主流程。

use std::fs;

use std::path::PathBuf;

/// 上下文总字符上限，防止 token 膨胀。

const CONTEXT_MAX_CHARS: usize = 6000;

/// 汇总项目记忆，返回可直接拼进系统提示词的 markdown 字符串。

/// 文件缺失/不可读时静默返回空字符串。

pub fn build_agent_context() -> String {
    let mut parts: Vec<String> = Vec::new();

    // 用户反馈：最靠后的条目最新（追加式），取末尾 40 行

    let fb = tail_lines(PathBuf::from("docs/feedback.md"), 40);

    if !fb.is_empty() {
        parts.push(format!("## 用户反馈（近期）\n{}", fb));
    }

    // 错误复盘：同样追加式，取末尾 35 行

    let err = tail_lines(PathBuf::from("docs/errors.md"), 35);

    if !err.is_empty() {
        parts.push(format!("## 错误复盘（近期）\n{}", err));
    }

    // 当前状态：最新条目在最上方，取开头 20 行

    let status = head_lines(PathBuf::from("docs/plans/STATUS.md"), 20);

    if !status.is_empty() {
        parts.push(format!("## 当前项目状态\n{}", status));
    }

    if parts.is_empty() {
        return String::new();
    }

    let combined: String = parts.join("\n\n");

    let capped = if combined.chars().count() > CONTEXT_MAX_CHARS {
        let truncated: String = combined.chars().take(CONTEXT_MAX_CHARS).collect();

        format!(

            "> [项目记忆上下文 · 自动汇总] 以下是近期相关记忆，仅供补充参考，不覆盖上方材料原文。\n{}\n…\n（已截断，完整内容见 docs/ 下对应文件）",

            truncated

        )
    } else {
        format!(

            "> [项目记忆上下文 · 自动汇总] 以下是近期相关记忆，仅供补充参考，不覆盖上方材料原文。\n{}",

            combined

        )
    };

    capped
}

/// 取文件末尾 n 行。

fn tail_lines(path: PathBuf, n: usize) -> String {
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,

        Err(_) => return String::new(),
    };

    let lines: Vec<&str> = content.lines().collect();

    if lines.len() <= n {
        return content;
    }

    lines[lines.len() - n..].join("\n")
}

/// 取文件开头 n 行。

fn head_lines(path: PathBuf, n: usize) -> String {
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,

        Err(_) => return String::new(),
    };

    let lines: Vec<&str> = content.lines().collect();

    if lines.len() <= n {
        return content;
    }

    lines[..n].join("\n")
}
