//! Phase 4 end-to-end test: run the pipeline with a synthetic CaseInput.
//! This test does NOT require an AI client — the pipeline works with None ai_client
//! (Stage A falls back to empty features, Stage B/C/D still run deterministically).
//!
//! The CaseInput is inlined as a synthetic JSON constant — no real case data,
//! no external fixture file, no graceful skip. If the pipeline breaks, this
//! test fails hard.

use innoforge::ai::case_input::CaseInput;
use innoforge::ai::response_pipeline::generate_response_letter_pipeline;

/// Synthetic CaseInput JSON — all values are placeholders, no real case data.
const SYNTHETIC_CASE_JSON: &str = r#"{
  "case_meta": {
    "application_number": "9999001.1",
    "applicant": "示例申请人",
    "invention_title": "一种合成技术装置",
    "oa_date": "2026-07-10",
    "oa_type": "reject_review"
  },
  "oa_full_text": "合成驳回决定\n\n申请号：9999001.1 发文序号：2099010100000001\n\n发明名称：一种合成技术装置\n\n对比文件 1: CN990001001A，公开日为2013年06月05日\n对比文件 2: CN990001002A，公开日为2021年07月13日\n对比文件 3: CN990001003U，公开日为2020年07月07日\n\n权利要求1请求保护一种合成技术装置，对比文件1（CN990001001A）是最接近的现有技术。\n\n审查员认为：区别特征的设置属于常规设计，对于本领域技术人员来说容易想到。上述区别仅属于惯用手段，属于常规设置和常规选择。通过常规调整即可实现。\n\n综上所述，权利要求1不具备创造性，驳回该申请。",
  "reference_documents": [
    {
      "label": "D1",
      "publication_number": "CN990001001A",
      "full_text": "合成对比文件D1\n专利号: CN990001001A\n公开日: 2013-06-05\n摘要：本发明公开一种合成装置，包括壳体和连接部件。"
    },
    {
      "label": "D2",
      "publication_number": "CN990001002A",
      "full_text": "合成对比文件D2\n专利号: CN990001002A\n公开日: 2021-07-13\n摘要：本发明公开一种合成组件。"
    },
    {
      "label": "D3",
      "publication_number": "CN990001003U",
      "full_text": "合成对比文件D3\n专利号: CN990001003U\n公开日: 2020-07-07\n摘要：本实用新型公开一种合成结构。"
    }
  ],
  "claims_under_review": "1. 一种合成技术装置，包括控制器和通风管，其特征在于，所述通风管的一端固定连接有合成喷嘴，合成喷嘴的圆周外壁固定连接有对角安装的上网状合成器和下网状合成器。"
}"#;

#[tokio::test]
async fn phase4_pipeline_e2e_sample() {
    let case_input: CaseInput =
        serde_json::from_str(SYNTHETIC_CASE_JSON).expect("synthetic CaseInput JSON must parse");

    // Verify case meta
    assert!(
        !case_input.case_meta.application_number.is_empty(),
        "application_number should not be empty"
    );
    assert_eq!(case_input.case_meta.applicant, "示例申请人");
    assert_eq!(case_input.case_meta.oa_type, "reject_review");
    assert!(
        !case_input.oa_full_text.is_empty(),
        "oa_full_text should not be empty"
    );

    // Run the pipeline without an AI client
    let letter = generate_response_letter_pipeline(&case_input, None, None).await;

    // The pipeline should produce a non-empty response letter
    assert!(
        !letter.full_text.is_empty(),
        "response letter full_text should not be empty"
    );
    assert!(
        letter.full_text.contains("复审请求书"),
        "response letter should contain 复审请求书"
    );

    eprintln!("Response letter length: {} chars", letter.full_text.len());
}
