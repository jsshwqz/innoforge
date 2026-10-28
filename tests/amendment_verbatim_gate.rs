//! UA3 v2 修改稿特征逐字核验门（M2 第 2 步）：分析流挂接点的负控 / 正控。
//!
//! 负控（红通道）：把 `check_oa_analysis` 的槽位④退回到 `check_a33_risk`（17 词表 +
//! 「修改为：「…」」正则）→ 本文件 `zero_hit_amendment_must_be_caught_by_analysis_gate`
//! 的 `A33逐字核验` 断言翻红（零命中措辞既不在 17 词表里、也不是该句式，旧判据全放过）。
//!
//! 夹具一律通用技术词，无申请号/人名/公开号/案件专有长句。

use innoforge::ai::check_oa_analysis;
use innoforge::evidence_kernel::amendment_gate::{
    gate_amendment_text, GateStatus, UNVERIFIED_MARK,
};

/// 原始申请文本（说明书 + 具体实施方式段落）。通用技术词。
const ORIGINAL: &str = "\
说明书
技术领域
本发明涉及一种物料输送装置。
背景技术
现有输送装置的输送带易跑偏。
发明内容
本发明要解决的技术问题是输送带跑偏。
具体实施方式
[0001] 一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机。
[0002] 输送带上方设有导料板，导料板与输送带之间的间距为20～30厘米。
[0003] 驱动电机的输出轴与输送带的主动辊通过链条连接，链条外侧设有张紧轮。
";

/// 零命中却声称"明确记载"的答复书片段（负控夹具）。
const LETTER_ZERO_HIT: &str = "\
=== 意见陈述书 ===

一、关于审查意见的答复
申请人认为对比文件未公开该特征。

二、关于权利要求的修改
修改后的权利要求1：
一种物料输送装置，包括机架，其特征在于，还包括设置在机架上的隔离罩，所述隔离罩与所述输送带之间的间隙小于预设差值。

上述修改均来源于原说明书具体实施方式的明确记载，未超出原始申请文件记载的范围。

三、关于创造性的论述
本申请具备创造性。
";

/// 合法改写（从属权项并入独立权项 + 五类撰写变换）的正控夹具。
const LETTER_SUPPORTED: &str = "\
=== 意见陈述书 ===

二、关于权利要求的修改
修改后的权利要求1：
一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机，输送带上方设有导料板，链条外侧设有张紧轮。

上述修改来源于原说明书具体实施方式的明确记载。
";

#[test]
fn zero_hit_amendment_must_be_caught_by_analysis_gate() {
    // 负控：修复前（槽位④仍调 check_a33_risk）此处没有任何 A33逐字核验 警告 → FAILED
    let report = check_oa_analysis(LETTER_ZERO_HIT, "", ORIGINAL);
    let hit = report
        .warnings
        .iter()
        .find(|w| w.category == "A33逐字核验")
        .unwrap_or_else(|| {
            panic!(
                "零命中却声称有记载必须被分析流挂接点抓住，实际 warnings: {:?}",
                report.warnings
            )
        });
    assert_eq!(hit.severity, "致命", "零命中应为致命档: {hit:?}");
    assert!(
        hit.description.contains("隔离罩") && hit.description.contains("差值"),
        "警告须点名零命中措辞: {}",
        hit.description
    );
    assert!(
        report.score <= 65.0,
        "零命中后评分必须跌破答复书网关阈值（提交度压死），实际 {}",
        report.score
    );

    // 同一份文本的实体门：就地标注 + 断言降级 + 提交度压死
    let gate = gate_amendment_text(LETTER_ZERO_HIT, ORIGINAL);
    assert_eq!(gate.status, GateStatus::Degraded);
    assert_eq!(gate.unsupported_count, 2, "{:?}", gate.unsupported);
    assert!(
        gate.gated_text
            .contains(&format!("{UNVERIFIED_MARK}还包括设置在机架上的隔离罩")),
        "标注必须就地落在特征句前: {}",
        gate.gated_text
    );
    assert!(
        !gate
            .gated_text
            .contains("来源于原说明书具体实施方式的明确记载"),
        "零命中时不得保留不降级的“明确记载”断言: {}",
        gate.gated_text
    );
    assert!(!gate.submission_ready, "N>0 时提交度维度必须为假");
}

#[test]
fn supported_amendment_produces_no_penalty() {
    // 正控：合法改写不得误伤（修复前后均 ok）
    let report = check_oa_analysis(LETTER_SUPPORTED, "", ORIGINAL);
    assert!(
        !report.warnings.iter().any(|w| w.category == "A33逐字核验"),
        "合法改写不得被判零命中: {:?}",
        report.warnings
    );
    let gate = gate_amendment_text(LETTER_SUPPORTED, ORIGINAL);
    assert_eq!(gate.status, GateStatus::Verified);
    assert_eq!(gate.unsupported_count, 0);
    assert!(gate.gated_text.contains("明确记载"), "无零命中时断言不动");
}

#[test]
fn empty_original_never_accuses() {
    // 没有原始文本 → 不得凭空判零命中（只能报"跳过"）
    let report = check_oa_analysis(LETTER_ZERO_HIT, "", "");
    assert!(
        !report.warnings.iter().any(|w| w.category == "A33逐字核验"),
        "缺原文时不得指控: {:?}",
        report.warnings
    );
    let gate = gate_amendment_text(LETTER_ZERO_HIT, "");
    assert_eq!(gate.status, GateStatus::NoOriginal);
    assert_eq!(gate.gated_text, LETTER_ZERO_HIT);
}

#[test]
fn a33_gate_and_c2_c12_are_orthogonal_both_penalized() {
    // 要求 4：与 C2/C12 范畴正交——同时触发属违规并罚，不互相抑制。
    let with_c12 = format!("{LETTER_ZERO_HIT}\n综上，该区别特征属于常规设置，无需创造性劳动。\n");
    let a33_only = check_oa_analysis(LETTER_ZERO_HIT, "", ORIGINAL);
    let both = check_oa_analysis(&with_c12, "", ORIGINAL);

    assert!(
        both.warnings.iter().any(|w| w.category == "A33逐字核验"),
        "C12 同时触发时 A33 判据不得被抑制: {:?}",
        both.warnings
    );
    assert!(
        both.warnings
            .iter()
            .any(|w| w.category.contains("C12禁令-P1")),
        "A33 同时触发时 C12 判据不得被抑制: {:?}",
        both.warnings
    );
    assert!(
        both.score < a33_only.score,
        "两条判据必须各自计分（并罚），实际 both={} a33_only={}",
        both.score,
        a33_only.score
    );
}
