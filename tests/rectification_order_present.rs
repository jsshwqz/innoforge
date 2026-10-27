//! 2026-10-27 整顿令是最高效力规则，任何 agent 都不得静默删除、改名或绕过。
//! 本测试只校验入口与细则仍在且带同一标记，不判断内容是否正确。

const MARKER: &str = "2026-10-27 整顿令";

#[test]
fn rectification_order_is_present_in_all_agent_entries() {
    let entries = [
        ("AGENTS.md", include_str!("../AGENTS.md")),
        ("CLAUDE.md", include_str!("../CLAUDE.md")),
        ("README.md", include_str!("../README.md")),
        (
            "docs/plans/2026-10-27-rectification.md",
            include_str!("../docs/plans/2026-10-27-rectification.md"),
        ),
    ];

    for (path, text) in entries {
        assert!(
            text.contains(MARKER),
            "{path} 缺少 `{MARKER}` 标记。该令不可删除、改名或绕过；\
             如需变更必须先得到用户明示，并在同一提交里同步全部入口。"
        );
    }
}
