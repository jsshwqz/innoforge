//! M2-B（基线失败 #2）回归：空文本事实核查不得假绿。
//!
//! 旧行为：check_oa_analysis("") 走完 5 个检测器 → warnings 空 → 100/100
//! "未检测到明显事实性问题"。M1 实测在主分析静默失败后发生（空文本拿满分）。
//!
//! 负控（红通道）：移除 `src/ai/fact_check.rs` 中 `ai_output.trim().is_empty()`
//! 早退块（git stash 该文件）→ `empty_input_must_not_pass` 必须翻红。
//! 本文件只用公开 API，对旧代码可独立编译。

use innoforge::ai::{check_oa_analysis, format_report};

#[test]
fn empty_input_must_not_pass() {
    for empty in ["", "   ", "\n\n \t "] {
        let r = check_oa_analysis(empty, "对比文件原文", "本申请原文");
        assert_eq!(r.score, 0.0, "空输入评分必须为 0，got {}", r.score);
        assert!(
            !r.warnings.is_empty(),
            "空输入必须产生显式警告（不得静默通过）"
        );
        assert!(
            r.warnings[0].category.contains("无内容"),
            "首条警告应为无内容类，got: {}",
            r.warnings[0].category
        );
        let text = format_report(&r);
        assert!(
            !text.contains("未检测到"),
            "空输入不得显示'未检测到问题'，got: {text}"
        );
        assert!(text.contains("无内容"), "应显示无内容提示，got: {text}");
    }
}

#[test]
fn zero_width_only_input_must_not_pass() {
    // M2 审查 P1-7：零宽字符（trim 不视为空白）不得绕过空输入检查
    for zw in ["\u{200B}", "\u{200B}\u{200C}\u{200D}", "\u{FEFF} \u{200B}"] {
        let r = check_oa_analysis(zw, "对比文件原文", "本申请原文");
        assert_eq!(r.score, 0.0, "零宽输入评分必须为 0，got {}", r.score);
        assert!(!r.warnings.is_empty(), "零宽输入必须显式警告");
        let text = format_report(&r);
        assert!(!text.contains("未检测到"), "零宽输入不得假绿: {text}");
    }
}

#[test]
fn clean_nonempty_text_keeps_full_score() {
    // 非空且无风险模式的文本仍应 100 分并显示"未检测到"（正常路径不误伤）
    let r = check_oa_analysis(
        "本申请涉及一种温控装置的技术方案。",
        "温控装置相关文献",
        "本申请说明书",
    );
    assert_eq!(
        r.score, 100.0,
        "干净文本应保持满分，got {} warnings={:?}",
        r.score, r.warnings
    );
    let text = format_report(&r);
    assert!(
        text.contains("未检测到"),
        "干净文本应显示未检测到问题，got: {text}"
    );
}
