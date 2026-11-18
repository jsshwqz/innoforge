//! T-205 运行时任务状态机与一源四出多体裁导出集成测试
//!
//! 覆盖：
//! 1. `DagRunner`：`RdExperiment` 档任务启动 → SSE `StageUpdate` → `pause_at_checkpoint`
//!    (`PausedCheckpoint` + `CheckpointRequired`) → `resume_from_checkpoint` → `fork_run`
//!    分叉出对比路线 B，验证 `parent_run_id` 谱系与前置已完成节点自动继承。
//! 2. `ForgeExporter`：
//!    - `.docx` 导出可用 `zip::ZipArchive` 解压读取 `word/document.xml` 且包含 `<w:ins` 与 `<w:del` 修订留痕；
//!    - `AcademicPaper` + `.tex` 导出仅包含 `doi_verified == true` 的真实 BibTeX，自动剔除未核验假文献，
//!      且关联 `unfiled_secret` 自有方案时触发时序守卫硬阻断；
//!    - `.xlsx`（`PK\x03\x04` 魔数）、`.md`、`.pdf`（`%PDF-1.4` 开头且含 `%%EOF`）全部完整生成。

use std::io::{Cursor, Read};

use innoforge::core::runtime::{
    CompileDeliverableRequest, DagRunner, ForgeExporter, TierDepthMode,
};
use innoforge::core::vault::models::{
    AssetRole, DagRunStatus, DeliverableFormat, DeliverableGenre, DocKind, FilingStatus,
    KnowledgeDoc, SseEnvelopeEvent,
};
use innoforge::core::vault::schema_mig::ensure_vault_tables;
use rusqlite::{params, Connection};
use zip::ZipArchive;

fn setup_test_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    ensure_vault_tables(&conn).expect("ensure vault tables");

    // 已锁定专利申请日的自有技术方案（patent_filed）
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, filing_status,
            doi_verified, abstract_text, claims_json, fulltext_raw, fulltext_status, chunk_ver
        ) VALUES (?1, 'patent', 'own_asset', 'CN990001002A', '我方已递交微流控散热专利', 'patent_filed',
                  0, '一种梯度微通道散热装置', '权利要求1：包括微通道与均热板。',
                  '具体实施方式：微通道采用铜基复合材料。', 'fulltext_ready', 2)",
        params!["OWN-FILED-001"],
    )
    .expect("insert OWN-FILED-001");

    // 尚未递交专利的保密技术方案（unfiled_secret）
    conn.execute(
        "INSERT INTO knowledge_docs (
            doc_id, doc_kind, asset_role, external_id, title_zh, filing_status,
            doi_verified, abstract_text, claims_json, fulltext_raw, fulltext_status, chunk_ver
        ) VALUES (?1, 'internal_scheme', 'own_asset', 'INT-SEC-001', '未公开下一代相变配方', 'unfiled_secret',
                  0, '内部保密配方方案', '', '核心配方参数处于保密状态。', 'fulltext_ready', 1)",
        params!["OWN-SECRET-001"],
    )
    .expect("insert OWN-SECRET-001");

    conn
}

#[test]
fn dag_runner_checkpoint_sse_and_fork_genealogy() {
    let conn = setup_test_db();
    let runner = DagRunner::new();

    // 1. 启动 RdExperiment 档任务
    let run = runner
        .start_run(
            &conn,
            "multimodal_cocreation",
            TierDepthMode::RdExperiment,
            r#"{"scheme":"Route-A","target":"microchannel"}"#,
            vec!["OWN-FILED-001".to_string()],
        )
        .expect("start RdExperiment run");
    assert_eq!(run.status, DagRunStatus::Running);

    let mut rx = runner.subscribe_events(&run.run_id);

    // 2. 推进前两步并验证 SSE StageUpdate 事件
    runner
        .complete_step(
            &conn,
            &run.run_id,
            "query_plan",
            serde_json::json!({"queries": ["microchannel cooling"]}),
        )
        .expect("complete query_plan");

    let evt_stage = rx.try_recv().expect("receive StageUpdate event");
    match evt_stage {
        SseEnvelopeEvent::StageUpdate { stage, percent, .. } => {
            assert_eq!(stage, "query_plan");
            assert!(percent > 0);
        }
        other => panic!("expected StageUpdate, got {:?}", other),
    }

    runner
        .complete_step(
            &conn,
            &run.run_id,
            "omni_search",
            serde_json::json!({"hit_count": 12}),
        )
        .expect("complete omni_search");
    let _ = rx.try_recv().expect("receive omni_search StageUpdate");

    // 3. 在实验设计卡点暂停，验证 PausedCheckpoint 与 CheckpointRequired 事件
    let paused = runner
        .pause_at_checkpoint(
            &conn,
            &run.run_id,
            "experiment_design_checkpoint",
            serde_json::json!({"candidates": ["Route-A", "Route-B"]}),
        )
        .expect("pause_at_checkpoint");
    assert_eq!(paused.status, DagRunStatus::PausedCheckpoint);

    let evt_cp = rx.try_recv().expect("receive CheckpointRequired event");
    match evt_cp {
        SseEnvelopeEvent::CheckpointRequired {
            checkpoint_kind, ..
        } => {
            assert_eq!(checkpoint_kind, "experiment_design_checkpoint");
        }
        other => panic!("expected CheckpointRequired, got {:?}", other),
    }

    // 4. 从卡点恢复
    let resumed = runner
        .resume_from_checkpoint(
            &conn,
            &run.run_id,
            "experiment_design_checkpoint",
            serde_json::json!({"selected": "Route-A"}),
        )
        .expect("resume_from_checkpoint");
    assert_eq!(resumed.status, DagRunStatus::Running);

    // 5. 从中间步骤 contradiction_analysis 分叉出对比路线 B
    let fork_b = runner
        .fork_run(
            &conn,
            &run.run_id,
            "contradiction_analysis",
            Some(r#"{"scheme":"Route-B","target":"vapor-chamber"}"#),
        )
        .expect("fork_run Route-B");

    assert_eq!(fork_b.parent_run_id.as_deref(), Some(run.run_id.as_str()));
    assert_eq!(
        fork_b.fork_from_step.as_deref(),
        Some("contradiction_analysis")
    );

    let (_, parent_nodes, parent_children) = runner
        .get_run_genealogy(&conn, &run.run_id)
        .expect("get parent genealogy");
    assert_eq!(parent_nodes.len(), 5);
    assert!(parent_children.iter().any(|c| c.run_id == fork_b.run_id));

    let (fetched_b, child_nodes, _) = runner
        .get_run_genealogy(&conn, &fork_b.run_id)
        .expect("get child genealogy");
    assert_eq!(
        fetched_b.parent_run_id.as_deref(),
        Some(run.run_id.as_str())
    );
    assert_eq!(child_nodes.len(), 5);

    // 验证前置已完成节点（query_plan, omni_search）被自动继承为 completed，
    // 而分叉点 contradiction_analysis 及后续节点被重置为 pending
    let status_of = |key: &str| -> String {
        child_nodes
            .iter()
            .find(|n| n.step_key == key)
            .map(|n| n.status.clone())
            .unwrap_or_default()
    };
    assert_eq!(status_of("query_plan"), "completed");
    assert_eq!(status_of("omni_search"), "completed");
    assert_eq!(status_of("contradiction_analysis"), "pending");
    assert_eq!(status_of("experiment_design_checkpoint"), "pending");
    assert_eq!(status_of("rd_synthesis"), "pending");
}

#[test]
fn forge_exporter_docx_tex_priority_xlsx_md_pdf_roundtrip() {
    let conn = setup_test_db();

    let verified_paper = KnowledgeDoc {
        doc_id: "REF-REAL-001".to_string(),
        doc_kind: DocKind::JournalPaper,
        asset_role: AssetRole::GeneralRef,
        external_id: "10.1038/s41586-025-00001-1".to_string(),
        title_zh: "真实核验学术论文".to_string(),
        title_en: "Verified Microchannel Cooling Study".to_string(),
        authors_or_assignees: "Zhang et al.".to_string(),
        pub_or_filing_date: "2025-06-01".to_string(),
        venue_or_authority: "Nature".to_string(),
        filing_status: FilingStatus::PublicDisclosed,
        doi_verified: true,
        bibtex_raw: "@article{zhang2025verified,\n  title={Verified Microchannel Cooling Study},\n  author={Zhang, Wei},\n  journal={Nature},\n  year={2025}\n}".to_string(),
        abstract_text: "Verified study.".to_string(),
        claims_json: String::new(),
        fulltext_raw: String::new(),
        fulltext_status: "metadata_only".to_string(),
        chunk_ver: 1,
    };

    let fake_paper = KnowledgeDoc {
        doc_id: "REF-FAKE-999".to_string(),
        doc_kind: DocKind::JournalPaper,
        asset_role: AssetRole::GeneralRef,
        external_id: "10.9999/hallucinated-doi".to_string(),
        title_zh: "未核验伪造文献".to_string(),
        title_en: "Hallucinated Paper".to_string(),
        authors_or_assignees: "Nobody".to_string(),
        pub_or_filing_date: "2026-01-01".to_string(),
        venue_or_authority: "FakeConf".to_string(),
        filing_status: FilingStatus::PublicDisclosed,
        doi_verified: false,
        bibtex_raw:
            "@article{fake2026hallucinated,\n  title={Hallucinated Paper},\n  year={2026}\n}"
                .to_string(),
        abstract_text: String::new(),
        claims_json: String::new(),
        fulltext_raw: String::new(),
        fulltext_status: "metadata_only".to_string(),
        chunk_ver: 1,
    };

    // 1. 验证 .docx 导出可用 zip::ZipArchive 解压并包含 <w:ins 与 <w:del 修订留痕 + 商业秘密脱敏
    let docx_req = CompileDeliverableRequest {
        title: "OA Response Draft".to_string(),
        genre: DeliverableGenre::OaPackage,
        format: DeliverableFormat::Docx,
        sections: vec![(
            "Response to Office Action".to_string(),
            "Adopted microchannel geometry with secret alloy-X99 coating.".to_string(),
        )],
        original_text_for_diff: Some("Claim 1: a heatsink base.".to_string()),
        amended_text_for_diff: Some(
            "Claim 1: a heatsink base with gradient copper microchannels.".to_string(),
        ),
        related_doc_ids: vec!["OWN-FILED-001".to_string()],
        reference_docs: vec![verified_paper.clone()],
        knowhow_markers: vec!["alloy-X99".to_string()],
        acknowledge_priority_warning: false,
    };
    let docx_out =
        ForgeExporter::compile_deliverable(&conn, &docx_req).expect("compile docx deliverable");
    assert!(docx_out.filename.ends_with(".docx"));

    let mut archive = ZipArchive::new(Cursor::new(&docx_out.bytes)).expect("open docx zip");
    let mut doc_xml = String::new();
    archive
        .by_name("word/document.xml")
        .expect("read word/document.xml")
        .read_to_string(&mut doc_xml)
        .expect("read xml content");
    assert!(doc_xml.contains("<w:del "));
    assert!(doc_xml.contains("<w:delText"));
    assert!(doc_xml.contains("<w:ins "));
    assert!(doc_xml.contains("gradient copper microchannels"));
    assert!(!doc_xml.contains("alloy-X99"));

    // 2. 验证 AcademicPaper + .tex 仅包含 doi_verified == true 的真实 BibTeX，自动剔除未核验假文献
    let tex_req = CompileDeliverableRequest {
        title: "Gradient Microchannel Paper".to_string(),
        genre: DeliverableGenre::AcademicPaper,
        format: DeliverableFormat::Tex,
        sections: vec![(
            "Introduction".to_string(),
            "Thermal dissipation in high-density chips.".to_string(),
        )],
        original_text_for_diff: None,
        amended_text_for_diff: None,
        related_doc_ids: vec!["OWN-FILED-001".to_string()],
        reference_docs: vec![verified_paper.clone(), fake_paper.clone()],
        knowhow_markers: Vec::new(),
        acknowledge_priority_warning: false,
    };
    let tex_out =
        ForgeExporter::compile_deliverable(&conn, &tex_req).expect("compile tex deliverable");
    let tex_str = String::from_utf8(tex_out.bytes.clone()).expect("tex utf8");
    assert!(tex_str.contains(r"\documentclass{article}"));
    assert!(tex_str.contains(r"\cite{zhang2025verified}"));
    assert!(!tex_str.contains("fake2026hallucinated"));

    let bib = tex_out.companion_bibtex.expect("companion bibtex present");
    assert!(bib.contains("zhang2025verified"));
    assert!(!bib.contains("fake2026hallucinated"));
    assert!(tex_out
        .gate_report
        .downgrade_warnings
        .iter()
        .any(|w| w.contains("REF-FAKE-999")));

    // 3. 验证关联 unfiled_secret 自有方案时，AcademicPaper 触发公开时序守卫硬阻断
    let blocked_req = CompileDeliverableRequest {
        related_doc_ids: vec!["OWN-SECRET-001".to_string()],
        ..tex_req
    };
    let blocked_err = ForgeExporter::compile_deliverable(&conn, &blocked_req)
        .expect_err("unfiled_secret AcademicPaper must be hard blocked");
    assert!(blocked_err.to_string().contains("硬门禁拦截"));

    // 4. 验证 .xlsx、.md、.pdf 全部完整生成
    let xlsx_req = CompileDeliverableRequest {
        title: "FTO Survey Matrix".to_string(),
        genre: DeliverableGenre::SurveyFtoReport,
        format: DeliverableFormat::Xlsx,
        sections: vec![("Summary".to_string(), "FTO clearance overview.".to_string())],
        original_text_for_diff: None,
        amended_text_for_diff: None,
        related_doc_ids: vec!["OWN-FILED-001".to_string()],
        reference_docs: vec![verified_paper.clone()],
        knowhow_markers: Vec::new(),
        acknowledge_priority_warning: false,
    };
    let xlsx_out = ForgeExporter::compile_deliverable(&conn, &xlsx_req).expect("compile xlsx");
    assert!(!xlsx_out.bytes.is_empty());
    assert!(xlsx_out.bytes.starts_with(b"PK\x03\x04"));

    let md_req = CompileDeliverableRequest {
        format: DeliverableFormat::Md,
        ..xlsx_req.clone()
    };
    let md_out = ForgeExporter::compile_deliverable(&conn, &md_req).expect("compile md");
    let md_str = String::from_utf8(md_out.bytes).expect("md utf8");
    assert!(md_str.contains("# FTO Survey Matrix"));
    assert!(md_str.contains("## Summary"));

    let pdf_req = CompileDeliverableRequest {
        format: DeliverableFormat::Pdf,
        ..xlsx_req
    };
    let pdf_out = ForgeExporter::compile_deliverable(&conn, &pdf_req).expect("compile pdf");
    assert!(pdf_out.bytes.starts_with(b"%PDF-1.4"));
    let pdf_lossy = String::from_utf8_lossy(&pdf_out.bytes);
    assert!(pdf_lossy.contains("%%EOF"));
}
