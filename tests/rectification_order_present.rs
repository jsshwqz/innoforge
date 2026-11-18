//! 永久产品宪章与规约换挡静态守卫（T-101）
//!
//! 原 `2026-10-27 整顿令` 冻结窗口已于 2026-11-18 结项归档至 `docs/plans/archive/2026-10-27-rectification.md`。
//! 本守卫强制校验：
//! 1. `AGENTS.md`、`CLAUDE.md`、`docs/plans/product-vision.md` 均完整内嵌全谱系创研「六大永久公理」；
//! 2. `docs/plans/product-vision.md` 保留 §3 最高实效验收标尺（报告敢用 · 决策敢做 · 答复与成果敢交）；
//! 3. `docs/plans/2026-11-18-v1-overhaul-plan.md` 重构总书与旧整顿令归档件均在位；
//! 4. `docs/errors-digest.md` 体积硬控在 ≤ 4096 字节（4KB）以内，防挤爆 `src/context.rs` 6000 字窗口。

const AXIOM_MARKERS: &[&str] = &[
    "表层每张卡片独立自治",
    "四维全域情报检索",
    "多模态",
    "审查与审稿三栖抗辩",
    "一源四出",
];

#[test]
fn permanent_six_axioms_are_present_in_all_charter_entries() {
    let entries = [
        ("AGENTS.md", include_str!("../AGENTS.md")),
        ("CLAUDE.md", include_str!("../CLAUDE.md")),
        (
            "docs/plans/product-vision.md",
            include_str!("../docs/plans/product-vision.md"),
        ),
    ];

    for (path, text) in entries {
        for marker in AXIOM_MARKERS {
            assert!(
                text.contains(marker),
                "{path} 缺少永久公理标记 `{marker}`——严禁将创研台退化回窄化的单一专利工具描述"
            );
        }
        assert!(
            text.contains("存量与增量") || text.contains("存量资产与增量研发"),
            "{path} 缺少公理 2（存量与增量双线并行）标记"
        );
    }
}

#[test]
fn product_vision_acceptance_bar_and_v1_plan_are_intact() {
    let vision = include_str!("../docs/plans/product-vision.md");
    for bar in ["报告敢用", "决策敢做", "答复与成果敢交"] {
        assert!(
            vision.contains(bar),
            "docs/plans/product-vision.md §3 缺少最高验收口径 `{bar}`"
        );
    }

    let overhaul = include_str!("../docs/plans/2026-11-18-v1-overhaul-plan.md");
    assert!(
        overhaul.contains("T-101") && overhaul.contains("T-504"),
        "docs/plans/2026-11-18-v1-overhaul-plan.md 必须完整保留 T-101～T-504 施工看板"
    );

    let archived_rectification = include_str!("../docs/plans/archive/2026-10-27-rectification.md");
    assert!(
        archived_rectification.contains("2026-10-27 整顿令"),
        "历史整顿令归档件必须保留完整备查"
    );
}

#[test]
fn errors_digest_stays_within_4kb_budget() {
    let digest = include_bytes!("../docs/errors-digest.md");
    assert!(!digest.is_empty(), "docs/errors-digest.md 不得为空");
    assert!(
        digest.len() <= 4096,
        "docs/errors-digest.md 当前 {} 字节，超过 4096 字节（4KB）上限（ERR-06）",
        digest.len()
    );
}
