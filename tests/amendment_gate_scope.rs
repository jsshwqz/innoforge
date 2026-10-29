//! UA3 v2 修改稿逐字核验门的**作用域**负控 / 正控（M2 第 2 步回归修复）。
//!
//! 缺陷：`src/ai/fact_check.rs:1350` 与 `src/routes/ai.rs:2057` 把**整封信**交给
//! `gate_amendment_text`，而门内的段落定位只要在正文里"看到"marker 就开块，于是
//! 「申请人认为修改后的权利要求1-3……」这类**叙述句**被当成修改稿段落起点，块一直
//! 延伸到下一节标题——信里所有非权项文本逐句零命中、被打满标记。
//! 真实案件实测：`amendment_unsupported=136`，正文 8,219 字里插了 137 个降级标注，
//! 连「权利要求1-3」都被拆坏，产物完全不可用。
//!
//! 本文件锁四件事（对应任务书验收四项）：
//! 1. 负控：含"尊敬的审查员/申请人认为/审查意见概述"的信，修改稿本身合法 → **0 标记**；
//! 2. 正控：修改稿里恰好 1 个零命中措辞 → **恰好 1 处**标记且落在该措辞上；
//! 3. 计数一致：横幅 `amendment_unsupported=N` 与正文标记数相等；
//! 4. 抽取失败：信中无修改稿 marker → `unsupported=0` + meta 状态为 skipped + 说明可见。
//!
//! 夹具一律通用技术词，无申请号/人名/公开号/案件专有长句。

use innoforge::evidence_kernel::amendment_gate::{
    amendment_blocks, gate_amendment_text, GateStatus, UNVERIFIED_MARK,
};

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

/// 负控夹具：正文叙述句里提到「修改后的权利要求1-3」，修改稿段落本身全部有原文支撑。
const LETTER_PROSE_MENTION: &str = "\
=== 意见陈述书 ===

尊敬的审查员：

申请人认为修改后的权利要求1-3已克服审查意见指出的全部缺陷，具体理由如下。

一、关于审查意见的答复
审查意见概述：审查员认为权利要求1缺少必要技术特征，且对比文件1未公开相关结构。
申请人认为，对比文件1并未公开输送带与导料板的配合关系，也未给出相应技术启示。
进一步地，本申请通过驱动电机驱动主动辊，使输送带稳定运行，解决了跑偏问题。
上述区别特征在本申请中产生了预料不到的技术效果，现有技术中没有任何教导。
因此，本申请权利要求1具备突出的实质性特点和显著的进步。

二、关于权利要求的修改
修改后的权利要求1：
一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机。

三、关于创造性的论述
本申请具备创造性。

四、结论
综上所述，申请人认为本申请符合专利法及其实施细则的相关规定。

此致

申请人：某公司
日期：某日
";

/// 正控夹具：修改稿里放一个原文零命中的措辞（「隔离罩」在 ORIGINAL 中不存在）。
const LETTER_ONE_ZERO_HIT: &str = "\
=== 意见陈述书 ===

尊敬的审查员：

一、关于审查意见的答复
申请人认为对比文件1未公开相关结构。

二、关于权利要求的修改
修改后的权利要求1：
一种物料输送装置，包括机架，机架上设有输送带，其特征在于，还包括设置在机架上的隔离罩。

三、关于创造性的论述
本申请具备创造性。
";

/// 抽取失败夹具：整封信不含任何修改稿 marker。
const LETTER_NO_AMENDMENT: &str = "\
=== 意见陈述书 ===

尊敬的审查员：

一、关于审查意见的答复
审查意见概述：审查员认为权利要求1缺少必要技术特征。
申请人认为，对比文件1并未公开输送带与导料板的配合关系。

二、关于创造性的论述
本申请具备突出的实质性特点和显著的进步。

三、结论
综上所述，申请人认为本申请符合专利法及其实施细则的相关规定。
";

fn marks(text: &str) -> usize {
    text.matches(UNVERIFIED_MARK).count()
}

/// 验收 1（负控）：非权项叙述不得被标注——修复前本例会打出满篇标记。
#[test]
fn prose_mentions_do_not_scope_the_gate() {
    let blocks = amendment_blocks(LETTER_PROSE_MENTION);
    // 叙述句"申请人认为修改后的权利要求1-3已克服…"位于这个字节之前；块起点不得落在那儿
    let prose_at = LETTER_PROSE_MENTION
        .find("申请人认为修改后的权利要求")
        .expect("夹具须含叙述句");
    let heading_at = LETTER_PROSE_MENTION
        .find("二、关于权利要求的修改")
        .expect("夹具须含修改稿小节");
    assert!(
        blocks.iter().all(|(s, _)| *s >= heading_at),
        "有块起点落在修改稿小节之前（＝把叙述句当成了修改稿起点）: blocks={blocks:?}, prose_at={prose_at}, heading_at={heading_at}"
    );
    assert!(
        !blocks.is_empty(),
        "真修改稿段落仍须被识别为块（否则就是收紧成漏检）"
    );
    let r = gate_amendment_text(LETTER_PROSE_MENTION, ORIGINAL);
    assert_eq!(
        marks(&r.gated_text),
        0,
        "非权项叙述被打了 {} 个标记（修复前形态）：\n{}",
        marks(&r.gated_text),
        r.gated_text
    );
    assert_eq!(r.unsupported_count, 0, "清单: {:?}", r.unsupported);
    assert!(r.submission_ready);
    assert!(r.banner().is_empty());
    // 修改稿段落仍在扫描范围内（不是"整封跳过"，而是"只扫真修改稿"）
    assert!(
        r.checked_feature_count > 0,
        "必须真的核验过修改稿特征句，而不是没扫"
    );
    assert_eq!(
        r.status,
        GateStatus::Verified,
        "合法修改稿应判 verified，实际 {:?}",
        r.status
    );
}

/// 验收 2（正控）：修改稿里 1 个零命中措辞 ⇒ 恰好 1 处标记，且落在该措辞上；
/// 信的其他部分（叙述、结论）一个字都不许标。
#[test]
fn single_zero_hit_marks_exactly_that_phrase() {
    let r = gate_amendment_text(LETTER_ONE_ZERO_HIT, ORIGINAL);
    assert_eq!(
        marks(&r.gated_text),
        1,
        "应恰好 1 处标记，实际 {} 处:\n{}",
        marks(&r.gated_text),
        r.gated_text
    );
    assert_eq!(r.unsupported_count, 1);
    assert!(
        r.gated_text
            .contains(&format!("{UNVERIFIED_MARK}还包括设置在机架上的隔离罩")),
        "标记必须紧贴零命中措辞之前，实际产物:\n{}",
        r.gated_text
    );
    assert!(
        r.gated_text.contains("还包括设置在机架上的隔离罩"),
        "原句必须保留（只降级不删除）"
    );
    assert!(r.unsupported.iter().any(|u| u.text.contains("隔离罩")));
    // 叙述段未被波及
    let tail = r
        .gated_text
        .split("三、关于创造性的论述")
        .nth(1)
        .unwrap_or("");
    assert_eq!(marks(tail), 0, "修改稿之外的文字不得被标注");
    assert!(!r.submission_ready, "有零命中时提交度维度必须压死");
}

/// 验收 3：横幅计数 == 正文标记数（含横幅自身在内的整份交付文本一起数）。
#[test]
fn banner_count_equals_body_marker_count() {
    for letter in [&LETTER_ONE_ZERO_HIT, &LETTER_PROSE_MENTION] {
        let r = gate_amendment_text(letter, ORIGINAL);
        let delivered = format!("{}{}", r.banner(), r.gated_text);
        assert_eq!(
            marks(&delivered),
            r.unsupported_count,
            "横幅计数与标记数不一致（N={}，交付文本标记={}）",
            r.unsupported_count,
            marks(&delivered)
        );
        assert!(
            !r.banner().contains(UNVERIFIED_MARK),
            "横幅不得自带标记字面量，否则读者会数出 N+1: {}",
            r.banner()
        );
    }
}

/// 验收 4：抽不到修改稿 ⇒ 不得整封扫；`unsupported=0` 且状态/meta 明确 skipped、说明可见。
#[test]
fn no_amendment_section_is_skipped_not_scanned() {
    let r = gate_amendment_text(LETTER_NO_AMENDMENT, ORIGINAL);
    assert_eq!(marks(&r.gated_text), 0, "无修改稿时不得标任何句子");
    assert_eq!(r.unsupported_count, 0);
    assert_eq!(r.status, GateStatus::NoAmendment);
    assert_eq!(r.status.as_str(), "skipped_no_amendment_section");
    assert_eq!(r.checked_feature_count, 0);
    assert!(
        r.skip_note().contains("未定位到"),
        "skipped 必须给出可见说明: {:?}",
        r.skip_note()
    );
    assert!(!r.gated_text.is_empty(), "gated_text 应保持原信完整");
    assert_eq!(r.gated_text, LETTER_NO_AMENDMENT);
}

/// 验收 5（真实答复书形态回归）：v4 的主因是「三、关于创造性的论述」下的
/// `（一）修改后的权利要求1` 小标题被当修改稿起点，块一路吞到下一节，整节论证被打满标记。
/// 修复后：标记只允许出现在「二、关于权利要求的修改」之内，论述节 0 标记。
#[test]
fn real_letter_shape_confines_marks_to_amendment_section() {
    const LETTER_REAL_SHAPE: &str = "\
=== 意见陈述书 ===

尊敬的审查员：

通知书认为修改后的权利要求1-3相对于对比文件1不具备专利法第22条第3款规定的创造性。

一、关于审查意见的答复
审查意见概述：审查员认为权利要求1缺少必要技术特征。

二、关于权利要求的修改
1. 权利要求1的修改：
在保留原权利要求1全部特征（含驱动电机、隔离罩、导料板）的基础上，进一步限定：
（1）将驱动电机进一步限定为：“所述驱动电机的输出轴与输送带的主动辊通过链条连接”；
（2）在末尾限定调节过程：“所述张紧轮的调节行程为预设差值”。

三、关于创造性的论述
（一）修改后的权利要求1
1. 区别特征确认
将修改后的权利要求1与对比文件1对比，对比文件1公开了机架与输送带。
区别特征A：隔离罩设置在机架上，对比文件1未公开。
区别特征B：预设差值用于张紧轮调节，对比文件1未给出技术启示。
2. 实际解决的技术问题
如何在输送过程中避免物料偏移。
3. 非显而易见性论证
第一，对比文件1不存在设置隔离罩的技术启示。

四、结论
综上所述，修改后的权利要求1具备突出的实质性特点和显著的进步。
";

    let r = gate_amendment_text(LETTER_REAL_SHAPE, ORIGINAL);
    let section_two = LETTER_REAL_SHAPE
        .find("二、关于权利要求的修改")
        .expect("夹具须含修改稿小节");
    let section_three = LETTER_REAL_SHAPE
        .find("三、关于创造性的论述")
        .expect("夹具须含论述节");

    // 标记位置必须全部落在 [section_two, section_three) 之内（按 gated 文本与原信的对应关系，
    // 这里用"论述节切片里 0 标记"直接判，不依赖偏移换算）
    let gated_three = r
        .gated_text
        .split_once("三、关于创造性的论述")
        .map(|(_, tail)| tail)
        .expect("gated 文本须保留论述节标题");
    assert_eq!(
        marks(gated_three),
        0,
        "创造性论述节被打了 {} 个标记（＝修复前的整节吞并形态）",
        marks(gated_three)
    );
    assert_eq!(
        marks(&r.gated_text),
        r.unsupported_count,
        "标记数与计数不一致"
    );
    assert!(
        r.unsupported_count >= 1,
        "修改稿里「隔离罩/预设差值」这类零命中措辞必须仍被抓到（不得收紧成漏检）"
    );
    assert!(
        r.unsupported
            .iter()
            .all(|u| !u.text.contains("非显而易见") && !u.text.contains("技术启示")),
        "论证句不得进零命中清单: {:?}",
        r.unsupported
    );
    assert!(section_two < section_three);
}

/// 验收 6（第二处静默漏检）：块内**第一个「权利要求N」标题之前**的正文也必须送检。
///
/// 真实答复书常写成 `1. 权利要求1的修改：\n在保留原权利要求1全部特征（含……）的基础上，
/// 增加隔离罩。` ——带前导序号的写法不匹配权项标题正则；而后面若出现一行规范的
/// `权利要求2：`，旧 `extract_claims` 就会**只从那一行开始扫**，把前面"要改什么"那段整段丢掉。
/// 结果：136 收敛成 0 命中，真缺陷反而看不见（把过火修成假绿）。
#[test]
fn text_before_first_claim_header_is_still_scanned() {
    const LETTER_MIXED_LABELS: &str = "\
=== 意见陈述书 ===

二、关于权利要求的修改
1. 权利要求1的修改：
在保留原权利要求1全部特征的基础上，增加设置于机架上的隔离罩与防尘帘。

权利要求2：
一种物料输送装置，包括机架，机架上设有输送带。

三、关于创造性的论述
本申请具备创造性。
";
    let r = gate_amendment_text(LETTER_MIXED_LABELS, ORIGINAL);
    assert!(
        r.unsupported_count >= 1,
        "第一个权项标题之前的零命中措辞必须被抓到，实际 unsupported_count={}",
        r.unsupported_count
    );
    assert!(
        r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
        "「隔离罩」所在特征句必须进清单: {:?}",
        r.unsupported
    );
    // 规范权项标题那一段照常按权项归号（不是靠"整块当一个假权项"蒙对）
    assert!(
        r.unsupported.iter().any(|u| u.claim_no == "未编号"),
        "前缀段应以 未编号 归号: {:?}",
        r.unsupported
    );
    // 标题行本身（`1. 权利要求1的修改：`）不得被当成特征句打标
    assert!(
        !r.gated_text
            .contains(&format!("{UNVERIFIED_MARK}1. 权利要求1的修改：")),
        "小标题行不是特征句: {}",
        r.gated_text
    );
}
