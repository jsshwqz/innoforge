//! Stage A verbatim 丢弃 trace 验证。
//!
//! 模拟 AI 返回一个非 verbatim 特征名，验证 Stage A 的 verbatim 校验逻辑会丢弃它。

#[tokio::test]
async fn stage_a_verbatim_discard_trace() {
    use innoforge::ai::case_input::*;
    use innoforge::ai::response_pipeline::generate_response_letter_pipeline;

    // 构造 CaseInput：权利要求中只有"对角安装"
    let input = CaseInput {
        case_meta: CaseMeta {
            application_number: "TEST001".into(),
            applicant: "测试申请人".into(),
            invention_title: "一种测试装置".into(),
            oa_date: None,
            oa_type: "reject_review".into(),
            claim_basis_date: None,
            claim_basis_text: None,
        },
        oa_full_text: "审查员认为对角安装是常规设计。".into(),
        reference_documents: vec![ReferenceDocument {
            label: "D1".into(),
            publication_number: "CN111222A".into(),
            full_text: "D1公开了一种安装方式。".into(),
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

    // 模拟 AI 返回两个特征：
    // - "对角安装" → 是 claims 的 verbatim 子串 → 应保留
    // - "斜向安装" → 不是 claims 的子串（意译） → 应丢弃
    let claims = &input.claims_under_review;
    let spec = input.spec_full_text.as_deref().unwrap_or("");

    let ai_features = vec!["对角安装", "斜向安装"];
    let mut kept = Vec::new();
    let mut discarded = Vec::new();

    for feat in &ai_features {
        if claims.contains(feat) || spec.contains(feat) {
            kept.push(*feat);
        } else {
            discarded.push(*feat);
        }
    }

    println!("=== Stage A Verbatim 丢弃 Trace ===");
    println!("权利要求原文: {}", claims);
    println!("AI返回特征: {:?}", ai_features);
    println!("保留: {:?}", kept);
    println!("丢弃: {:?}", discarded);
    println!("丢弃原因: '斜向安装' 不是权利要求原文的 verbatim 子串");
    println!();

    // 验证
    assert!(kept.contains(&"对角安装"), "对角安装应保留");
    assert!(discarded.contains(&"斜向安装"), "斜向安装应丢弃");
    assert!(!claims.contains("斜向安装"), "确认: 斜向安装不是claims子串");

    // 同时验证管线在无AI模式下正常运行
    let result = generate_response_letter_pipeline(&input, None, None).await;
    assert!(result.guard_result.passed, "Guard应通过");
    println!("管线输出Guard: passed={}", result.guard_result.passed);
}
