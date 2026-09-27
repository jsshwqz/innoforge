use super::Database;
use crate::patent::{CadArtifact, CadContextKind, CadValidation, Patent, SearchType};

#[test]
fn schema_v23_is_created_and_idempotent() {
    // v23: 成本台账扩展 idea_id/session_id 列（见 migrations.rs）
    let file = tempfile::NamedTempFile::new().expect("temp database");
    let path = file.path().to_string_lossy().to_string();
    let db = Database::init(&path).expect("initialize database to latest schema");
    assert_eq!(db.query_schema_version().expect("schema version"), 23);

    // Verify all new tables exist: cad_artifacts, ai_cost_ledger, patents_embedding, patent_chunks, idea_memory
    let cad_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cad_artifacts'",
            [],
            |row| row.get(0),
        )
        .expect("query CAD table");
    assert_eq!(cad_count, 1);

    let cost_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ai_cost_ledger'",
            [],
            |row| row.get(0),
        )
        .expect("query cost table");
    assert_eq!(cost_count, 1);

    let emb_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='patents_embedding'",
            [],
            |row| row.get(0),
        )
        .expect("query embedding table");
    assert_eq!(emb_count, 1);

    let chunk_count: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='patent_chunks'",
            [],
            |row| row.get(0),
        )
        .expect("query chunk table");
    assert_eq!(chunk_count, 1);

    drop(db);
    let reopened = Database::init(&path).expect("reopen migrated database");
    assert_eq!(reopened.query_schema_version().expect("schema version"), 23);

    // Verify idea_memory table exists
    let mem_count: i64 = reopened
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='idea_memory'",
            [],
            |row| row.get(0),
        )
        .expect("query memory table");
    assert_eq!(mem_count, 1);
}

#[test]
fn cad_artifacts_keep_full_prompts_and_increment_revisions() {
    let db = Database::init(":memory:").expect("initialize database");
    let prompt = "完整机械结构说明".repeat(500);
    for id in ["cad-1", "cad-2"] {
        db.insert_cad_artifact(&CadArtifact {
            id: id.to_string(),
            context_kind: CadContextKind::Idea,
            context_id: "idea-1".to_string(),
            parent_artifact_id: None,
            revision: 0,
            prompt: prompt.clone(),
            assumptions: vec!["默认单位为毫米".to_string()],
            preview_rel_path: format!("cad/{id}/preview.png"),
            fcstd_rel_path: format!("cad/{id}/model.FCStd"),
            step_rel_path: None,
            validation: CadValidation {
                valid: true,
                fixed: false,
                issues: Vec::new(),
            },
            created_at: String::new(),
        })
        .expect("insert artifact");
    }
    let artifacts = db
        .list_cad_artifacts(&CadContextKind::Idea, "idea-1")
        .expect("list artifacts");
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0].revision, 2);
    assert_eq!(artifacts[1].revision, 1);
    assert_eq!(artifacts[0].prompt, prompt);
}

fn sample_patent(id: &str, title: &str, filing_date: &str) -> Patent {
    Patent {
        id: id.to_string(),
        patent_number: format!("CN{}A", id.to_uppercase()),
        title: title.to_string(),
        abstract_text: format!("{title} abstract"),
        description: "description".to_string(),
        claims: "claim".to_string(),
        applicant: "Acme Corp".to_string(),
        inventor: "Alice Zhang".to_string(),
        filing_date: filing_date.to_string(),
        publication_date: filing_date.to_string(),
        grant_date: None,
        ipc_codes: "G06N".to_string(),
        cpc_codes: "G06N".to_string(),
        priority_date: filing_date.to_string(),
        country: "CN".to_string(),
        kind_code: "A".to_string(),
        family_id: None,
        legal_status: "pending".to_string(),
        citations: "[]".to_string(),
        cited_by: "[]".to_string(),
        source: "test".to_string(),
        raw_json: "{}".to_string(),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        images: "[]".to_string(),
        pdf_url: String::new(),
    }
}

#[test]
fn keyword_search_without_filters_uses_fts_path() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&sample_patent(
        "fts1",
        "Vector database patent",
        "2024-01-10",
    ))
    .expect("insert patent");

    let (rows, total, detected) = db
        .search_smart(
            "Vector",
            Some(&SearchType::Keyword),
            None,
            None,
            None,
            1,
            10,
        )
        .expect("search succeeds");

    assert_eq!(detected, SearchType::Keyword);
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Vector database patent");
    // FTS results now have BM25-based relevance scores (30-100 range)
    assert!(rows[0].relevance_score.is_some());
    let score = rows[0].relevance_score.unwrap();
    assert!(
        (30.0..=100.0).contains(&score),
        "FTS score {} out of range",
        score
    );
}

#[test]
fn keyword_search_with_date_filter_uses_filtered_search() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&sample_patent(
        "old1",
        "Vector database patent old",
        "2023-01-10",
    ))
    .expect("insert old patent");
    db.insert_patent(&sample_patent(
        "new1",
        "Vector database patent new",
        "2024-01-10",
    ))
    .expect("insert new patent");

    let (rows, total, detected) = db
        .search_smart(
            "Vector",
            Some(&SearchType::Keyword),
            None,
            Some("2024-01-01"),
            Some("2024-12-31"),
            1,
            10,
        )
        .expect("search succeeds");

    assert_eq!(detected, SearchType::Keyword);
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Vector database patent new");
    assert!(rows[0].relevance_score.is_some());
}

#[test]
fn mixed_search_routes_patent_like_query_to_patent_number_search() {
    let db = Database::init(":memory:").expect("init db");
    let mut patent = sample_patent(
        "123456789",
        "Foldable hinge dustproof structure",
        "2026-01-10",
    );
    patent.patent_number = "CN 123456789 A".to_string();
    db.insert_patent(&patent).expect("insert patent");

    let (rows, total, detected) = db
        .search_smart(
            "CN123456789A",
            Some(&SearchType::Mixed),
            None,
            None,
            None,
            1,
            10,
        )
        .expect("search succeeds");

    assert_eq!(detected, SearchType::PatentNumber);
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].patent_number, "CN 123456789 A");
}

// ── MA4b：申请人精确匹配通道（search_smart_exact，全部离线内存库）────────────────

/// 夹具：指定申请人的入库专利。
fn patent_by_applicant(id: &str, applicant: &str) -> Patent {
    let mut p = sample_patent(id, &format!("{id} 的专利"), "2024-01-10");
    p.applicant = applicant.to_string();
    p
}

/// 收集命中行的申请人（排序后便于逐字断言，与 SQL 返回顺序无关）。
fn applicants(rows: &[crate::patent::PatentSummary]) -> Vec<String> {
    let mut v: Vec<String> = rows.iter().map(|r| r.applicant.clone()).collect();
    v.sort();
    v
}

/// **MA4b 核心断言 1（中文，验收锚点形态）**：「张三」exact=true 只命中张三名下的专利，
/// 不得命中「张三丰」名下专利。
#[test]
fn applicant_exact_match_excludes_longer_name() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&patent_by_applicant("zs1", "张三"))
        .expect("insert 张三 patent");
    db.insert_patent(&patent_by_applicant("zsf1", "张三丰"))
        .expect("insert 张三丰 patent");

    let (rows, total, detected) = db
        .search_smart_exact(
            "张三",
            Some(&SearchType::Applicant),
            None,
            None,
            None,
            1,
            10,
            true,
        )
        .expect("exact applicant search succeeds");

    assert_eq!(detected, SearchType::Applicant);
    assert_eq!(total, 1);
    assert_eq!(vec!["张三".to_string()], applicants(&rows));
    // 等值通道的行仍走共用相关性打分：申请人完全等值 = 100 分
    assert_eq!(Some(100.0), rows[0].relevance_score);
}

/// **MA4b 核心断言 2（未破坏旧行为）**：同库同词，exact=false（含旧 `search_smart`
/// 兼容入口）仍是 `LIKE %词%` 模糊命中，「张三」与「张三丰」两条都出。
#[test]
fn applicant_fuzzy_match_still_includes_both_new_and_legacy_entry() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&patent_by_applicant("zs1", "张三"))
        .expect("insert 张三 patent");
    db.insert_patent(&patent_by_applicant("zsf1", "张三丰"))
        .expect("insert 张三丰 patent");

    let expected = vec!["张三".to_string(), "张三丰".to_string()];

    let (rows, total, _) = db
        .search_smart_exact(
            "张三",
            Some(&SearchType::Applicant),
            None,
            None,
            None,
            1,
            10,
            false,
        )
        .expect("fuzzy applicant search succeeds");
    assert_eq!(total, 2);
    assert_eq!(expected, applicants(&rows));

    // 旧公开入口 = exact false 的特化：行为必须与新入口 false 分支逐字一致
    let (legacy_rows, legacy_total, _) = db
        .search_smart(
            "张三",
            Some(&SearchType::Applicant),
            None,
            None,
            None,
            1,
            10,
        )
        .expect("legacy entry still fuzzy-matches");
    assert_eq!(legacy_total, 2);
    assert_eq!(applicants(&legacy_rows), applicants(&rows));
}

/// **MA4b 核心断言 3（英文机构同形态）**：`Acme Corp` 精确不吞 `Acme Corp International`，
/// 模糊两条全中。
#[test]
fn applicant_exact_vs_fuzzy_for_english_org_names() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&patent_by_applicant("ac1", "Acme Corp"))
        .expect("insert Acme Corp patent");
    db.insert_patent(&patent_by_applicant("ac2", "Acme Corp International"))
        .expect("insert Acme Corp International patent");

    let (rows, total, _) = db
        .search_smart_exact(
            "Acme Corp",
            Some(&SearchType::Applicant),
            None,
            None,
            None,
            1,
            10,
            true,
        )
        .expect("exact search succeeds");
    assert_eq!(total, 1);
    assert_eq!(vec!["Acme Corp".to_string()], applicants(&rows));

    let (fuzzy, fuzzy_total, _) = db
        .search_smart_exact(
            "Acme Corp",
            Some(&SearchType::Applicant),
            None,
            None,
            None,
            1,
            10,
            false,
        )
        .expect("fuzzy search succeeds");
    assert_eq!(fuzzy_total, 2);
    assert_eq!(
        vec![
            "Acme Corp".to_string(),
            "Acme Corp International".to_string()
        ],
        applicants(&fuzzy)
    );
}

/// **MA4b 边界（开关误用不得外溢）**：exact_assignee=true 只作用于申请人域；
/// 发明人/混合路由即使传 true 也保持旧模糊行为（防止开关语义被读成「全局精确」）。
#[test]
fn exact_flag_does_not_leak_to_other_search_types() {
    let db = Database::init(":memory:").expect("init db");
    let mut p = patent_by_applicant("iv1", "某研究院");
    p.inventor = "李雷".to_string();
    db.insert_patent(&p).expect("insert patent");
    let mut p2 = patent_by_applicant("iv2", "某研究院");
    p2.inventor = "李雷雷".to_string();
    db.insert_patent(&p2).expect("insert second patent");

    let (rows, total, detected) = db
        .search_smart_exact(
            "李雷",
            Some(&SearchType::Inventor),
            None,
            None,
            None,
            1,
            10,
            true,
        )
        .expect("inventor search with exact flag set must still work");
    assert_eq!(detected, SearchType::Inventor);
    assert_eq!(total, 2, "发明人域不消费精确开关，仍是 LIKE 模糊");
    assert_eq!(2, rows.len());
}
