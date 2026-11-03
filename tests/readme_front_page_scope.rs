//! README 首屏是给陌生用户的，不是内部工作台。
//!
//! 2026-10-27 的整顿令被放进 README 顶栏，本意是让 AI agent 进门就看到；
//! 结果第一屏变成「维护中 / 不可跳过 / ACK」，与已经发布的版本自相矛盾。
//! 这类错不需要用户来指出——用一条守卫把它变成 CI 的事：
//!
//! - 首屏（第一个 `---` 分隔线之前）不得出现内部治理词；
//! - 整顿令本身不删，入口留在「贡献」节（存在性由
//!   `rectification_order_present` 继续校验）。
//!
//! 只检查“读者拿到的第一屏”，不限制正文任何内容。

const README: &str = include_str!("../README.md");

fn front_page() -> &'static str {
    // README 的首屏是标题/徽章/定位/三步开始，第一处分节线之前的部分。
    README.split("\n---\n").next().unwrap_or(README)
}

#[test]
fn readme_front_page_has_no_internal_governance_markers() {
    let fp = front_page();
    for marker in ["维护中", "整顿令", "不可跳过", "agent 开工前", "ACK "] {
        assert!(
            !fp.contains(marker),
            "README 首屏出现了内部治理词 `{marker}`。\
             首屏应只回答「这是什么、怎么开始」；\
             维护者与 AI agent 的开工要求请放在「贡献」节并链接 AGENTS.md。\
             —— 2026-10-27 整顿令曾因此占据门面，见 PR #181。"
        );
    }
}

#[test]
fn readme_front_page_gives_a_start_path() {
    let fp = front_page();
    assert!(
        fp.contains("下载") && (fp.contains("解压") || fp.contains("快速开始")),
        "README 首屏必须给出下载/解压入口，而不是只有定位描述"
    );
}
