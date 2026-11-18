use super::*;

// 通用技术词夹具（无申请号/人名/公开号/案件专有长句）
const SPEC: &str = "\
## 说明书\n\
### 技术领域\n\
本发明涉及一种物料输送装置。\n\
### 具体实施方式\n\
[0001] 一种物料输送装置，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。\n\
[0002] 输送带（2）上方设有导料板（4），导料板（4）与输送带（2）之间的间距为20～30厘米。\n\
[0003] 驱动电机（3）的输出轴与输送带（2）的主动辊通过链条连接，链条外侧设有张紧轮（5）。\n\
[0004] 机架（1）底部设有可调支脚，可调支脚的调节行程不超过50毫米。\n";

fn gate(letter: &str) -> AmendmentGateResult {
    gate_amendment_text(letter, SPEC)
}

// ── 正控：五类合法撰写变换各一条（修复前后均必须 ok，证明不误伤）──

#[test]
fn positive_control_1_pronoun_added_or_removed() {
    // ① 指代词增删："所述"在原文有、修改稿去掉（原文侧归一化同样去掉）
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count, 0,
        "指代词增删不得误伤: {:?}",
        r.unsupported
    );
    assert!(r.gated_text.contains("一种物料输送装置"));
}

#[test]
fn positive_control_2_connective_and_word_order() {
    // ② 权项连接语与语序：其特征在于 / 根据权利要求X所述的 / 还包括 的位置变化
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求2：\n根据权利要求1所述的物料输送装置，其特征在于，还包括导料板（4），导料板（4）与输送带（2）之间的间距为20至30厘米。\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count, 0,
        "连接语/语序变化不得误伤: {:?}",
        r.unsupported
    );
}

#[test]
fn positive_control_3_reference_signs_and_brackets() {
    // ③ 附图标记与中英文括号：修改稿去掉 (1)/(2)、保留全角括号
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机。\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count, 0,
        "附图标记/括号差异不得误伤: {:?}",
        r.unsupported
    );
}

#[test]
fn positive_control_4_punctuation() {
    // ④ 标点：修改稿用分号/无标点，原文用逗号/句号
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求2：\n输送带上方设有导料板;导料板与输送带之间的间距为20~30厘米。\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count, 0,
        "标点差异不得误伤: {:?}",
        r.unsupported
    );
}

#[test]
fn positive_control_5_number_range_and_units() {
    // ⑤ 数值范围符号与单位全半角：～/至/-，全角数字与符号
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求3：\n驱动电机的输出轴与输送带的主动辊通过链条连接，链条外侧设有张紧轮（5），调节行程不超过５０毫米。\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count, 0,
        "数值范围/单位全半角差异不得误伤: {:?}",
        r.unsupported
    );
}

#[test]
fn positive_control_6_dependent_claim_merged_into_independent() {
    // 从属权项并入独立权项（合法改写）：权2/权3 的特征在原文里都在
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3），输送带（2）上方设有导料板（4），链条外侧设有张紧轮（5）。\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count, 0,
        "从属权项并入独立权项不得误伤: {:?}",
        r.unsupported
    );
}

// ── 负控：零命中却声称"明确记载" ──

#[test]
fn zero_hit_feature_is_marked_in_place() {
    let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），其特征在于，还包括设置在机架上的隔离罩，隔离罩与输送带之间的间隙小于预设差值。\n\n\
上述修改均来源于原说明书具体实施方式的明确记载，未超出原始申请文件记载的范围。\n";
    let r = gate(letter);
    assert_eq!(r.status, GateStatus::Degraded);
    assert!(r.unsupported_count >= 1, "零命中必须被抓住: {r:?}");
    assert!(
        r.gated_text
            .contains("【未证实：该措辞在原始文本零命中】还包括设置在机架上的隔离罩"),
        "就地标注在特征句前: {}",
        r.gated_text
    );
    assert!(
        r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
        "unsupported 清单须点名措辞: {:?}",
        r.unsupported
    );
    assert!(!r.submission_ready, "有零命中时提交度维度必须压死");
}

#[test]
fn amendment_act_narration_is_meta_but_real_feature_still_checked() {
    // 实际场景（复杂修改稿复跑时抓到）：说明"本次把权2并入权1"的句子描述的是修改动作，
    // 不是修改后权利要求里的技术特征；它进了逐字核验就会把达标的稿子压成不可交。
    let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），机架（1）上设有输送带（2）。\n\
本次修改系将原权利要求2的附加技术特征并入权利要求1\n\
机架侧面还固定有气动夹爪组件与压力传感器\n";
    let r = gate(letter);
    assert_eq!(
        r.unsupported_count,
        1,
        "叙述句不该计数、真编造特征句必须仍被抓到： {:?}",
        r.unsupported
            .iter()
            .map(|u| u.text.chars().count())
            .collect::<Vec<_>>()
    );
    assert!(
        !r.gated_text.contains("】本次修改系将"),
        "修改动作叙述句不得被就地标注: {}",
        r.gated_text
    );
}

#[test]
fn supported_features_are_not_marked() {
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架（1），机架上设有输送带（2）。\n";
    let r = gate(letter);
    assert_eq!(r.status, GateStatus::Verified);
    assert!(!r.gated_text.contains(UNVERIFIED_MARK));
    assert!(r.submission_ready);
}

#[test]
fn assertion_downgraded_on_zero_hit() {
    let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），机架上设有伸缩轴行程限位块。\n\n\
本修改来源于原说明书具体实施方式的明确记载。\n";
    let r = gate(letter);
    assert_eq!(r.unsupported_count, 1, "{:?}", r.unsupported);
    assert_eq!(r.assertions_downgraded, 1);
    assert!(
        !r.gated_text.contains("来源于原说明书"),
        "零命中后不得保留出处断言原句: {}",
        r.gated_text
    );
    assert!(
        r.gated_text.contains(ASSERTION_DOWNGRADE_BLOCK),
        "断言必须整句替换为降级标注: {}",
        r.gated_text
    );
}

#[test]
fn no_amendment_block_is_noop() {
    let r = gate("## 一、关于审查意见的答复\n\n申请人认为对比文件未公开该特征。\n");
    assert_eq!(r.status, GateStatus::NoAmendment);
    assert_eq!(r.unsupported_count, 0);
}

#[test]
fn missing_original_skips_gate_without_accusing() {
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架。\n";
    let r = gate_amendment_text(letter, "");
    assert_eq!(r.status, GateStatus::NoOriginal);
    assert_eq!(r.unsupported_count, 0, "缺原文不得凭空判零命中");
    assert_eq!(r.gated_text, letter);
}

#[test]
fn annotation_keeps_original_sentence_intact() {
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，还包括设置在机架上的隔离罩。\n";
    let r = gate(letter);
    assert!(r.gated_text.contains("还包括设置在机架上的隔离罩"));
    assert_eq!(r.gated_text.matches(UNVERIFIED_MARK).count(), 1);
}

#[test]
fn corpus_keeps_claims_and_spec_paragraphs_separate() {
    let original = "权利要求书\n1. 一种物料输送装置，包括机架。\n2. 根据权利要求1所述的装置，其特征在于，机架上设有输送带。\n说明书\n技术领域\n本发明涉及输送设备。\n具体实施方式\n[0001] 机架上设有输送带，输送带上方设有导料板。\n";
    let corpus = build_own_application_corpus(original);
    assert_eq!(corpus.docs.len(), 1);
    assert_eq!(corpus.docs[0].doc_id, OWN_DOC_ID);
    let claims = corpus.docs[0]
        .sections
        .iter()
        .filter(|s| s.kind == LocatorKind::Claim)
        .count();
    assert_eq!(claims, 2, "权利要求书应拆成 2 条权项三元组");
    let paras = corpus.docs[0]
        .sections
        .iter()
        .filter(|s| s.kind == LocatorKind::Paragraph)
        .count();
    assert!(paras >= 1, "具体实施方式应至少 1 段: {corpus:?}");
}

#[test]
fn halfwidth_and_range_symbols_are_equivalent() {
    let a = strip_legal_variation("间距为２０～３０ｃｍ");
    let b = strip_legal_variation("间距为20-30cm");
    assert_eq!(a, b);
    let c = strip_legal_variation("调节行程20至30毫米");
    let d = strip_legal_variation("调节行程20-30毫米");
    assert_eq!(c, d);
    // "至少"不得被当成范围号
    assert!(strip_legal_variation("至少一个").contains("至少"));
}

// ── 作用域：marker 只在行首标签位才开块（过火回归守卫）──

#[test]
fn prose_mention_of_marker_does_not_open_block() {
    // 真实过火形态①：正文叙述句里提到"修改后的权利要求1-3"，句中含逗号句号
    let text = "申请人认为修改后的权利要求1-3已克服审查意见指出的全部缺陷，具体理由如下。\n";
    assert!(
        amendment_blocks(text).is_empty(),
        "叙述句里的 marker 不得开块: {:?}",
        amendment_blocks(text)
    );
}

#[test]
fn argumentation_subheading_does_not_open_block() {
    // 真实过火形态②（v4 实测的主因）：`（一）修改后的权利要求1` 是**创造性论述**的小标题，
    // 下面跟的是区别特征比对与论证，不是权项文本；当起点会把整节论述打满标记。
    let text = "\
三、关于创造性的论述

（一）修改后的权利要求1

1. 区别特征确认
将修改后的权利要求1与D1对比，D1公开了管道与电动执行器。
区别特征A：计算机控制器内设有PLC程序，D1未公开。
";
    assert!(
        amendment_blocks(text).is_empty(),
        "论述小标题不得开块: {:?}",
        amendment_blocks(text)
    );
}

#[test]
fn label_forms_still_open_block() {
    // 四种真标签写法都必须继续开块（否则就是将过火修成漏检）
    let cases = [
        "修改后的权利要求1：\n一种物料输送装置，包括机架。\n",
        "**修改后的权利要求 1**：\n一种物料输送装置，包括机架。\n",
        "（二）关于权利要求的修改\n修改后的权利要求书：\n1. 一种物料输送装置，包括机架。\n",
        "## 修改后的权利要求1-3：\n一种物料输送装置，包括机架。\n",
    ];
    for c in cases {
        assert!(!amendment_blocks(c).is_empty(), "标签写法未被识别: {c:?}");
    }
}

#[test]
fn banner_and_body_marker_counts_agree() {
    // 交付文本 = 横幅 + gated；整段里 UNVERIFIED_MARK 的字面量数必须等于 unsupported_count。
    // （修复前横幅自带一次字面量，读者数出 N+1——实测长文本 136 vs 137）
    let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），其特征在于，还包括设置在机架上的隔离罩。\n";
    let r = gate(letter);
    assert_eq!(r.unsupported_count, 1, "{:?}", r.unsupported);
    let delivered = format!("{}{}", r.banner(), r.gated_text);
    assert_eq!(
        delivered.matches(UNVERIFIED_MARK).count(),
        r.marked_count,
        "横幅计数与正文标记数必须一致: {delivered}"
    );
    assert_eq!(
        r.marked_count,
        r.gated_text.matches(UNVERIFIED_MARK).count(),
        "marked_count 必须是从成品里数出来的"
    );
}

#[test]
fn marked_count_is_the_number_a_reader_can_count() {
    // 实测用例：24 条零命中特征句，正文只有 17 处标注（同位置重叠合并）。
    // 只报条数时读者数一遍就对不上，会直接不信整张表。
    let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），还包括虚构的磁悬浮导轨与虚构的量子阀。\n";
    let r = gate(letter);
    assert!(r.unsupported_count > 0);
    assert_eq!(
        r.marked_count,
        r.gated_text.matches(UNVERIFIED_MARK).count(),
        "marked_count 必须等于正文里能数出来的处数"
    );
    let delivered = format!("{}{}", r.banner(), r.gated_text);
    assert_eq!(
        delivered.matches(UNVERIFIED_MARK).count(),
        r.marked_count,
        "横幅里不得再带一次字面量"
    );
    if r.unsupported_count != r.marked_count {
        assert!(
            r.banner().contains("正文实际标注"),
            "条数与处数不等时横幅必须说明: {}",
            r.banner()
        );
    }
}

// ── issue #142：真实版式的修改稿（带前导序号小标题 + 跨行引号全文）──

/// 跨行引号必须被接住：`具体修改为：` 换行后给 `“一种……”`。
/// 节内叙述句（"为进一步突出…""缩短审查周期"）**不得**被当特征句打标；
/// 引号内权项全文才是核验对象，其中真正无原文依据的措辞必须被抓到。
#[test]
fn multiline_quoted_claim_is_scanned_and_narrative_is_not() {
    let letter = "\
=== 意见陈述书 ===

二、关于权利要求的修改
1. 权利要求1的修改说明：
为进一步突出本发明的实质性技术贡献，缩短审查周期，申请人对权利要求书进行了限定性修改，具体修改为：
“一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机，且设有隔离罩。”

三、关于创造性的论述
本申请具备创造性。
";
    let r = gate_amendment_text(letter, SPEC);
    // 叙述句零标记：整封信里只允许出现引号段内部的标记
    let narrative_zone = r
        .gated_text
        .split_once('“')
        .map(|(head, _)| head)
        .unwrap_or("");
    assert_eq!(
        narrative_zone.matches(UNVERIFIED_MARK).count(),
        0,
        "引号之前的叙述句不得被标注:\n{}",
        r.gated_text
    );
    assert!(
        r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
        "引号内权项全文的零命中措辞必须被抓到: {:?}",
        r.unsupported
    );
    assert!(
        r.unsupported.iter().all(|u| u.claim_no == "引号内"),
        "本例核验对象应全部来自引号段: {:?}",
        r.unsupported
            .iter()
            .map(|u| u.claim_no.clone())
            .collect::<Vec<_>>()
    );
}

/// 既无行首权项标题、也无引号段 ⇒ **保留**旧回退（整块当一个权项继续粗筛）。
/// 这是刻意选择：宁可多标也不静默放过——修改稿哪怕写得不成样子，33 条核验也不能
/// 变成"没找到正文所以通过"。与 issue #142 原文里"返回 skipped"的设想相反，
/// 已在 issue 下注明取舍理由。
#[test]
fn block_without_header_or_quote_still_falls_back() {
    let letter = "## 二、关于权利要求的修改\n\n在保留原权利要求1全部特征的基础上，增加设置于机架上的隔离罩。\n";
    let r = gate_amendment_text(letter, SPEC);
    assert!(
        !r.unsupported.is_empty(),
        "无标题无引号时仍须回退整块核验，不得静默放过"
    );
    assert!(
        r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
        "回退路径必须仍能抓到零命中措辞: {:?}",
        r.unsupported
    );
}

/// 引号段长度上限从 300 提到 1200：真实独立权项普遍超过 300 字，
/// 旧上限会把权项全文截断成半句，导致核验对象错位。
#[test]
fn long_quoted_claim_is_not_truncated() {
    let long_claim = format!(
        "一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机，{}，并设有隔离罩。",
        "导料板与输送带之间的间距为20至30厘米，".repeat(12)
    );
    assert!(
        long_claim.chars().count() > 300,
        "夹具须超过旧上限 300 字才有意义"
    );
    let letter = format!(
        "## 二、关于权利要求的修改\n\n修改后的权利要求书如下：\n具体修改为：\n“{long_claim}”\n"
    );
    let ranges = quoted_amendment_ranges(&letter);
    assert_eq!(ranges.len(), 1, "长引号段应被完整捕获为一处");
    let captured = letter.get(ranges[0].0..ranges[0].1).unwrap_or("");
    assert!(
        captured.ends_with("并设有隔离罩。"),
        "捕获内容必须到引号结尾，实际尾部: …{}",
        captured.chars().rev().take(12).collect::<String>()
    );
}

// ── issue #140：语料必须认得本应用自己的 JSON 形态 my_patent ──

/// JSON 形态（主 UI 与答复书网关实际传的就是这种）：权项正文必须进语料。
#[test]
fn json_shaped_input_indexes_claims() {
    let json = concat!(
        r#"{"title":"一种物料输送装置","abstract_text":"本发明涉及输送设备。","#,
        r#""claims":"1.一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机。\n"#,
        r#"2.根据权利要求1所述的装置，其特征在于，输送带上方设有导料板。","#,
        r#""description":"具体实施方式\n[0001] 机架上设有导料板，导料板与输送带之间的间距为20～30厘米。"}"#
    );
    let corpus = build_own_application_corpus(json);
    let claims = claim_section_count(&corpus);
    assert!(
        claims >= 2,
        "JSON 的 claims 应拆出 ≥2 条权项三元组，实际 {claims}"
    );
    // 逐字取自 claims 的修改稿不得被判零命中（v5 实测误报 30 处的根因）
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机。\n";
    let r = gate_amendment_text(letter, json);
    assert_eq!(r.corpus_claims, claims, "corpus_claims 必须与语料一致");
    assert_eq!(
        r.unsupported_count,
        0,
        "逐字来自原权利要求书的修改稿不得被误判: {:?} / 标记 {}",
        r.unsupported,
        r.gated_text.matches(UNVERIFIED_MARK).count()
    );
    assert!(
        r.skip_note().is_empty(),
        "有权项语料时不得再挂缺基准说明: {}",
        r.skip_note()
    );
}

/// 只有说明书、没有权项 ⇒ 必须显式否认"已核验"，不得让读者以为比过了原文。
#[test]
fn corpus_without_claims_is_visible_not_silent() {
    let spec_only =
        "说明书\n技术领域\n本发明涉及输送设备。\n具体实施方式\n[0001] 机架上设有输送带。\n";
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，其特征在于，机架上设有输送带，且设有隔离罩。\n";
    let r = gate_amendment_text(letter, spec_only);
    assert_eq!(r.corpus_claims, 0, "纯说明书输入不应有权项三元组");
    assert!(
        r.skip_note().contains("未索引到任何权利要求"),
        "语料缺权项时必须显式说明，实际: {:?}",
        r.skip_note()
    );
}

/// `claims_under_review` 是别名键，也必须认得。
#[test]
fn json_alias_claims_under_review_is_recognised() {
    let json = r#"{"claims_under_review":"1.一种物料输送装置，包括机架。","description":"具体实施方式\n[0001] 机架上设有输送带。"}"#;
    let corpus = build_own_application_corpus(json);
    assert!(
        claim_section_count(&corpus) >= 1,
        "别名键 claims_under_review 必须被认得"
    );
}

/// 不是 JSON（或 JSON 里没有可用字段）时不得瞎猜，按原文继续。
#[test]
fn non_json_input_falls_back_unchanged() {
    assert!(
        flatten_json_application(SPEC).is_none(),
        "标题式纯文本不该被当 JSON"
    );
    assert!(
        flatten_json_application(r#"{"title":"只有标题"}"#).is_none(),
        "没有 claims/description 的 JSON 不得凭空造语料"
    );
}

#[test]
fn skip_note_only_for_unverified_statuses() {
    let no_section = gate("## 一、关于审查意见的答复\n\n申请人认为对比文件未公开该特征。\n");
    assert_eq!(no_section.status, GateStatus::NoAmendment);
    assert!(no_section.skip_note().contains("未定位到"));
    assert_eq!(no_section.status.as_str(), "skipped_no_amendment_section");

    let no_original = gate_amendment_text(
        "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架。\n",
        "",
    );
    assert!(!no_original.skip_note().is_empty());

    let verified =
        gate("## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架。\n");
    assert_eq!(verified.status, GateStatus::Verified);
    // SPEC 夹具只有说明书、没有「权利要求书」分节 ⇒ 语料无权项，
    // 新行为要求此时挂"语料缺权项"说明（#140），但**不得**挂"未定位到修改稿"那种
    // 与事实相反的说明——两者是不同的可见性通道，这里分别钉住。
    assert!(
        !verified.skip_note().contains("未定位到"),
        "已找到修改稿时不得声称未定位到: {}",
        verified.skip_note()
    );
    assert_eq!(
        verified.corpus_claims, 0,
        "SPEC 夹具无权利要求书分节，corpus_claims 应为 0"
    );
    assert!(
        verified.skip_note().contains("未索引到任何权利要求"),
        "语料缺权项时即使判 verified 也必须挂缺基准说明: {:?}",
        verified.skip_note()
    );
}

#[test]
fn prompt_block_flattens_json_so_model_sees_what_the_gate_compares() {
    let json = serde_json::json!({
        "claims": "1. 一种物料输送装置，包括机架。",
        "description": "[0001] 机架上设有输送带。"
    })
    .to_string();
    let block = original_application_block(&json);
    assert!(block.contains("权利要求书"), "{block}");
    assert!(block.contains("一种物料输送装置"), "{block}");
    assert!(block.contains("[0001] 机架上设有输送带。"), "{block}");
    // 与门比对用的归一化必须是同一份
    assert_eq!(
        block,
        flatten_json_application(&json).expect("JSON 形态应能摊平")
    );
}

#[test]
fn prompt_block_keeps_plain_text_as_is() {
    let plain = "权利要求书\n1. 一种物料输送装置，包括机架。\n说明书\n[0001] 机架上设有输送带。\n";
    assert_eq!(original_application_block(plain), plain.trim());
}

#[test]
fn prompt_block_missing_original_is_visible_not_empty() {
    // 提示词里留空，模型就会自己编修改稿；必须是看得见的"未提供"
    let block = original_application_block("   \n  ");
    assert!(block.contains("未提供原始申请文本"), "{block}");
    assert!(
        block.contains("不得输出任何「修改后的权利要求」正文"),
        "缺失时必须明令禁止编修改稿: {block}"
    );
}

// ── issue #160：不修改答复书被整节当修改稿核验（边界场景）──

#[test]
fn no_amendment_declaration_is_not_treated_as_amendment_draft() {
    // 真实形态：标题按模板写「二、关于权利要求的修改」，本节讲的是"为什么不改"。
    // 修复前：本节整段被当修改稿切句，论述句逐句零命中，合法稿被压成 graded/degraded。
    let letter = "\
## 二、关于权利要求的修改

本次答复不修改权利要求。

申请人认为，现行权利要求1已以电缸直行程限定与D1的区别。
通知书认定的三项区别特征并非彼此独立的单项替换。
";
    let r = gate(letter);
    assert_eq!(r.status, GateStatus::NoAmendment);
    assert_eq!(r.unsupported_count, 0, "论述句不该被计成零命中特征");
    assert_eq!(
        r.gated_text.matches(UNVERIFIED_MARK).count(),
        0,
        "正文不得出现标注：{}",
        r.gated_text
    );
    assert!(!r.submission_ready, "没核验就不能说可交");
}

#[test]
fn declaration_of_no_amendment_does_not_hide_an_actual_claim_draft() {
    // 负控：声明"不修改权利要求2"但确实贴了修改后的权利要求1 ⇒ 必须继续核验，
    // 不能因为句子里有"不修改"就整节跳过。
    let letter = "\
## 二、关于权利要求的修改

本次不修改权利要求2，修改后的权利要求1：

1.一种物料输送装置，机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。
";
    let r = gate(letter);
    assert_ne!(
        r.status,
        GateStatus::NoAmendment,
        "带修改稿的节不得被判为无修改稿"
    );
    assert!(r.checked_feature_count > 0, "必须真的核验了特征句");
}

#[test]
fn unverified_statuses_are_never_submission_ready() {
    // 假绿灯：`submission_ready` 曾经写成 `status != Degraded`，
    // 于是"未定位到修改稿""缺原始文本"这两种**没核验**的状态都返回 true。
    let no_section = gate("## 一、关于审查意见的答复\n\n申请人认为对比文件未公开该特征。\n");
    assert_eq!(no_section.status, GateStatus::NoAmendment);
    assert!(!no_section.submission_ready, "未定位到修改稿 ≠ 可交");

    let no_original = gate_amendment_text(
        "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架（1）。\n",
        "",
    );
    assert_eq!(no_original.status, GateStatus::NoOriginal);
    assert!(!no_original.submission_ready, "缺原始文本 ≠ 可交");

    let verified = gate(
        "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架（1）上设有输送带（2）。\n",
    );
    assert_eq!(verified.status, GateStatus::Verified);
    assert!(verified.submission_ready, "核验通过才可交");
}

// ── issue #158：门不幂等（重跑把标注本身算成新零命中；旧横幅留在产物里）──

#[test]
fn gate_is_idempotent_over_its_own_output() {
    let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架上设有虚构的磁悬浮导轨。\n";
    let once = gate(letter);
    assert!(once.unsupported_count > 0, "这个夹具本来就该零命中");
    let twice = gate(&once.gated_text);
    assert_eq!(
        twice.unsupported_count, once.unsupported_count,
        "重跑不得把上一轮的标注算成新的零命中特征"
    );
    assert_eq!(
        twice.gated_text.matches(UNVERIFIED_MARK).count(),
        once.gated_text.matches(UNVERIFIED_MARK).count(),
        "标注处数必须一致（旧行为是 1 → 2）"
    );
}

#[test]
fn stale_banner_is_stripped_so_the_artifact_stops_contradicting_itself() {
    // 真实形态：手改过的稿子正文已干净，首行旧横幅还在，写着"核验阻断 28 处"。
    let clean = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架（1）上设有输送带（2）。\n";
    let stale = format!("> ⚠️ 修改稿核验阻断（amendment_unsupported=28）：旧的一次。\n\n{clean}");
    let r = gate(&stale);
    assert_eq!(r.status, GateStatus::Verified);
    assert_eq!(r.unsupported_count, 0);
    assert!(
        !r.gated_text.contains("修改稿核验阻断"),
        "旧横幅必须剥掉，否则产物与新结论自相矛盾：{}",
        r.gated_text
    );
}

// ── 复核补漏（第三方复核命中）：括号编号的权项行也必须算"有修改稿"──

#[test]
fn no_amendment_declaration_with_parenthesised_claim_draft_still_verified() {
    // 声明"不作修改"，但正文确实贴了（3）一种… 的修改稿。
    // 修复前：编号只认 `3.一种`，括号编号不命中 ⇒ 整节被当"无修改稿"跳过（静默漏检）。
    let letter = "## 二、关于权利要求的修改

本次对独立权利要求1不作修改。

（3）一种物料输送装置，机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。
";
    let r = gate(letter);
    assert_ne!(
        r.status,
        GateStatus::NoAmendment,
        "括号编号的修改稿不得被当成\"无修改稿\"跳过：{}",
        r.gated_text
    );
    assert!(r.checked_feature_count > 0, "必须真的核验了特征句");
}

#[test]
fn plain_numbered_claim_draft_is_still_recognised() {
    // 负控的另一半：阿拉伯点号编号仍要认（改动不能把常见形态弄坏）。
    let letter = "## 二、关于权利要求的修改

本次对独立权利要求1不作修改。

3.一种物料输送装置，机架（1）上设有输送带（2）。
";
    let r = gate(letter);
    assert_ne!(
        r.status,
        GateStatus::NoAmendment,
        "3.一种… 必须仍被识别为修改稿"
    );
}

// ── issue #159：删限定＝扩大保护范围（现权项当基准）──

#[test]
fn deleting_a_limitation_is_flagged_as_broadening() {
    let current = "1.一种物料输送装置，其特征在于，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有防护罩（3）。
";
    let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，包括机架（1），机架（1）上设有输送带（2）。
";
    let r = gate_amendment_text_with_current_claims(letter, SPEC, current);
    assert!(
        !r.broadened.is_empty(),
        "删掉「防护罩」必须被报成扩范围，实际 broadened={:?}",
        r.broadened
    );
    assert_eq!(r.status, GateStatus::Degraded);
    assert!(!r.submission_ready, "扩范围不得判可交");
    assert!(
        r.direction_note().contains("扩范围"),
        "{}",
        r.direction_note()
    );
}

#[test]
fn adding_a_limitation_alone_is_not_broadening() {
    let current = "1.一种物料输送装置，其特征在于，包括机架（1），机架（1）上设有输送带（2）。
";
    let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。
";
    let r = gate_amendment_text_with_current_claims(letter, SPEC, current);
    assert!(
        r.broadened.is_empty(),
        "只增加限定不算扩范围：{:?}",
        r.broadened
    );
    assert!(r.current_claims_checked);
    assert!(
        !r.direction_note().contains("扩范围"),
        "只加不减不该报扩范围"
    );
}

#[test]
fn missing_current_claims_is_visible_not_silent() {
    let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，机架（1）上设有输送带（2）。
";
    let r = gate(letter);
    assert!(r.broadened.is_empty());
    assert!(
        !r.current_claims_checked,
        "没给现权项时必须是未核验，而不是悄悄当通过"
    );
    assert!(
        r.direction_note().contains("未核验"),
        "缺输入的可见性说明缺失: {}",
        r.direction_note()
    );
}

// ── 差额拆两块：#156 提的 unlocated，与"重叠合并"分开报 ──

#[test]
fn shortfall_is_split_into_merged_and_unlocated() {
    let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，包括机架（1），还包括虚构的量子阀。
";
    let r = gate(letter);
    assert_eq!(
        r.marked_count + r.merged_count + r.unlocated_count,
        r.unsupported_count,
        "标注 + 合并 + 未定位 必须等于零命中条数：{:?}",
        r
    );
}

#[test]
fn banner_names_unlocated_separately_from_merged() {
    // 只钉文案：把两个计数分别置位，确认横幅不会把"没定位到"说成"重叠合并"。
    let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，包括虚构部件。
";
    let mut r = gate(letter);
    assert!(r.unsupported_count > 0);
    r.merged_count = 1;
    r.unlocated_count = 2;
    let b = r.banner();
    assert!(b.contains("合并为一处标注"), "合并要有自己的说法: {b}");
    assert!(b.contains("没能定位"), "未定位必须单独说: {b}");
}
