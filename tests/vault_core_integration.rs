use innoforge::core::vault::models::{
    AssetRole, DocKind, ExperimentRecord, ExperimentStatus, FilingStatus, KnowledgeDoc,
    SearchSceneMode, SectionKind,
};
use innoforge::core::vault::VaultService;
use innoforge::db::Database;
use rusqlite::Connection;

fn setup_vault_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    VaultService::ensure_tables(&conn).expect("ensure vault tables");
    conn
}

#[test]
fn legacy_patents_migrate_to_v26_and_backfill_incrementally() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let db_path = dir.path().join("legacy.db");
    let path_str = db_path.to_string_lossy().to_string();

    {
        let conn = Connection::open(&path_str).expect("open legacy db");
        conn.execute_batch(
            "CREATE TABLE schema_version (version INTEGER NOT NULL);
             INSERT INTO schema_version (version) VALUES (24);
             CREATE TABLE patents (
                 id TEXT PRIMARY KEY,
                 patent_number TEXT NOT NULL,
                 title TEXT NOT NULL,
                 abstract_text TEXT,
                 description TEXT,
                 claims TEXT,
                 applicant TEXT,
                 inventor TEXT,
                 filing_date TEXT,
                 publication_date TEXT,
                 grant_date TEXT,
                 ipc_codes TEXT,
                 cpc_codes TEXT,
                 priority_date TEXT,
                 country TEXT,
                 kind_code TEXT,
                 family_id TEXT,
                 legal_status TEXT,
                 citations TEXT,
                 cited_by TEXT,
                 source TEXT,
                 raw_json TEXT,
                 created_at TEXT DEFAULT (datetime('now'))
             );
             INSERT INTO patents (id, patent_number, title, abstract_text, description, claims, applicant, publication_date, country)
             VALUES (
                 'legacy-p1',
                 'CN990001001A',
                 '高速涡轮轴温监测装置',
                 '一种用于监测高速涡轮轴温的传感结构。',
                 '[0001] 本发明涉及高速涡轮轴温传感器与阻尼孔冷却结构。',
                 '1. 一种高速涡轮轴温监测装置，包括传感器21与壳体10。',
                 '示例科技公司',
                 '2025-01-10',
                 'CN'
             );",
        )
        .expect("seed legacy v24 db");
    }

    let db = Database::init(&path_str).expect("migrate legacy db to v26");
    assert_eq!(db.query_schema_version().expect("schema version"), 26);
    drop(db);

    let conn = Connection::open(&path_str).expect("reopen migrated db");
    let mapped_count =
        VaultService::backfill_legacy_patents_batch(&conn, 10).expect("backfill legacy patents");
    assert_eq!(mapped_count, 1);

    let mapped_second =
        VaultService::backfill_legacy_patents_batch(&conn, 10).expect("idempotent second backfill");
    assert_eq!(mapped_second, 0);

    let doc = VaultService::get_document(&conn, "legacy-p1")
        .expect("query mapped doc")
        .expect("mapped doc exists");
    assert_eq!(doc.external_id, "CN990001001A");
    assert_eq!(doc.title_zh, "高速涡轮轴温监测装置");
    assert_eq!(doc.doc_kind, DocKind::Patent);
}

#[test]
fn cjk_bigram_bm25_and_weighted_rrf_prioritize_claims_over_background() {
    let conn = setup_vault_conn();

    let doc = KnowledgeDoc {
        doc_id: "pat-turbine-1".to_string(),
        doc_kind: DocKind::Patent,
        asset_role: AssetRole::CompetitorWall,
        external_id: "CN990001013B".to_string(),
        title_zh: "涡轮轴温与阻尼孔结构".to_string(),
        title_en: String::new(),
        authors_or_assignees: "竞品公司".to_string(),
        pub_or_filing_date: "2025-03-15".to_string(),
        venue_or_authority: "CNIPA".to_string(),
        filing_status: FilingStatus::PublicDisclosed,
        doi_verified: false,
        bibtex_raw: String::new(),
        abstract_text: "摘要：涉及涡轮机械。".to_string(),
        claims_json: serde_json::to_string(&vec![
            "1. 一种涡轮冷却组件，其特征在于包括轴温传感器21与节流阻尼孔12，所述阻尼孔12连通壳体10内腔。",
        ])
        .expect("serialize claims"),
        fulltext_raw: "\
背景技术\n\
[0001] 现有技术中涡轮轴温受阻尼孔堵塞影响较大，传统方案难以稳定监测涡轮轴温与阻尼孔压降。\n\
具体实施方式\n\
[0010] 实施例一：如图1所示，10、壳体，12-阻尼孔，传感器（21）安装于轴温测点处。"
            .to_string(),
        fulltext_status: "fulltext_ready".to_string(),
        chunk_ver: 0,
    };

    VaultService::upsert_document_with_chunks(&conn, &doc).expect("upsert doc with chunks");

    let tokens = VaultService::build_fts_tokens("轴温传感器");
    assert!(tokens.contains("轴 温 传 感 器"));
    assert!(tokens.contains("轴温 温传 传感 感器"));

    let numerals =
        VaultService::get_figure_numerals(&conn, "pat-turbine-1").expect("get figure numerals");
    assert!(numerals
        .iter()
        .any(|n| n.numeral_code == "10" && n.part_name == "壳体"));
    assert!(numerals
        .iter()
        .any(|n| n.numeral_code == "12" && n.part_name == "阻尼孔"));
    assert!(numerals
        .iter()
        .any(|n| n.numeral_code == "21" && n.part_name == "传感器"));

    let bundle = VaultService::hybrid_search_chunks(
        &conn,
        "涡轮 轴温 阻尼孔",
        SearchSceneMode::PatentCompare,
        None,
        None,
        None,
        10,
        8000,
    )
    .expect("hybrid search");

    assert!(!bundle.hits.is_empty(), "should recall matching chunks");
    assert!(
        bundle
            .hits
            .iter()
            .any(|h| h.chunk.section_kind == SectionKind::ClaimIndep),
        "ClaimIndep should be recalled"
    );

    let bg_score = bundle
        .hits
        .iter()
        .find(|h| h.chunk.section_kind == SectionKind::Background)
        .map(|h| h.rrf_score);
    let claim_score = bundle
        .hits
        .iter()
        .find(|h| h.chunk.section_kind == SectionKind::ClaimIndep)
        .map(|h| h.rrf_score);
    if let (Some(bg), Some(cl)) = (bg_score, claim_score) {
        assert!(
            cl > bg,
            "ClaimIndep RRF score ({cl}) must exceed Background ({bg})"
        );
    }

    let exact = VaultService::get_chunk_by_anchor(&conn, "pat-turbine-1", "Desc:[0010]")
        .expect("o1 anchor lookup")
        .expect("Desc:[0010] exists");
    assert_eq!(exact.section_kind, SectionKind::Embodiment);
    assert_eq!(exact.embodiment_group.as_deref(), Some("实施例一"));
}

#[test]
fn dependent_claim_hit_prepends_full_parent_claim_chain() {
    let conn = setup_vault_conn();

    let doc = KnowledgeDoc {
        doc_id: "pat-chain-1".to_string(),
        doc_kind: DocKind::Patent,
        asset_role: AssetRole::OwnAsset,
        external_id: "CN990001014A".to_string(),
        title_zh: "分级阻尼密封结构".to_string(),
        title_en: String::new(),
        authors_or_assignees: "我方".to_string(),
        pub_or_filing_date: "2026-01-01".to_string(),
        venue_or_authority: "CNIPA".to_string(),
        filing_status: FilingStatus::PatentFiled,
        doi_verified: false,
        bibtex_raw: String::new(),
        abstract_text: String::new(),
        claims_json: serde_json::to_string(&vec![
            "1. 一种密封基座，包括环形基体与导流槽。",
            "2. 如权利要求1所述的密封基座，其特征在于所述导流槽内设有陶瓷衬套。",
            "3. 如权利要求2所述的密封基座，其特征在于所述陶瓷衬套表面涂覆金刚石微晶镀层。",
        ])
        .expect("serialize claims"),
        fulltext_raw: String::new(),
        fulltext_status: "claims_only".to_string(),
        chunk_ver: 0,
    };

    VaultService::upsert_document_with_chunks(&conn, &doc).expect("upsert chain doc");

    let bundle = VaultService::hybrid_search_chunks(
        &conn,
        "金刚石微晶镀层",
        SearchSceneMode::PatentCompare,
        Some(&["pat-chain-1".to_string()]),
        None,
        None,
        5,
        8000,
    )
    .expect("search dependent claim 3");

    let claim3_hit = bundle
        .hits
        .iter()
        .find(|h| h.chunk.section_tag == "Claim:3")
        .expect("Claim:3 should be hit");
    assert_eq!(claim3_hit.chunk.section_kind, SectionKind::ClaimDep);
    assert_eq!(claim3_hit.chunk.parent_claim_no, Some(2));
    assert!(claim3_hit.assembled_context.contains("[Claim:1]"));
    assert!(claim3_hit.assembled_context.contains("[Claim:2]"));
    assert!(claim3_hit.assembled_context.contains("[Claim:3]"));

    let pos1 = claim3_hit
        .assembled_context
        .find("[Claim:1]")
        .expect("pos1");
    let pos2 = claim3_hit
        .assembled_context
        .find("[Claim:2]")
        .expect("pos2");
    let pos3 = claim3_hit
        .assembled_context
        .find("[Claim:3]")
        .expect("pos3");
    assert!(pos1 < pos2 && pos2 < pos3);
}

#[test]
fn experiment_record_backfill_returns_verified_exp_anchor() {
    let conn = setup_vault_conn();

    let designed = ExperimentRecord {
        exp_id: "exp-001".to_string(),
        run_id: Some("run-alpha".to_string()),
        title: "阻尼孔压降对照实验".to_string(),
        hypothesis: "双级阻尼孔可降低轴温波动 30%".to_string(),
        control_group_spec: "单级直通孔".to_string(),
        test_group_spec: "双级阶梯阻尼孔".to_string(),
        sandbox_output_json: None,
        measured_data_json: None,
        status: ExperimentStatus::Designed,
    };
    let anchor_before =
        VaultService::save_and_verify_experiment(&conn, &designed).expect("save designed exp");
    assert!(anchor_before.is_none());

    let verified = ExperimentRecord {
        measured_data_json: Some(r#"{"temp_drop_pct": 34.2, "samples": 12}"#.to_string()),
        status: ExperimentStatus::RealDataFilled,
        ..designed
    };
    let anchor_after =
        VaultService::save_and_verify_experiment(&conn, &verified).expect("save verified exp");
    assert_eq!(anchor_after.as_deref(), Some("Exp:exp-001"));

    let fetched = VaultService::get_experiment(&conn, "exp-001")
        .expect("get exp")
        .expect("exp exists");
    assert_eq!(fetched.status, ExperimentStatus::RealDataFilled);
}
