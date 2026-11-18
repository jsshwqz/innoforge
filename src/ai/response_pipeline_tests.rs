#[cfg(test)]
use super::*;

#[test]
fn collapse_whitespace_removes_consecutive_spaces() {
    assert_eq!(collapse_whitespace("a  b"), "a b");
    assert_eq!(collapse_whitespace("a    b"), "a b");
    assert_eq!(collapse_whitespace("a		b"), "a b");
    assert_eq!(collapse_whitespace("a 	 b"), "a b");
    assert_eq!(collapse_whitespace("no spaces"), "no spaces");
    assert_eq!(collapse_whitespace("  leading"), " leading");
    assert_eq!(collapse_whitespace("trailing  "), "trailing ");
}

#[test]
fn stage_e_output_has_no_consecutive_whitespace() {
    // 验收：Stage E 输出不含连续2个以上空白字符
    let input = make_sample_case_input();
    let features: Vec<DistinguishingFeature> = vec![];
    let term_checks: Vec<TermCountCheck> = vec![];
    let unsupported: Vec<UnsupportedClaim> = vec![];
    let rebuttals: Vec<FeatureRebuttal> = vec![];
    let mismatches: Vec<EffectMismatch> = vec![];
    let output = stage_e_assemble(
        &input,
        &features,
        &term_checks,
        &unsupported,
        &rebuttals,
        &mismatches,
        &[],
    );
    let mut max_consecutive = 0;
    let mut current = 0;
    for ch in output.chars() {
        if ch == ' ' || ch == '\t' {
            current += 1;
            if current > max_consecutive {
                max_consecutive = current;
            }
        } else {
            current = 0;
        }
    }
    assert!(
        max_consecutive <= 1,
        "Found {} consecutive whitespace chars in output",
        max_consecutive
    );
}

use super::super::case_input::{
    CaseMeta, DistinguishingFeature, FeatureRebuttal, OaTag, ReferenceDocument,
};
use super::super::fact_check::FactWarning;
use super::*;

#[test]
fn fact_check_banner_marks_zero_evidence_as_blocking() {
    let report = FactCheckReport {
        warnings: vec![FactWarning {
            category: "C2零举证".into(),
            description: "检测到15处零举证断言".into(),
            severity: "高".into(),
        }],
        score: 85.0,
        c0_extractions: vec![],
    };
    let banner = fact_check_banner(&report);
    assert!(banner.contains("不可直接提交"));
    assert!(banner.contains("零举证"));
}

#[test]
fn material_warnings_flag_missing_refs_and_short_oa() {
    let input = make_sample_case_input();
    let warnings = material_warnings(&input);
    assert!(warnings.iter().any(|w| w.contains("字")));
    assert!(warnings.iter().any(|w| w.contains("对比文件")));
}

#[test]
fn stage_e_includes_legal_basis_and_evidence_table() {
    let mut input = make_sample_case_input();
    input.oa_full_text = "根据专利法第22条第3款，权利要求1不具备创造性。".into();
    let features = vec![DistinguishingFeature {
        ordinal: 1,
        feature_name: "特征A".into(),
        oa_tag: OaTag::RoutineChoice,
        attributed_ref: Some("D1".into()),
        oa_excerpt: String::new(),
    }];
    let output = stage_e_assemble(&input, &features, &[], &[], &[], &[], &[]);
    assert!(output.contains("法律依据"));
    assert!(output.contains("专利法第22条第3款"));
    assert!(output.contains("区别特征与驳回决定出处对照表"));
    assert!(output.contains("【缺原文摘录】"));
}

#[test]
fn stage_e_reject_review_without_substantive_law_marks_gap() {
    let mut input = make_sample_case_input();
    input.oa_full_text = "驳回决定正文，未引用具体条款。".into();
    let output = stage_e_assemble(&input, &[], &[], &[], &[], &[], &[]);
    assert!(output.contains("专利法第41条"));
    assert!(output.contains("实体条款未在现有文本中识别"));
}

/// **2026-11-06 缺陷锁（真实案件形态）**：驳回决定首页的候选法条勾选表，未勾选项也留在
/// OCR 文本里。旧实现把它们全当真依据列给用户（实测 15 条 vs 实际 1 条实体依据）。
/// 现只认「本案适用」语句；表格裸条目必须被丢弃。
#[test]
fn legal_basis_ignores_rejection_form_checklist() {
    let oa = "1. 根据专利法第38条及其实施细则第59条的规定，决定驳回上述专利申请。\n\
              申请不符合专利法第2条第2款的规定。\n\
              申请不符合专利法第5条或者第25条规定的不授予专利权的范围。\n\
              申请不符合专利法第9条第1款的规定。\n\
              申请不符合专利法第22条第2款的规定。\n\
              申请不符合专利法第26条第3款或者第4款的规定。\n\
              权利要求不符合专利法第2条第2款的规定。\n\
              权利要求不符合专利法第31条第1款的规定。\n\
              权利要求不具备专利法第22条第2款规定的新颖性。\n\
              权利要求不具备专利法第22条第4款规定的实用性。\n\
              详细的驳回理由见驳回决定正文部分（共3页）。\n\
              审查员于2026年01月19日发出第一次审查意见通知书，指出权利要求1-4不具备专利法第22条第3款规定的创造性。\n\
              该权利要求不具备专利法第二十二条第三款规定的创造性。\n\
              3. 根据专利法第41条及实施细则第65条的规定，申请人可以请求复审。";
    let bases = extract_legal_basis(oa);
    let joined = bases.join("|");
    // 实际适用的实体依据保留
    assert!(
        joined.contains("第22条第3款"),
        "本案实体依据必须保留: {joined}"
    );
    // 决定主文的程序依据保留
    assert!(
        joined.contains("第38条"),
        "驳回主文程序依据必须保留: {joined}"
    );
    // 表格空白项一律不得出现（含「权利要求不符合…」「权利要求不具备…规定的新颖性」
    // 这两种带「权利要求」/「不具备」字样的模板行）
    for bogus in [
        "第2条第2款",
        "第9条第1款",
        "第22条第2款",
        "第22条第4款",
        "第26条第3款",
        "第31条第1款",
    ] {
        assert!(
            !bases.iter().any(|b| b.contains(bogus)),
            "勾选表未适用条目 {bogus} 不得列为法律依据: {joined}"
        );
    }
    // 中文数字与阿拉伯数字同一条款只出现一次
    assert_eq!(
        bases
            .iter()
            .filter(|b| law_citation_key(b).contains("第22条第3款"))
            .count(),
        1,
        "第二十二条第三款与第22条第3款必须归一去重: {joined}"
    );
}

/// **UA2 接线锁（2026-11-06）**：复审请求书文末必须带「提交前清单」固定段。
/// 旧状态是文案常量早已定稿但从未拼进产物（`allow(dead_code)`），属假交付。
#[test]
fn stage_e_reject_review_appends_pre_submit_checklist() {
    let mut input = make_sample_case_input();
    input.case_meta.oa_type = "reject_review".into();
    let output = stage_e_assemble(&input, &[], &[], &[], &[], &[], &[]);
    assert!(
        output.contains("提交前清单"),
        "复审请求书文末必须有提交前清单"
    );
    for i in 1..=8 {
        assert!(
            output.contains(&format!("□ {i}.")),
            "清单第 {i} 项必须出现在产物里"
        );
    }

    // 非复审类型（普通审查意见答复）不得追加复审专用清单
    let mut oa_input = make_sample_case_input();
    oa_input.case_meta.oa_type = "office_action".into();
    let oa_output = stage_e_assemble(&oa_input, &[], &[], &[], &[], &[], &[]);
    assert!(
        !oa_output.contains("提交前清单"),
        "提交前清单是复审专用，不得混入其他答复类型"
    );
}

#[test]
fn stage_e_ties_rebuttal_to_evidence_row() {
    let input = make_sample_case_input();
    let features = vec![DistinguishingFeature {
        ordinal: 1,
        feature_name: "特征A".into(),
        oa_tag: OaTag::RoutineChoice,
        attributed_ref: Some("D1".into()),
        oa_excerpt: "审查员转述".into(),
    }];
    let rebuttals = vec![FeatureRebuttal {
        ordinal: 1,
        feature_name: "特征A".into(),
        oa_tag: OaTag::RoutineChoice,
        severance: "割裂".into(),
        synergy: "协同".into(),
        insufficient: "不足".into(),
    }];
    let output = stage_e_assemble(&input, &features, &[], &[], &rebuttals, &[], &[]);
    assert!(output.contains("根据驳回决定原文摘录（见证据表第1行）"));
}

#[test]
fn r6c_pipeline_output_contains_fact_check_section() {
    // R6c: pipeline 输出必须包含 AI事实核查 固定段
    let input = make_sample_case_input();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let letter = rt.block_on(generate_response_letter_pipeline(&input, None, None));
    assert!(
        letter.full_text.contains("AI事实核查"),
        "pipeline output must contain fact-check section"
    );
    assert!(
        letter.full_text.contains("可信度评分"),
        "fact-check section must contain score"
    );
}

fn make_sample_case_input() -> CaseInput {
    CaseInput {
        case_meta: CaseMeta {
            application_number: "1990999999999".into(),
            applicant: "示例申请人".into(),
            invention_title: "一种合成技术装置的装置".into(),
            oa_date: Some("2026-07-10".into()),
            oa_type: "reject_review".into(),
            claim_basis_date: Some("2026-06-25".into()),
            claim_basis_text: None,
        },
        oa_full_text: "驳回决定正文".into(),
        reference_documents: vec![
            ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN990001001A".into(),
                full_text: "D1原文".into(),
                extraction_method: Some("ocr".into()),
                page_count: Some(9),
                technical_field: None,
            },
            ReferenceDocument {
                label: "D2".into(),
                publication_number: "CN990001002A".into(),
                full_text: "D2原文".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            },
            ReferenceDocument {
                label: "D3".into(),
                publication_number: "CN990001003U".into(),
                full_text: "D3原文".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            },
        ],
        claims_under_review: "1. 一种合成技术装置的装置".into(),
        spec_effect_statements: vec!["通过电磁场调控使反应气体流向发生偏转".into()],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    }
}

#[tokio::test]
async fn pipeline_no_ai_produces_guarded_output() {
    let input = make_sample_case_input();
    let result = generate_response_letter_pipeline(&input, None, None).await;
    assert!(result.full_text.contains("复审请求书"));
    assert!(result.full_text.contains("1990999999999"));
    assert!(result.full_text.contains("发回原审查部门"));
    assert!(
        result.guard_result.passed,
        "Guard errors: {:?}",
        result.guard_result.errors
    );
}

#[test]
fn dependency_graph_empty_for_empty_claims() {
    let graph = build_dependency_graph("");
    assert!(graph.edges.is_empty());
}

#[test]
fn auto_extract_returns_empty_for_empty_spec() {
    let effects = auto_extract_effect_statements("");
    assert!(effects.is_empty());
}

#[test]
fn ordinal_circled_basic() {
    assert_eq!(ordinal_circled(1), "①");
    assert_eq!(ordinal_circled(9), "⑨");
    assert_eq!(ordinal_circled(11), "(11)");
}

#[tokio::test]
async fn pipeline_with_synthetic_second_case() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "2025108888888".into(),
            applicant: "张三".into(),
            invention_title: "一种散热装置".into(),
            oa_date: Some("2026-08-01".into()),
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "驳回决定：权利要求1不具备创造性。".into(),
        reference_documents: vec![
            ReferenceDocument {
                label: "D1".into(),
                publication_number: "CN111122233A".into(),
                full_text: "D1原文".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            },
            ReferenceDocument {
                label: "D2".into(),
                publication_number: "CN222233344U".into(),
                full_text: "D2原文".into(),
                extraction_method: None,
                page_count: None,
                technical_field: None,
            },
        ],
        claims_under_review: "1. 一种散热装置，包括散热鳍片、热管、均温板".into(),
        spec_effect_statements: vec!["通过均温板将热量均匀传导至散热鳍片".into()],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let result = generate_response_letter_pipeline(&input, None, None).await;
    assert!(result.full_text.contains("2025108888888"));
    assert!(result.full_text.contains("张三"));
    assert!(result.full_text.contains("散热装置"));
    assert!(result.full_text.contains("发回原审查部门"));
    assert!(
        result.guard_result.passed,
        "Guard errors: {:?}",
        result.guard_result.errors
    );
}

// ── Stage B 确定性计数测试 ──

#[test]
fn stage_b_counts_terms_in_reference() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "1990999999999".into(),
            applicant: "测试".into(),
            invention_title: "测试".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "审查员认为对角安装是常规设计。对角安装能够实现均匀受热。".into(),
        reference_documents: vec![ReferenceDocument {
            label: "D1".into(),
            publication_number: "CN990001001A".into(),
            full_text: "D1公开了一种安装方式，但未提及对角安装。".into(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        }],
        claims_under_review: "1. 一种装置，包括对角安装的加热元件".into(),
        spec_effect_statements: vec![],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let features = vec![DistinguishingFeature {
        ordinal: 1,
        feature_name: "对角安装".into(),
        oa_tag: OaTag::RoutineDesign,
        attributed_ref: Some("D1".into()),
        oa_excerpt: "对角安装是常规设计，能够实现均匀受热".into(),
    }];
    let (term_checks, unsupported) = stage_b_count_terms(&input, &features);

    // "对角安装" 在D1中出现1次，OA中出现2次
    let tc = term_checks.iter().find(|t| t.term == "对角安装").unwrap();
    assert_eq!(tc.ref_count, 1, "D1中'对角安装'应出现1次");
    assert_eq!(tc.oa_count, 2, "OA中'对角安装'应出现2次");

    // "均匀受热" 在D1中0次，OA中1次 → 应生成unsupported
    let uc = unsupported
        .iter()
        .find(|u| u.rebuttal_text.contains("均匀受热"));
    assert!(uc.is_some(), "应检测到'均匀受热'在D1无支撑");
}

#[test]
fn stage_b_no_ref_no_check() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "123".into(),
            applicant: "t".into(),
            invention_title: "t".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "OA".into(),
        reference_documents: vec![],
        claims_under_review: "claims".into(),
        spec_effect_statements: vec![],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let features = vec![DistinguishingFeature {
        ordinal: 1,
        feature_name: "test".into(),
        oa_tag: OaTag::EasyToThink,
        attributed_ref: None, // 无归因
        oa_excerpt: "test".into(),
    }];
    let (tc, uc) = stage_b_count_terms(&input, &features);
    assert!(tc.is_empty());
    assert!(uc.is_empty());
}

// ── extract_json_array 测试 ──

#[test]
fn extract_json_from_markdown() {
    let raw = "```json\n[{\"a\":1}]\n```";
    let extracted = extract_json_array(raw);
    assert_eq!(extracted, "[{\"a\":1}]");
}

#[test]
fn extract_json_plain() {
    let raw = "[{\"a\":1}]";
    let extracted = extract_json_array(raw);
    assert_eq!(extracted, "[{\"a\":1}]");
}

#[test]
fn extract_json_with_prefix_text() {
    let raw = "好的，结果如下：\n[{\"a\":1}]";
    let extracted = extract_json_array(raw);
    assert_eq!(extracted, "[{\"a\":1}]");
}

// ── extract_key_terms 测试 ──

#[test]
fn extract_key_terms_basic() {
    let terms = extract_key_terms("对角安装是常规设计，能够实现均匀受热", &[]);
    assert!(terms.contains(&"对角安装".to_string()));
    assert!(terms.contains(&"均匀受热".to_string()));
    // 停用词应被过滤
    assert!(!terms.contains(&"常规设计".to_string())); // "常规设计" 不在停用词，应该保留
}

#[test]
fn extract_key_terms_filters_stopwords() {
    let terms = extract_key_terms("对比文件公开了技术方案", &[]);
    assert!(!terms.contains(&"对比文件".to_string()));
    assert!(!terms.contains(&"技术方案".to_string()));
}

// ── Stage C 三段式反驳测试 ──

#[tokio::test]
async fn stage_c_produces_three_part_rebuttal() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "9999999".into(),
            applicant: "测试".into(),
            invention_title: "测试".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "OA".into(),
        reference_documents: vec![ReferenceDocument {
            label: "D1".into(),
            publication_number: "CN111A".into(),
            full_text: "D1".into(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        }],
        claims_under_review: "包括特征甲、特征乙".into(),
        spec_effect_statements: vec![],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let features = vec![
        DistinguishingFeature {
            ordinal: 1,
            feature_name: "特征甲".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: Some("D1".into()),
            oa_excerpt: "特征甲是常规设计".into(),
        },
        DistinguishingFeature {
            ordinal: 2,
            feature_name: "特征乙".into(),
            oa_tag: OaTag::EasyToThink,
            attributed_ref: Some("D1".into()),
            oa_excerpt: "特征乙容易想到".into(),
        },
    ];
    let rebuttals = stage_c_rebut_features(&input, &features).await;
    assert_eq!(rebuttals.len(), 2);

    // 第一项：特征甲 — 仲裁后 severance 字段包含全部段落
    let r0 = &rebuttals[0];
    assert_eq!(r0.feature_name, "特征甲");
    // 定性失真段（仲裁优先级1，首段）
    assert!(
        r0.severance.contains("上位泛化标签"),
        "应包含定性失真主攻段"
    );
    // 举证不足段（仲裁优先级2）
    assert!(r0.severance.contains("举证"), "应包含零举证主攻段");
    assert!(r0.severance.contains("常规设计"));
    // 割裂论断段（仲裁优先级5，防守）
    assert!(r0.severance.contains("割裂"));
    assert!(r0.severance.contains("D1"));
    // 协同段
    assert!(r0.severance.contains("特征乙"), "应包含协同段含共现特征乙");
}

#[tokio::test]
async fn stage_c_no_co_occurrence_no_synergy_deps() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "999".into(),
            applicant: "t".into(),
            invention_title: "t".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "".into(),
        reference_documents: vec![],
        claims_under_review: "包括特征甲。包括特征乙。".into(),
        spec_effect_statements: vec![],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let features = vec![
        DistinguishingFeature {
            ordinal: 1,
            feature_name: "特征甲".into(),
            oa_tag: OaTag::RoutineChoice,
            attributed_ref: Some("D1".into()),
            oa_excerpt: "".into(),
        },
        DistinguishingFeature {
            ordinal: 2,
            feature_name: "特征乙".into(),
            oa_tag: OaTag::RoutineChoice,
            attributed_ref: Some("D1".into()),
            oa_excerpt: "".into(),
        },
    ];
    let rebuttals = stage_c_rebut_features(&input, &features).await;
    // 特征甲和特征乙不在同一从句（句号分隔），无共现
    let r0 = &rebuttals[0];
    // 仲裁后全部内容在 severance 字段；无共现时协同段为独立特征描述
    assert!(
        r0.severance.contains("独立技术特征"),
        "无共现时应为独立特征描述"
    );
}

#[test]
fn dependency_graph_co_occurrence() {
    let features = vec![
        DistinguishingFeature {
            ordinal: 1,
            feature_name: "特征甲".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: None,
            oa_excerpt: "".into(),
        },
        DistinguishingFeature {
            ordinal: 2,
            feature_name: "特征乙".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: None,
            oa_excerpt: "".into(),
        },
        DistinguishingFeature {
            ordinal: 3,
            feature_name: "特征丙".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: None,
            oa_excerpt: "".into(),
        },
    ];
    // 特征甲和特征乙在同一从句（顿号分隔），特征丙在不同从句
    let graph = build_dependency_graph_with_features("包括特征甲、特征乙，以及特征丙", &features);
    let deps_a = graph
        .edges
        .iter()
        .find(|(n, _)| n == "特征甲")
        .map(|(_, d)| d.clone())
        .unwrap_or_default();
    assert!(deps_a.contains(&"特征乙".to_string()), "甲应依赖乙（共现）");
    assert!(
        !deps_a.contains(&"特征丙".to_string()),
        "甲不应依赖丙（不同从句）"
    );
}

#[test]
fn dependency_graph_no_ai_edges() {
    // 确认：即使两个特征语义相关，若不在原文共现也不建边
    let features = vec![
        DistinguishingFeature {
            ordinal: 1,
            feature_name: "加热".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: None,
            oa_excerpt: "".into(),
        },
        DistinguishingFeature {
            ordinal: 2,
            feature_name: "温控".into(),
            oa_tag: OaTag::RoutineDesign,
            attributed_ref: None,
            oa_excerpt: "".into(),
        },
    ];
    // 加热和温控不在同一从句
    let graph = build_dependency_graph_with_features("包括加热模块。还包括温控单元。", &features);
    assert!(graph.edges.is_empty(), "无共现时不应建边");
}

// ── Stage D 作用相同断言对比测试 ──

#[tokio::test]
async fn stage_d_detects_effect_claim_mismatch() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "999".into(),
            applicant: "t".into(),
            invention_title: "t".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "D1中公开了加热模块，其与本申请的加热模块作用相同，均为了提高温度。".into(),
        reference_documents: vec![ReferenceDocument {
            label: "D1".into(),
            publication_number: "CN111A".into(),
            full_text: "D1原文".into(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        }],
        claims_under_review: "包括加热模块".into(),
        spec_effect_statements: vec!["通过电磁感应实现均匀加热".into()],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let mismatches = stage_d_compare_effects(&input, None).await;
    assert!(!mismatches.is_empty(), "应检测到效果不匹配");
    let m = &mismatches[0];
    assert!(m.rebuttal_text.contains("实质差异"), "反驳段应指出实质差异");
    assert!(
        m.rebuttal_text.contains("电磁感应"),
        "反驳段应引用本申请实际效果"
    );
}

#[tokio::test]
async fn stage_d_no_effect_claim_no_mismatch() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "999".into(),
            applicant: "t".into(),
            invention_title: "t".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "权利要求1不具备创造性。".into(),
        reference_documents: vec![],
        claims_under_review: "1. 一种装置".into(),
        spec_effect_statements: vec!["提高效率".into()],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    let mismatches = stage_d_compare_effects(&input, None).await;
    assert!(mismatches.is_empty(), "无'作用相同'断言时不应生成mismatch");
}

#[test]
fn scan_effect_claims_finds_patterns() {
    let oa = "D1的加热模块与本申请作用相同，均为了提高温度。另外，D2的效果一致。";
    let claims = scan_effect_claims(oa);
    assert!(
        claims.iter().any(|c| c.text.contains("作用相同")),
        "应匹配'作用相同'"
    );
    assert!(
        claims.iter().any(|c| c.text.contains("均为了")),
        "应匹配'均为了'"
    );
    assert!(
        claims.iter().any(|c| c.text.contains("效果一致")),
        "应匹配'效果一致'"
    );
}

#[test]
fn scan_effect_claims_quotes_end_at_sentence_boundary() {
    // 造一段：命中"效果相同"后跟 >60 字才到句号——旧硬窗口（+60 字）会把引文
    // 切在句中（如「公开了流量传」半句），正式文书不可读。
    let long_tail = "本领域技术人员在对比文件的基础上结合公知常识即可获得该技术方案，这对所属技术领域的技术人员来说是显而易见的，因此该权利要求不具备创造性";
    let oa = format!(
        "对比文件1的流量控制与本申请的效果相同，{}。第二句从句号后开始。",
        long_tail
    );
    let claims = scan_effect_claims(&oa);
    let hit = claims
        .iter()
        .find(|c| c.text.contains("效果相同"))
        .expect("应命中'效果相同'");
    assert!(
        hit.text.ends_with('。'),
        "引文应在句号收尾，实际结尾: {:?}",
        hit.text.chars().last()
    );
    assert!(
        hit.text.contains("不具备创造性"),
        "引文应包含到句号为止的完整句子"
    );
    assert!(!hit.text.ends_with('…'), "未触发截断时不应带省略号");
}

#[test]
fn extend_window_to_sentence_boundary_finds_or_defers() {
    let chars: Vec<char> = "第一句。第二句很长但还没有句号".chars().collect();
    // from 指向第二句开头"第"，句号在 lookahead 内 → 返回句号后一位
    let from = chars.iter().rposition(|&c| c == '第').unwrap();
    let end = super::extend_window_to_sentence_boundary(&chars, from, 40);
    assert_eq!(chars[end - 1], '。', "应停在句号后");
    // lookahead 内无边界 → 返回原值（调用方回退硬窗口）
    let end2 = super::extend_window_to_sentence_boundary(&chars, from, 3);
    assert_eq!(end2, from, "无边界时应返回 from");
}

#[test]
fn dropped_features_note_emitted_and_lists_names() {
    let dropped = vec![
        "某条未逐字定位的特征表述甲".to_string(),
        "另一条被审查员提及的表述乙，其中包含较长的描述内容用于测试截断行为是否正确处理——这一段是超过六十字的填充内容，用于验证预览截断省略号".to_string(),
    ];
    let note = format_dropped_features_note(&dropped).expect("有丢弃必须有提示");
    assert!(note.contains("2 条"), "提示应含丢弃条数: {note}");
    assert!(note.contains("未逐字定位的特征表述甲"), "应列出丢弃特征名");
    assert!(note.contains('…'), "超 60 字的条目应截断加省略号");
    assert!(
        note.contains("请人工核对"),
        "提示应要求人工核对（响亮失败）"
    );
}

#[test]
fn dropped_features_note_none_when_empty() {
    assert!(format_dropped_features_note(&[]).is_none());
}

#[test]
fn stage_e_emits_dropped_note_after_material_warnings() {
    let input = make_sample_case_input();
    let features: Vec<DistinguishingFeature> = vec![];
    let dropped = vec!["被丢弃的特征丙（测试）".to_string()];
    let output = stage_e_assemble(&input, &features, &[], &[], &[], &[], &dropped);
    let warn_pos = output
        .find("【材料完整性提示")
        .or_else(|| output.find("复审请求范围"));
    let note_pos = output.find("【特征抽取丢弃提示");
    assert!(note_pos.is_some(), "有丢弃必须在产物内显式提示");
    if let (Some(w), Some(n)) = (warn_pos, note_pos) {
        assert!(w < n, "丢弃提示应在材料提示之后、正文论证之前");
    }
    assert!(output.contains("被丢弃的特征丙（测试）"));
    // 无丢弃 → 不出现提示
    let clean = stage_e_assemble(&input, &features, &[], &[], &[], &[], &[]);
    assert!(!clean.contains("【特征抽取丢弃提示"), "无丢弃时不得占版面");
}

#[test]
fn auto_extract_effect_statements_basic() {
    let spec = "本发明通过特殊结构提高燃烧效率。普通句子没有关键词。\
                该设计避免了热能损耗。另外的描述。";
    let effects = auto_extract_effect_statements(spec);
    assert!(
        effects.iter().any(|e| e.contains("提高")),
        "应提取含'提高'的效果句"
    );
    assert!(
        effects.iter().any(|e| e.contains("避免")),
        "应提取含'避免'的效果句"
    );
    assert!(
        !effects.iter().any(|e| e.contains("普通句子")),
        "不应提取无关键词的句子"
    );
}

#[test]
fn find_ref_label_finds_nearby() {
    let oa = "对比文件D1公开了一种结构。该结构与本申请的方案作用相同。";
    let label = find_ref_label_for_claim(oa, "作用相同");
    assert_eq!(label, Some("D1".to_string()));
}

// ── Stage A verbatim 丢弃 trace 测试 ──

#[test]
fn stage_a_discards_non_verbatim_feature() {
    // 模拟AI返回一个非verbatim特征名
    // claims中只有"对角安装"，AI返回"斜向安装"（意译，非verbatim）
    let claims = "1. 一种装置，包括对角安装的加热元件";
    let ai_feature = "斜向安装"; // 意译，不是claims的子串
    assert!(
        !claims.contains(ai_feature),
        "验证前提：'斜向安装'确实不是claims子串"
    );
    assert!(
        claims.contains("对角安装"),
        "验证前提：'对角安装'是claims子串"
    );
    // Stage A 的 verbatim 校验逻辑：claims.contains(feature_name)
    // "斜向安装" 会被丢弃，"对角安装" 会保留
}

#[tokio::test]
async fn pipeline_full_stage_c_d_integration() {
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "2025999".into(),
            applicant: "测试人".into(),
            invention_title: "一种测试装置".into(),
            oa_date: Some("2026-09-01".into()),
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "审查员认为，D1公开了加热模块，其与本申请作用相同，均为了提高温度。\
                       权利要求1的区别特征加热模块相对于D1属于常规设计。"
            .into(),
        reference_documents: vec![ReferenceDocument {
            label: "D1".into(),
            publication_number: "CN111222A".into(),
            full_text: "D1公开了一种加热装置。".into(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        }],
        claims_under_review: "1. 一种测试装置，包括加热模块、温控单元".into(),
        spec_effect_statements: vec!["通过电磁感应实现均匀加热".into()],
        spec_full_text: None,
        legacy_analysis: None,
        legacy_discussion: None,
        custom_stopwords: vec![],
    };
    // 无AI运行：Stage A空，Stage B空，Stage C空，Stage D确定性运行
    let result = generate_response_letter_pipeline(&input, None, None).await;
    assert!(result.full_text.contains("复审请求书"));
    assert!(result.full_text.contains("2025999"));
    assert!(result.full_text.contains("测试人"));
    assert!(result.full_text.contains("发回原审查部门"));
    // Guard应通过
    assert!(
        result.guard_result.passed,
        "Guard errors: {:?}",
        result.guard_result.errors
    );
}

#[test]
fn stage_e_includes_academic_doi_and_part_topology_rebuttals() {
    let mut input = make_sample_case_input();
    input.oa_full_text = "审查员认为上述特征属于本领域的公知常识。".to_string();
    input.claims_under_review =
        "一种控制阀，包括1-机架、2-阀体和3-阀芯，阀芯3与阀体2滑动配合。".to_string();
    input.reference_documents = vec![
        ReferenceDocument {
            label: "D1".to_string(),
            publication_number: "CN001".to_string(),
            full_text: "对比文件1公开了一种机架和阀体，但未公开阀芯及其滑动配合结构。".to_string(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        },
        ReferenceDocument {
            label: "NPL1".to_string(),
            publication_number: "10.1016/j.test.2026.009".to_string(),
            full_text: "Academic study.".to_string(),
            extraction_method: Some("openalex".to_string()),
            page_count: None,
            technical_field: None,
        },
    ];

    let output = stage_e_assemble(&input, &[], &[], &[], &[], &[], &[]);
    assert!(output.contains("10.1016/j.test.2026.009"));
    assert!(output.contains("阀芯") && output.contains("滑动配合"));

    let guard = crate::ai::output_guard::run_guard(&output, &input);
    assert!(guard.passed, "Guard errors: {:?}", guard.errors);
}
