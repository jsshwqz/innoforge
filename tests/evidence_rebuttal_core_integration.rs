//! T-204 三栖证据核验、跨实施例识破、时序守卫与五维硬阻断总闸门集成测试
//!
//! 覆盖 5 类负控拦截与 1 组正向对照：
//! 1. 伪造专利段落号 `Desc:[9999]` -> `Unverified` + `DowngradeNeedsEvidence`
//! 2. 伪造未核验论文 `DOI:10.9999/fake-paper-2026` -> `Unverified` + `DowngradeNeedsEvidence`
//! 3. 跨实施例拼凑（部件 `11` 仅在实施例1，部件 `22` 仅在实施例2） -> `Some(StitchingAlert)`
//! 4. 第 33 条修改超范围（引入原说明书未记载的 `"钛合金纳米涂层"` / `"850℃"`） -> `HardBlocked`
//! 5. 公开时序冲突（`unfiled_secret` 导出 `AcademicPaper`） + 商业秘密脱敏 -> `red` + `HardBlocked`
//! 6. 正向对照（真实锚点 + 核验 DOI + 实测实验 + 合规修改 + `patent_filed`） -> `PassSubmittable`

use innoforge::core::evidence_rebuttal::{EvidenceRebuttalService, GateEvaluationInput};
use innoforge::core::vault::models::{CitationVerdict, DeliverableGenre, GateDecision};
use innoforge::core::vault::schema_mig::ensure_vault_tables;
use rusqlite::{params, Connection};

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    ensure_vault_tables(&conn).expect("ensure vault tables");

    // 对比专利文献 D1（含完整全文与两个互不重叠的实施例）
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, filing_status,
            doi_verified, abstract_text, claims_json, fulltext_raw, fulltext_status, chunk_ver
        ) VALUES (?1, 'patent', 'competitor_wall', 'CN990001001A', '对比散热装置', 'public_disclosed',
                  0, '一种基础散热结构', '权利要求1：包括基座10。', '实施例1包括液冷微通道11；实施例2包括相变均热板22。', 'fulltext_ready', 2)",
        params!["CN-D1-001"],
    )
    .expect("insert D1");

    conn.execute(
        "INSERT INTO doc_chunks (
            doc_id, section_kind, section_tag, claim_no, para_start, para_end,
            embodiment_group, raw_content, fts_tokens, char_count
        ) VALUES (?1, 'embodiment', 'Desc:[0024]', NULL, 24, 24, '实施例1',
                  '在实施例1中，基座内部设有液冷微通道11，用于提高导热效率。', '实施例 1 液冷 微通道 11', 34)",
        params!["CN-D1-001"],
    )
    .expect("insert D1 chunk 0024");

    conn.execute(
        "INSERT INTO doc_chunks (
            doc_id, section_kind, section_tag, claim_no, para_start, para_end,
            embodiment_group, raw_content, fts_tokens, char_count
        ) VALUES (?1, 'embodiment', 'Desc:[0035]', NULL, 35, 35, '实施例2',
                  '在实施例2中，采用相变均热板22替代微通道以适应无泵场景。', '实施例 2 相变 均热板 22', 31)",
        params!["CN-D1-001"],
    )
    .expect("insert D1 chunk 0035");

    conn.execute(
        "INSERT INTO figure_numerals (doc_id, figure_label, numeral_code, part_name, embodiment_group, anchor_tag)
         VALUES (?1, '图1', '11', '液冷微通道', '实施例1', 'Desc:[0024]')",
        params!["CN-D1-001"],
    )
    .expect("insert numeral 11");

    conn.execute(
        "INSERT INTO figure_numerals (doc_id, figure_label, numeral_code, part_name, embodiment_group, anchor_tag)
         VALUES (?1, '图2', '22', '相变均热板', '实施例2', 'Desc:[0035]')",
        params!["CN-D1-001"],
    )
    .expect("insert numeral 22");

    // 我方已递交专利（patent_filed）
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, filing_status,
            doi_verified, abstract_text, claims_json, fulltext_raw, fulltext_status, chunk_ver
        ) VALUES (?1, 'patent', 'own_asset', 'CN990001002A', '我方复合散热结构', 'patent_filed',
                  0, '一种梯度多孔铜散热结构', '权利要求1：一种散热器，包括基座和微通道。',
                  '具体实施方式：微通道内壁采用梯度多孔铜结构，工作温度为120℃至200℃。', 'fulltext_ready', 2)",
        params!["CN-OWN-001"],
    )
    .expect("insert OWN-001");

    conn.execute(
        "INSERT INTO doc_chunks (
            doc_id, section_kind, section_tag, claim_no, para_start, para_end,
            embodiment_group, raw_content, fts_tokens, char_count
        ) VALUES (?1, 'claim_indep', 'Claim:1', 1, NULL, NULL, NULL,
                  '一种散热器，包括基座和微通道。', '散热器 基座 微通道', 16)",
        params!["CN-OWN-001"],
    )
    .expect("insert OWN-001 Claim:1");

    // 我方未递交专利的保密新方案（unfiled_secret）
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, filing_status,
            doi_verified, abstract_text, claims_json, fulltext_raw, fulltext_status, chunk_ver
        ) VALUES (?1, 'internal_scheme', 'own_asset', '', '未递交核心冷却配方', 'unfiled_secret',
                  0, '内部未公开配方', '', '内部机密工艺参数', 'fulltext_ready', 1)",
        params!["OWN-SECRET-001"],
    )
    .expect("insert OWN-SECRET-001");

    // 已核验真实论文 DOI
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_en, filing_status,
            doi_verified, abstract_text, fulltext_status, chunk_ver
        ) VALUES (?1, 'journal_paper', 'general_ref', '10.1038/s41586-024-verified',
                  'Verified Thermal Management Survey', 'public_disclosed',
                  1, 'Experimental benchmark of porous copper microchannels.', 'fulltext_ready', 2)",
        params!["PAPER-VERIFIED-001"],
    )
    .expect("insert verified paper");

    // 未核验论文 DOI（doi_verified = 0）
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_en, filing_status,
            doi_verified, abstract_text, fulltext_status, chunk_ver
        ) VALUES (?1, 'journal_paper', 'general_ref', '10.9999/unverified-doi',
                  'Unverified Preprint', 'public_disclosed',
                  0, 'Not verified yet.', 'summary_only', 0)",
        params!["PAPER-UNVERIFIED-001"],
    )
    .expect("insert unverified paper");

    // 已回填实测数据的对照实验
    conn.execute(
        "INSERT INTO experiment_records (
            exp_id, run_id, title, hypothesis, control_group_spec, test_group_spec,
            measured_data_json, status
        ) VALUES ('exp-001', 'run-1', '多孔铜微通道温降实测', '温降>=15%', '纯铜光滑通道', '梯度多孔铜通道',
                  '{\"delta_t_pct\": 18.4}', 'real_data_filled')",
        [],
    )
    .expect("insert exp-001");

    conn
}

#[test]
fn neg1_fabricated_patent_paragraph_triggers_unverified_and_downgrade() {
    let conn = setup_test_db();
    let draft = "对比文件 CN-D1-001 在 [CN-D1-001#Desc:[9999]] 中公开了全部特征。";

    let citations = EvidenceRebuttalService::verify_all_citations(&conn, draft, Some("CN-D1-001"))
        .expect("verify citations");
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].verdict, CitationVerdict::Unverified);
    assert_eq!(citations[0].anchor_tag, "Desc:[9999]");

    let gate = EvidenceRebuttalService::evaluate_submission_gate(
        &conn,
        &GateEvaluationInput {
            draft_text: draft.to_string(),
            default_doc_id: Some("CN-D1-001".to_string()),
            require_citations: true,
            ..Default::default()
        },
    )
    .expect("evaluate gate");

    assert_eq!(gate.decision, GateDecision::DowngradeNeedsEvidence);
    assert!(!gate.downgrade_warnings.is_empty());
}

#[test]
fn neg2_fabricated_doi_triggers_unverified_and_downgrade() {
    let conn = setup_test_db();
    let draft = "根据学术文献 DOI:10.9999/fake-paper-2026 的结论，该方案已被现有技术公开。";

    let citations = EvidenceRebuttalService::verify_all_citations(&conn, draft, None)
        .expect("verify citations");
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].verdict, CitationVerdict::Unverified);

    let gate = EvidenceRebuttalService::evaluate_submission_gate(
        &conn,
        &GateEvaluationInput {
            draft_text: draft.to_string(),
            require_citations: true,
            ..Default::default()
        },
    )
    .expect("evaluate gate");

    assert_eq!(gate.decision, GateDecision::DowngradeNeedsEvidence);
}

#[test]
fn neg3_cross_embodiment_stitching_detected_deterministically() {
    let conn = setup_test_db();
    let cited = vec!["11".to_string(), "22".to_string()];

    let alert =
        EvidenceRebuttalService::detect_cross_embodiment_stitching(&conn, "CN-D1-001", &cited)
            .expect("detect stitching")
            .expect("must detect cross-embodiment stitching");

    assert_eq!(alert.doc_id, "CN-D1-001");
    assert!(alert
        .conflicting_embodiment_groups
        .contains(&"实施例1".to_string()));
    assert!(alert
        .conflicting_embodiment_groups
        .contains(&"实施例2".to_string()));
    assert!(alert.rebuttal_rationale.contains("跨实施例拼凑"));

    let gate = EvidenceRebuttalService::evaluate_submission_gate(
        &conn,
        &GateEvaluationInput {
            draft_text: "结合 [CN-D1-001#Desc:[0024]] 与 [CN-D1-001#Desc:[0035]]。".to_string(),
            default_doc_id: Some("CN-D1-001".to_string()),
            stitching_alerts: vec![alert],
            ..Default::default()
        },
    )
    .expect("evaluate gate");

    assert_eq!(gate.decision, GateDecision::DowngradeNeedsEvidence);
}

#[test]
fn neg4_patent_art33_out_of_scope_triggers_hard_blocked() {
    let conn = setup_test_db();
    let orig_claim = "一种散热器，包括基座和微通道。";
    let amended_claim = "一种散热器，包括基座和微通道，表面设有钛合金纳米涂层，工作温度为850℃。";

    let scope_report = EvidenceRebuttalService::check_patent_art33_scope(
        &conn,
        "CN-OWN-001",
        orig_claim,
        amended_claim,
        "请求基于修改后的权利要求继续审查。",
    )
    .expect("check art33");

    assert!(!scope_report.passed);
    assert!(scope_report
        .violations
        .iter()
        .any(|v| v.contains("钛合金纳米涂层") || v.contains("850℃")));

    let gate = EvidenceRebuttalService::evaluate_submission_gate(
        &conn,
        &GateEvaluationInput {
            draft_text: "见 [CN-OWN-001#Claim:1]。".to_string(),
            default_doc_id: Some("CN-OWN-001".to_string()),
            scope_report: Some(scope_report),
            ..Default::default()
        },
    )
    .expect("evaluate gate");

    assert_eq!(gate.decision, GateDecision::HardBlocked);
    assert!(!gate.blocking_reasons.is_empty());
}

#[test]
fn neg5_unfiled_secret_academic_disclosure_triggers_red_and_hard_blocked() {
    let conn = setup_test_db();
    let draft = "本文公开散热结构，其中核心冷却液配比参数为 X-999-SECRET。";
    let (priority, redacted) = EvidenceRebuttalService::check_disclosure_priority(
        &conn,
        &["OWN-SECRET-001".to_string()],
        DeliverableGenre::AcademicPaper,
        &["X-999-SECRET".to_string()],
        draft,
    )
    .expect("check priority");

    assert_eq!(priority.lamp, "red");
    assert!(priority.has_conflict);
    assert_eq!(
        priority.conflicting_doc_ids,
        vec!["OWN-SECRET-001".to_string()]
    );
    assert!(!redacted.contains("X-999-SECRET"));
    assert!(redacted.contains("[商业秘密脱敏保留 / Know-how Redacted]"));

    let gate = EvidenceRebuttalService::evaluate_submission_gate(
        &conn,
        &GateEvaluationInput {
            draft_text: redacted,
            priority_check: Some(priority),
            ..Default::default()
        },
    )
    .expect("evaluate gate");

    assert_eq!(gate.decision, GateDecision::HardBlocked);
}

#[test]
fn pos_all_verified_anchors_compliant_amendment_and_patent_filed_pass() {
    let conn = setup_test_db();

    // 合规修改：仅引入原说明书中已有字面记载的“梯度多孔铜结构”
    let scope_report = EvidenceRebuttalService::check_patent_art33_scope(
        &conn,
        "CN-OWN-001",
        "一种散热器，包括基座和微通道。",
        "一种散热器，包括基座和微通道，微通道内壁采用梯度多孔铜结构。",
        "我方修改未超出原说明书和权利要求书记载的范围。",
    )
    .expect("check compliant art33");
    assert!(
        scope_report.passed,
        "violations: {:?}",
        scope_report.violations
    );

    // 时序守卫：CN-OWN-001 已锁定专利申请日（patent_filed）
    let draft = "我方方案 [CN-OWN-001#Claim:1] 不同于对比文献 [CN-D1-001#Desc:[0024]]，\
                 结合论文 DOI:10.1038/s41586-024-verified 与实测实验 Exp:exp-001，性能提升显著。";
    let (priority, _) = EvidenceRebuttalService::check_disclosure_priority(
        &conn,
        &["CN-OWN-001".to_string()],
        DeliverableGenre::AcademicPaper,
        &[],
        draft,
    )
    .expect("check priority");
    assert_eq!(priority.lamp, "green");
    assert!(!priority.has_conflict);

    let gate = EvidenceRebuttalService::evaluate_submission_gate(
        &conn,
        &GateEvaluationInput {
            draft_text: draft.to_string(),
            default_doc_id: Some("CN-OWN-001".to_string()),
            require_citations: true,
            stitching_alerts: Vec::new(),
            scope_report: Some(scope_report),
            priority_check: Some(priority),
        },
    )
    .expect("evaluate gate");

    assert_eq!(gate.decision, GateDecision::PassSubmittable);
    assert_eq!(gate.verified_citations.len(), 4);
    assert!(gate
        .verified_citations
        .iter()
        .all(|c| c.verdict == CitationVerdict::Verified));
}
