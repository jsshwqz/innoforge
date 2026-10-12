//! C1-C13 端到端集成测试
//!
//! 这些测试验证各纯函数模块在真实 OA 文本上的端到端行为。
//! 标记 #[ignore] 因为它们依赖测试夹具文件，且属于回归测试而非单元测试。
//! 运行方式：cargo test --test c_modules_e2e -- --ignored

use innoforge::ai::citation_check::check_citations;
use innoforge::ai::claim_coverage::{check_claim_coverage, check_feature_coverage};
use innoforge::ai::feature_attribution::check_feature_attribution;
use innoforge::ai::golden_regression::check_golden_regression;
use innoforge::ai::paraphrase_check::{check_paraphrase_distortion, extract_paraphrase_segments};
use innoforge::ai::prohibition_check::check_prohibitions;
use innoforge::ai::search_report::{extract_filing_info, parse_search_report};
use innoforge::ai::targeted_claims::extract_targeted_claims_and_conclusion;
use innoforge::ai::tech_problem_check::check_tech_problem;
use innoforge::ai::whitespace_search::{bidirectional_compare, count_term, strip_whitespace};
use innoforge::ai::zero_evidence::check_zero_evidence;

const OA_REJECTION: &str = include_str!("../tests/fixtures/oa_sample/oa_rejection.txt");
const OA_FIRST_NOTICE: &str = include_str!("../tests/fixtures/oa_sample/oa_first_notice.txt");
const D1: &str = include_str!("../tests/fixtures/oa_sample/d1_sample.txt");
const D2: &str = include_str!("../tests/fixtures/oa_sample/d2_sample.txt");
const D3: &str = include_str!("../tests/fixtures/oa_sample/d3_sample.txt");
const SUBMITTED_CLAIMS: &str = include_str!("../tests/fixtures/oa_sample/submitted_claims.txt");
const GOLDEN_REF: &str = include_str!("../tests/fixtures/oa_sample/golden_reference.txt");

const KEY_FEATURES: &[&str] = &[
    "对角",
    "网状",
    "把手",
    "控制阀",
    "陶瓷",
    "弹簧",
    "定位杆",
    "滑块",
];

const KEY_TERMS: &[&str] = &["对角", "交错", "内侧面", "长方形", "稳燃锥罩"];

#[test]
#[ignore]
fn e2e_c1_paraphrase_distortion_rejection_vs_d1() {
    let segments = extract_paraphrase_segments(OA_REJECTION);
    assert!(!segments.is_empty(), "应提取到转述段");

    for (doc_id, paraphrase) in &segments {
        let original = if doc_id == "对比文件1" {
            D1
        } else if doc_id == "对比文件2" {
            D2
        } else if doc_id == "对比文件3" {
            D3
        } else {
            continue;
        };

        let report = check_paraphrase_distortion(doc_id, paraphrase, original, KEY_TERMS);
        eprintln!("C1 {}: {}", doc_id, report.summary);
    }

    // C1 D1 精确断言（干净文本层提取）
    let d1_no_ws = strip_whitespace(D1);
    assert!(count_term(&d1_no_ws, "交错") >= 2, "D1 交错应>=2");
    assert!(count_term(&d1_no_ws, "内侧面") >= 4, "D1 内侧面应>=4");
    assert!(count_term(&d1_no_ws, "长方形") >= 2, "D1 长方形应>=2");
    assert_eq!(count_term(&d1_no_ws, "对角"), 0, "D1 对角应==0");
    eprintln!("C1 D1 断言全部通过：交错>=2 内侧面>=4 长方形>=2 对角==0");
}

#[test]
#[ignore]
fn e2e_c2_zero_evidence_in_rejection() {
    let report = check_zero_evidence(OA_REJECTION);
    eprintln!("C2 零举证：{}", report.summary);
    for a in &report.zero_evidence_assertions {
        eprintln!(
            "  权项{:?} {}: {}",
            a.claim_number,
            a.assertion_type,
            &a.text_fragment[..a
                .text_fragment
                .char_indices()
                .take(50)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0)]
        );
    }

    // C2 精确断言：零举证合计必须 == 11
    // 基准：容易想到=5 + 惯用手段=2 + 常规设计=2 + 常规选择=1 + 常规调整=1 = 11
    // 注意：此断言要求 oa_rejection.txt 是干净文本层提取，非 OCR
    assert_eq!(
        report.count, 11,
        "C2 零举证合计应为11（容易想到5+惯用手段2+常规设计2+常规选择1+常规调整1），实际{}。         如果此断言失败，检查 oa_rejection.txt 是否为干净文本层提取而非OCR。",
        report.count
    );
}

#[test]
#[ignore]
fn e2e_c3_search_report_from_first_notice() {
    let entries = parse_search_report(OA_FIRST_NOTICE);
    eprintln!("C3 检索报告：{} 条目", entries.len());
    for e in &entries {
        eprintln!(
            "  {} {} {} {}",
            e.relevance_type, e.publication_number, e.publication_date, e.ipc
        );
    }
}

#[test]
#[ignore]
fn e2e_c4_citation_check_all_documents() {
    let report = check_citations(
        OA_REJECTION,
        &[("对比文件1", D1), ("对比文件2", D2), ("对比文件3", D3)],
    );
    eprintln!("C4 引文核对：{}", report.summary);
}

#[test]
#[ignore]
fn e2e_c5_tech_problem_reconstruction() {
    let report = check_tech_problem(OA_REJECTION, KEY_FEATURES);
    eprintln!("C5/C11 技术问题：{}", report.summary);
    eprintln!("  区别段长度: {}", report.distinction_text.len());
    eprintln!("  技术问题句长度: {}", report.tech_problem_text.len());
}

#[test]
#[ignore]
fn e2e_c6_targeted_claims_and_conclusion() {
    let report = extract_targeted_claims_and_conclusion(OA_REJECTION);
    eprintln!("C6: {}", report.summary);
    if report.has_concluding_paragraph {
        eprintln!(
            "  末段前200字: {}",
            &report.concluding_paragraph[..200.min(report.concluding_paragraph.len())]
        );
    }
}

#[test]
#[ignore]
fn e2e_c8_filing_info() {
    if let Some(info) = extract_filing_info(OA_REJECTION) {
        eprintln!(
            "C8 驳回决定：序号={} 日期={} 期限={}月",
            info.filing_serial, info.filing_date, info.response_deadline_months
        );
    }
    if let Some(info) = extract_filing_info(OA_FIRST_NOTICE) {
        eprintln!(
            "C8 一通：序号={} 日期={} 期限={}月",
            info.filing_serial, info.filing_date, info.response_deadline_months
        );
    }
}

#[test]
#[ignore]
fn e2e_c9_claim_coverage() {
    let report = check_claim_coverage(OA_REJECTION, 6);
    eprintln!("C9 权项覆盖：{}", report.summary);

    let feature_report = check_feature_coverage(OA_REJECTION, SUBMITTED_CLAIMS, KEY_FEATURES);
    eprintln!("C9 特征覆盖：{}", feature_report.summary);
}

#[test]
#[ignore]
fn e2e_c10_feature_attribution() {
    let report = check_feature_attribution(
        OA_REJECTION,
        KEY_FEATURES,
        &[("对比文件1", D1), ("对比文件2", D2), ("对比文件3", D3)],
    );
    eprintln!("C10 特征归属：{}", report.summary);
    for f in &report.features {
        eprintln!(
            "  '{}': 声称={} 实际={:?} 正确={}",
            f.feature, f.claimed_source, f.actual_sources, f.is_correct
        );
    }
}

#[test]
#[ignore]
fn e2e_c12_prohibition_check_on_golden() {
    let report = check_prohibitions(GOLDEN_REF);
    eprintln!("C12 黄金参考禁令自查：{}", report.summary);
    assert!(
        report
            .violations
            .iter()
            .all(|v| v.severity != innoforge::ai::prohibition_check::ProhibitionSeverity::Fatal),
        "黄金参考不应有致命违规"
    );
}

#[test]
#[ignore]
fn e2e_c13_golden_regression() {
    let report = check_golden_regression(GOLDEN_REF, GOLDEN_REF, KEY_FEATURES, 0.85);
    assert!(report.passed, "黄金参考自回归应通过");
    eprintln!("C13 黄金自回归：{}", report.summary);
}

#[test]
#[ignore]
fn e2e_bidirectional_key_terms_rejection_vs_d1() {
    let report = bidirectional_compare(OA_REJECTION, D1, KEY_TERMS);
    eprintln!("双向比对 OA vs D1：");
    for (term, a, b) in report
        .a_counts
        .iter()
        .zip(report.b_counts.iter())
        .map(|((t, a), (_, b))| (t, a, b))
    {
        eprintln!("  {}: OA={} D1={}", term, a, b);
    }
    for d in &report.distortions {
        eprintln!("  失真: {} ({:?})", d.term, d.kind);
    }
}

#[test]
#[ignore]
fn e2e_submitted_claims_structure_only() {
    let stripped = strip_whitespace(SUBMITTED_CLAIMS);

    // 递交权项应只含结构特征，不含机理描述
    let mechanism_terms = [
        "电场",
        "交流",
        "接通电源",
        "辅助电场",
        "形变",
        "湍流",
        "迁移",
        "往返",
        "周期性",
        "带电粒子",
    ];
    for term in &mechanism_terms {
        let count = count_term(&stripped, term);
        assert_eq!(count, 0, "递交权项不应含'{}'，但找到{}次", term, count);
    }

    // 应含结构特征
    assert!(count_term(&stripped, "对角") >= 1);
    assert!(count_term(&stripped, "网状") >= 4);
    assert_eq!(count_term(&stripped, "把手"), 1);
    assert_eq!(count_term(&stripped, "控制阀"), 1);
    assert_eq!(count_term(&stripped, "陶瓷"), 1);
    assert_eq!(count_term(&stripped, "弹簧"), 1);
    assert_eq!(count_term(&stripped, "定位杆"), 2);
    assert_eq!(count_term(&stripped, "滑块"), 3);

    eprintln!("递交权项结构特征验证全部通过");
}
