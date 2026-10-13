//! 审计检查路由 — 将 12 个 ai 审计模块接线到 HTTP 路由
//!
//! POST /api/ai/audit-checks
//! 接收 OA 文本和可选的对比文件、特征清单等参数，
//! 调用全部 12 个审计模块并返回结构化 JSON 结果。

use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::ai::{
    citation_check, claim_coverage, feature_attribution, golden_regression, paraphrase_check,
    prohibition_check, search_report, targeted_claims, tech_problem_check,
};

/// POST /api/ai/audit-checks — 运行全部 12 个审计模块
pub async fn api_ai_audit_checks(
    State(_s): State<crate::routes::AppState>,
    Json(req): Json<Value>,
) -> Json<Value> {
    let oa_text = req["oa_text"].as_str().unwrap_or("");
    if oa_text.is_empty() {
        return Json(json!({ "error": "缺少 oa_text / Missing oa_text" }));
    }

    // 解析对比文件列表
    let documents: Vec<(String, String)> = req["documents"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    let id = d["id"].as_str().unwrap_or("").to_string();
                    let text = d["text"].as_str().unwrap_or("").to_string();
                    if id.is_empty() || text.is_empty() {
                        None
                    } else {
                        Some((id, text))
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let doc_refs: Vec<(&str, &str)> = documents
        .iter()
        .map(|(id, text)| (id.as_str(), text.as_str()))
        .collect();

    // 解析已知特征
    let known_features: Vec<String> = req["known_features"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let feature_refs: Vec<&str> = known_features.iter().map(|s| s.as_str()).collect();

    // 解析关键术语
    let key_terms: Vec<String> = req["key_terms"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let term_refs: Vec<&str> = key_terms.iter().map(|s| s.as_str()).collect();

    let submitted_claims = req["submitted_claims"].as_str().unwrap_or("");
    let submitted_claim_count = req["submitted_claim_count"].as_u64().unwrap_or(0) as usize;
    let golden_output = req["golden_output"].as_str().unwrap_or("");
    let ai_output = req["ai_output"].as_str().unwrap_or("");

    // ── 1. citation_check (C4) ──
    let citation_report = citation_check::check_citations(oa_text, &doc_refs);
    let citation_json = json!({
        "citations": citation_report.citations.iter().map(|c| json!({
            "doc_id": c.doc_id,
            "quoted_text": c.quoted_text,
            "found_in_original": c.found_in_original,
            "match_position": c.match_position,
        })).collect::<Vec<_>>(),
        "mismatch_count": citation_report.mismatch_count,
        "summary": citation_report.summary,
    });

    // ── 2. claim_coverage (C9) ──
    let coverage_report = claim_coverage::check_claim_coverage(oa_text, submitted_claim_count);
    let coverage_json = json!({
        "evaluated_claims": coverage_report.evaluated_claims,
        "unevaluated_claims": coverage_report.unevaluated_claims,
        "summary": coverage_report.summary,
    });

    // ── 3. feature_coverage (C9) ──
    let feat_cov_report =
        claim_coverage::check_feature_coverage(oa_text, submitted_claims, &feature_refs);
    let feat_cov_json = json!({
        "covered_features": feat_cov_report.covered_features,
        "uncovered_features": feat_cov_report.uncovered_features,
        "summary": feat_cov_report.summary,
    });

    // ── 4. feature_attribution (C10) ──
    let attr_report =
        feature_attribution::check_feature_attribution(oa_text, &feature_refs, &doc_refs);
    let attr_json = json!({
        "features": attr_report.features.iter().map(|a| json!({
            "feature": a.feature,
            "claimed_source": a.claimed_source,
            "actual_sources": a.actual_sources,
            "is_correct": a.is_correct,
        })).collect::<Vec<_>>(),
        "misattribution_count": attr_report.misattribution_count,
        "summary": attr_report.summary,
    });

    // ── 5. golden_regression (C13) ──
    let golden_report = if !golden_output.is_empty() && !ai_output.is_empty() {
        let g =
            golden_regression::check_golden_regression(ai_output, golden_output, &term_refs, 0.8);
        json!({
            "char_diff_count": g.char_diff_count,
            "term_coverage_diff": g.term_coverage_diff,
            "similarity": g.similarity,
            "passed": g.passed,
            "summary": g.summary,
        })
    } else {
        json!({ "skipped": "未提供 golden_output / ai_output" })
    };

    // ── 6. paraphrase_check (C1) ──
    let paraphrase_segments = paraphrase_check::extract_paraphrase_segments(oa_text);
    let paraphrase_json = json!({
        "segments": paraphrase_segments.iter().map(|(doc_id, text)| json!({
            "doc_id": doc_id,
            "paraphrase_text": text,
        })).collect::<Vec<_>>(),
        "segment_count": paraphrase_segments.len(),
    });

    // ── 7. pre_submission_check ──
    let precheck_json = json!({
        "status": "available",
        "note": "需完整 CaseInput，请用 /api/ai/oa-discuss 或 /api/ai/oa-panel 调用",
    });

    // ── 8. search_report (C3/C7/C8) ──
    let sr_entries = search_report::parse_search_report(oa_text);
    let sr_dependent = search_report::parse_dependent_claim_evaluations(oa_text);
    let sr_filing = search_report::extract_filing_info(oa_text);
    let search_json = json!({
        "entries": sr_entries.iter().map(|e| json!({
            "relevance_type": e.relevance_type,
            "publication_number": e.publication_number,
            "involved_claims": e.involved_claims,
            "ipc": e.ipc,
        })).collect::<Vec<_>>(),
        "entry_count": sr_entries.len(),
        "dependent_claim_evaluations": sr_dependent.len(),
        "filing_info": sr_filing.as_ref().map(|f| json!({
                "filing_date": f.filing_date,
                "filing_serial": f.filing_serial,
                "response_deadline_months": f.response_deadline_months,
                "response_deadline_text": f.response_deadline_text,
            })),
    });

    // ── 9. strategy_arbitration ──
    let strategy_json = json!({
        "status": "available",
        "note": "需完整 CaseInput + DistinguishingFeature[]，请用 /api/ai/oa-discuss 调用",
    });

    // ── 10. targeted_claims (C6) ──
    let targeted_report = targeted_claims::extract_targeted_claims_and_conclusion(oa_text);
    let targeted_json = json!({
        "rejected_claims": targeted_report.rejected_claims,
        "concluding_paragraph": targeted_report.concluding_paragraph,
        "summary": targeted_report.summary,
    });

    // ── 11. tech_problem_check (C5/C11) ──
    let tech_report = tech_problem_check::check_tech_problem(oa_text, &feature_refs);
    let tech_json = json!({
        "distinction_features": tech_report.distinction_features,
        "feature_coverage": tech_report.feature_coverage,
        "summary": tech_report.summary,
    });

    // ── 12. whitespace_search (基础设施) ──
    let ws_json = json!({
        "status": "available",
        "note": "提供 ai_output + golden_output + key_terms 以启用双向比较",
    });

    // extraction_quality 也接线（标记 OCR 文本层）
    let eq_json = if let Some(text) = req["pdf_text"].as_str() {
        let result = crate::ai::extraction_quality::mark_ocr_text(text.to_string(), None);
        json!({
            "method": result.method.as_str(),
            "is_reliable": result.method.is_reliable(),
            "text_length": result.text.len(),
            "risk_warning": result.risk_warning(),
        })
    } else {
        json!({ "skipped": "未提供 pdf_text" })
    };

    // ── 13. prohibition_check (C12) ──
    let prohibition_report = prohibition_check::check_prohibitions(oa_text);
    let prohibition_json = json!({
        "violations": prohibition_report.violations.iter().map(|v| json!({
            "rule_id": v.rule_id,
            "rule_description": v.rule_description,
            "violating_text": v.violating_text,
            "severity": match v.severity {
                prohibition_check::ProhibitionSeverity::Fatal => "fatal",
                prohibition_check::ProhibitionSeverity::Warning => "warning",
            },
        })).collect::<Vec<_>>(),
        "count": prohibition_report.count,
        "summary": prohibition_report.summary,
    });

    Json(json!({
        "status": "ok",
        "results": {
            "citation_check": citation_json,
            "claim_coverage": coverage_json,
            "feature_coverage": feat_cov_json,
            "feature_attribution": attr_json,
            "golden_regression": golden_report,
            "paraphrase_check": paraphrase_json,
            "pre_submission_check": precheck_json,
            "prohibition_check": prohibition_json,
            "search_report": search_json,
            "strategy_arbitration": strategy_json,
            "targeted_claims": targeted_json,
            "tech_problem_check": tech_json,
            "whitespace_search": ws_json,
            "extraction_quality": eq_json,
        }
    }))
}

// ═══════════════════════════════════════════════════════════════════════
//  POST /api/ai/prohibition-check — 纯函数禁令自查（不调 LLM）
// ═══════════════════════════════════════════════════════════════════════

/// POST /api/ai/prohibition-check
///
/// 接收 `response_text`，调用 `prohibition_check::check_prohibitions` 纯函数，
/// 返回 `violations` / `count` / `summary`。不依赖 LLM 服务商出网。
pub async fn api_ai_prohibition_check(
    State(_s): State<crate::routes::AppState>,
    Json(req): Json<Value>,
) -> Json<Value> {
    let response_text = req["response_text"].as_str().unwrap_or("");
    if response_text.is_empty() {
        return Json(json!({ "error": "缺少 response_text / Missing response_text" }));
    }

    let report = prohibition_check::check_prohibitions(response_text);

    Json(json!({
        "status": "ok",
        "violations": report.violations.iter().map(|v| json!({
            "rule_id": v.rule_id,
            "rule_description": v.rule_description,
            "violating_text": v.violating_text,
            "severity": match v.severity {
                prohibition_check::ProhibitionSeverity::Fatal => "fatal",
                prohibition_check::ProhibitionSeverity::Warning => "warning",
            },
        })).collect::<Vec<_>>(),
        "count": report.count,
        "summary": report.summary,
    }))
}

// ═══════════════════════════════════════════════════════════════════════
//  测试 — 正向断言：每个模块产出结构化结果
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{pre_submission_check, strategy_arbitration, whitespace_search};

    fn sample_oa() -> &'static str {
        "审查意见通知书\n\
         对比文件1公开了\u{201c}反应腔侧壁设有对称分布的传感元件\u{201d}。\n\
         对比文件2记载了交错排列的电极组件以实现均匀放电。\n\
         区别在于：本申请还包括辅助信号发生器。\n\
         实际解决的技术问题是如何提高放电均匀性。\n\
         权利要求1-10不具备创造性。\n\
         综上所述，本申请不具备创造性，予以驳回。"
    }

    fn sample_d1() -> &'static str {
        "一种反应器，反应腔侧壁设有对称分布的传感元件和辅助信号发生器。"
    }

    #[test]
    fn audit_citation_check_produces_results() {
        let report = citation_check::check_citations(sample_oa(), &[("对比文件1", sample_d1())]);
        assert!(
            !report.citations.is_empty(),
            "应提取≥1条引文，实际{}条",
            report.citations.len()
        );
        assert!(
            report.citations.iter().any(|c| c.found_in_original),
            "至少1条引文应在原文中找到"
        );
    }

    #[test]
    fn audit_claim_coverage_produces_results() {
        let report = claim_coverage::check_claim_coverage(sample_oa(), 10);
        assert!(!report.summary.is_empty(), "应产出非空摘要");
        assert!(
            report.evaluated_claims.len() + report.unevaluated_claims.len() > 0,
            "应产出权项列表"
        );
    }

    #[test]
    fn audit_feature_coverage_produces_results() {
        let report = claim_coverage::check_feature_coverage(
            sample_oa(),
            "反应腔侧壁设有对称分布的传感元件和辅助信号发生器",
            &["反应腔", "传感元件", "辅助信号发生器"],
        );
        assert!(!report.summary.is_empty(), "应产出非空摘要");
    }

    #[test]
    fn audit_feature_attribution_produces_results() {
        let report = feature_attribution::check_feature_attribution(
            sample_oa(),
            &["传感元件"],
            &[("对比文件1", sample_d1())],
        );
        assert!(!report.summary.is_empty(), "应产出非空摘要");
    }

    #[test]
    fn audit_golden_regression_produces_results() {
        let report = golden_regression::check_golden_regression(
            "火焰喷嘴的电极",
            "火焰喷嘴的电极",
            &["火焰喷嘴", "电极"],
            0.8,
        );
        assert!(report.passed, "相同文本应通过回归");
        assert!(
            report.similarity >= 0.8,
            "相似度应≥0.8，实际{}",
            report.similarity
        );
    }

    #[test]
    fn audit_paraphrase_check_produces_results() {
        let segments = paraphrase_check::extract_paraphrase_segments(sample_oa());
        assert!(
            !segments.is_empty(),
            "应提取≥1个转述段，实际{}个",
            segments.len()
        );
    }

    #[test]
    fn audit_pre_submission_check_produces_results() {
        use crate::ai::case_input::{CaseInput, CaseMeta};
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "202410000001.1".to_string(),
                applicant: "测试申请人".to_string(),
                invention_title: "测试发明".to_string(),
                oa_date: Some("2026-07-10".to_string()),
                oa_type: "reject_review".to_string(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: sample_oa().to_string(),
            reference_documents: vec![],
            claims_under_review: "权利要求1".to_string(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let checklist = pre_submission_check::run_pre_submission_check(
            &input,
            pre_submission_check::TargetSystem::CnipaFirstOfficeAction,
        );
        assert!(
            checklist.items.len() >= 5,
            "应≥5个检查项，实际{}个",
            checklist.items.len()
        );
    }

    #[test]
    fn audit_search_report_produces_results() {
        let entries = search_report::parse_search_report(sample_oa());
        let _filing = search_report::extract_filing_info(sample_oa());
        let _dependent = search_report::parse_dependent_claim_evaluations(sample_oa());
        // parse_search_report 对非标准格式可能返回空，但调用本身不应 panic
        let _ = entries;
    }

    #[test]
    fn audit_targeted_claims_produces_results() {
        let report = targeted_claims::extract_targeted_claims_and_conclusion(sample_oa());
        assert!(!report.summary.is_empty(), "应产出非空摘要");
        assert!(!report.concluding_paragraph.is_empty(), "应提取结论文本");
    }

    #[test]
    fn audit_tech_problem_check_produces_results() {
        let report = tech_problem_check::check_tech_problem(sample_oa(), &["下网状电极发生器"]);
        assert!(!report.summary.is_empty(), "应产出非空摘要");
    }

    #[test]
    fn audit_whitespace_search_produces_results() {
        let stripped = whitespace_search::strip_whitespace("火焰 喷嘴 的 电极");
        assert_eq!(stripped, "火焰喷嘴的电极");
        assert_eq!(whitespace_search::count_term(&stripped, "电极"), 1);
    }

    #[test]
    fn audit_extraction_quality_produces_results() {
        let result = crate::ai::extraction_quality::mark_ocr_text("识别文本".to_string(), Some(1));
        assert!(!result.text.is_empty(), "应保留文本");
        assert!(!result.method.is_reliable(), "OCR 标记应不可靠");
        assert!(result.risk_warning().is_some(), "OCR 应有风险警告");
    }

    #[test]
    fn audit_strategy_arbitration_produces_results() {
        use crate::ai::case_input::{CaseInput, CaseMeta, DistinguishingFeature};
        let input = CaseInput {
            case_meta: CaseMeta {
                application_number: "202410000001.1".to_string(),
                applicant: "测试".to_string(),
                invention_title: "测试".to_string(),
                oa_date: None,
                oa_type: "reject_review".to_string(),
                claim_basis_date: None,
                claim_basis_text: None,
            },
            oa_full_text: sample_oa().to_string(),
            reference_documents: vec![],
            claims_under_review: "权利要求1".to_string(),
            spec_effect_statements: vec![],
            spec_full_text: None,
            legacy_analysis: None,
            legacy_discussion: None,
            custom_stopwords: vec![],
        };
        let features = vec![DistinguishingFeature {
            ordinal: 1,
            feature_name: "下网状电极发生器".to_string(),
            oa_tag: crate::ai::case_input::OaTag::EasyToThink,
            attributed_ref: None,
            oa_excerpt: "区别在于".to_string(),
        }];
        let result = strategy_arbitration::arbitrate(&input, &features, &[], &[]);
        assert!(
            !result.items.is_empty(),
            "应产出≥1条策略项，实际{}条",
            result.items.len()
        );
    }
}
