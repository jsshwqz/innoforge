use super::Database;
use crate::patent::{CadArtifact, CadContextKind, CadValidation, Patent, SearchType};

#[test]
fn schema_v23_is_created_and_idempotent() {
    // v24: OA 轮次追踪表 idea_id/session_id 列（见 migrations.rs）
    let file = tempfile::NamedTempFile::new().expect("temp database");
    let path = file.path().to_string_lossy().to_string();
    let db = Database::init(&path).expect("initialize database to latest schema");
    assert_eq!(db.query_schema_version().expect("schema version"), 24);

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
    assert_eq!(reopened.query_schema_version().expect("schema version"), 24);

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

// ── Issue #13: Migration atomicity and self-healing tests ──────────────────────
//
// Pattern: run full migration → reset version to simulate interrupted migration
// → re-run from old version → verify self-heal (skip ALTER, update version).
//
// The run() function executes ALL migrations from current_version to SCHEMA_VERSION
// regardless of target_version (which is only for logging). So we run the full
// migration first, then reset the recorded version to simulate "DDL applied but
// version not recorded", then re-run to verify the column_exists() guards work.

/// Helper: create a fresh in-memory db with schema_version table.
fn fresh_migration_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(":memory:").unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")
        .unwrap();
    conn
}

/// Helper: read current schema version.
fn current_schema_version(conn: &rusqlite::Connection) -> i32 {
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Helper: check if a column exists.
fn has_column(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .unwrap();
    let mut rows = stmt.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let name: String = row.get(1).unwrap();
        if name == column {
            return true;
        }
    }
    false
}

/// Helper: reset the recorded schema version to simulate an interrupted migration.
fn reset_schema_version(conn: &rusqlite::Connection, version: i32) {
    conn.execute_batch(&format!(
        "DELETE FROM schema_version; INSERT INTO schema_version (version) VALUES({});",
        version
    ))
    .unwrap();
}

#[test]
fn migration_self_heal_v9_interrupted() {
    // Scenario: migration 8→9 was interrupted after ALTER TABLE but before
    // recording the version. The columns exist but version is still 8.
    let conn = fresh_migration_db();
    super::migrations::run(&conn, 0, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), Database::SCHEMA_VERSION);
    assert!(has_column(&conn, "feature_cards", "technical_problem"));

    // Simulate interrupted migration: reset version to 8
    reset_schema_version(&conn, 8);
    assert_eq!(current_schema_version(&conn), 8);

    // Run from v8 — must self-heal (skip ALTER since columns exist, just update version)
    super::migrations::run(&conn, 8, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), Database::SCHEMA_VERSION);
}

#[test]
fn migration_self_heal_v3_interrupted() {
    let conn = fresh_migration_db();
    super::migrations::run(&conn, 0, Database::SCHEMA_VERSION).unwrap();
    assert!(has_column(&conn, "ideas", "discussion_summary"));

    // Simulate interrupted migration: reset version to 2
    reset_schema_version(&conn, 2);
    assert_eq!(current_schema_version(&conn), 2);

    // Must self-heal: column already exists, skip ALTER, update version
    super::migrations::run(&conn, 2, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), Database::SCHEMA_VERSION);
}

#[test]
fn migration_self_heal_v4_interrupted() {
    let conn = fresh_migration_db();
    super::migrations::run(&conn, 0, Database::SCHEMA_VERSION).unwrap();
    assert!(has_column(&conn, "patents", "images"));

    // Simulate interrupted migration: reset version to 3
    reset_schema_version(&conn, 3);
    assert_eq!(current_schema_version(&conn), 3);

    super::migrations::run(&conn, 3, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), Database::SCHEMA_VERSION);
}

#[test]
fn migration_self_heal_v15_interrupted() {
    let conn = fresh_migration_db();
    super::migrations::run(&conn, 0, Database::SCHEMA_VERSION).unwrap();
    assert!(has_column(&conn, "oa_analyses", "version"));

    // Simulate interrupted migration: reset version to 14
    reset_schema_version(&conn, 14);
    assert_eq!(current_schema_version(&conn), 14);

    super::migrations::run(&conn, 14, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), Database::SCHEMA_VERSION);
}

#[test]
fn migration_self_heal_v23_partial_interrupt() {
    // v23 adds two columns: idea_id and session_id.
    // Simulate: both columns exist (full migration ran), but version reset to 22.
    // The migration must detect both exist and just update the version.
    let conn = fresh_migration_db();
    super::migrations::run(&conn, 0, Database::SCHEMA_VERSION).unwrap();
    assert!(has_column(&conn, "ai_cost_ledger", "idea_id"));
    assert!(has_column(&conn, "ai_cost_ledger", "session_id"));

    // Simulate interrupted migration: reset version to 22
    reset_schema_version(&conn, 22);
    assert_eq!(current_schema_version(&conn), 22);

    // Must self-heal: both columns exist, skip ALTERs, update version
    super::migrations::run(&conn, 22, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), Database::SCHEMA_VERSION);
    assert!(has_column(&conn, "ai_cost_ledger", "session_id"));
}

#[test]
fn migration_idempotent_rerun_at_full_version() {
    let conn = fresh_migration_db();
    super::migrations::run(&conn, 0, Database::SCHEMA_VERSION).unwrap();
    let v = current_schema_version(&conn);
    // Re-run from current version — must not error
    super::migrations::run(&conn, v, Database::SCHEMA_VERSION).unwrap();
    assert_eq!(current_schema_version(&conn), v);
}

// ── issue #72：两字中文术语被当人名 / 多词整句 LIKE ──────────────────────────

/// 构造一条含多个两字技术词的合成专利。
fn issue72_synthetic_patent() -> Patent {
    let mut p = sample_patent("issue72a", "相变储热装置及温控风扇", "2024-06-01");
    p.patent_number = "CNTEST00000001A".to_string();
    p.abstract_text =
        "本发明涉及一种相变储热装置，采用石蜡作为相变材料，配合温控风扇实现均热板与散热底座的高效散热。"
            .to_string();
    p.claims =
        "1.一种相变储热装置，包括散热底座和均热板，其特征在于使用石蜡作为相变材料。".to_string();
    p.applicant = "测试科技有限公司".to_string();
    p.inventor = "王明".to_string();
    p
}

/// 正向A：两字技术词（相变/石蜡/齿轮）必须命中，search_type 不得是 inventor。
#[test]
fn issue72_two_char_technical_terms_hit_as_mixed_not_inventor() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&issue72_synthetic_patent())
        .expect("insert synthetic patent");

    for term in &["相变", "石蜡", "齿轮"] {
        let (rows, total, detected) = db
            .search_smart_exact(term, None, None, None, None, 1, 10, false)
            .expect("search succeeds");
        // 齿轮不在合成专利中，但相变/石蜡必须在
        if *term != "齿轮" {
            assert!(
                total > 0 && !rows.is_empty(),
                "term `{term}` must hit (total={total})"
            );
            assert_ne!(
                detected,
                SearchType::Inventor,
                "term `{term}` must NOT be classified as inventor search"
            );
        }
        // 无论是否命中，都不得被判为 inventor 检索
        assert_ne!(
            detected,
            SearchType::Inventor,
            "two-char technical term `{term}` must not route to inventor"
        );
    }
}

/// 通用性：v2 删除姓氏路由门后，首字与常见姓重合的技术词也走 Mixed，
/// 不得因「形似人名」被收窄到 inventor-only（否则高温/金属/白色/向心 会 0 命中）。
#[test]
fn issue72_surname_leading_technical_terms_route_to_mixed() {
    let db = Database::init(":memory:").expect("init db");
    for term in &["高温", "金属", "白色", "向心"] {
        assert_eq!(
            db.detect_search_type(term),
            SearchType::Mixed,
            "term `{term}` must route to Mixed (not inventor-only)"
        );
    }
}

/// 正向B：两个 token 单独都命中的多词查询必须命中（OR 语义）。
#[test]
fn issue72_multi_token_or_query_hits() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&issue72_synthetic_patent())
        .expect("insert synthetic patent");

    for query in &["散热底座 均热板", "笔记本 均热板"] {
        let (rows, total, _detected) = db
            .search_smart_exact(query, None, None, None, None, 1, 10, false)
            .expect("search succeeds");
        assert!(
            total > 0 && !rows.is_empty(),
            "multi-token query `{query}` must hit with OR semantics (total={total})"
        );
    }
}

/// 负控1：真实姓名（张三/李四）走 Mixed 通道，仍须命中且命中 inventor 字段。
///
/// # issue #72 v2 — 路由不再独占 inventor
/// 删除姓氏门后，姓名不再路由到 Inventor-only，而走 Mixed（含 inventor 字段搜索）。
/// 本测试断言：(1) 路由为 Mixed，(2) 仍命中，(3) 命中行的 inventor 字段含该姓名
/// （即 Mixed 确实搜了 inventor 列，而非只撞 title）。对应路由层 matched_fields 对
/// Mixed 含 "inventor"。
#[test]
fn issue72_real_names_hit_via_mixed_including_inventor() {
    let db = Database::init(":memory:").expect("init db");
    let mut p1 = sample_patent("zs1", "张三的专利", "2024-01-10");
    p1.inventor = "张三".to_string();
    let mut p2 = sample_patent("ls1", "李四的专利", "2024-02-10");
    p2.inventor = "李四".to_string();
    db.insert_patent(&p1).expect("insert 张三 patent");
    db.insert_patent(&p2).expect("insert 李四 patent");

    for name in &["张三", "李四"] {
        let detected = db.detect_search_type(name);
        assert_eq!(
            detected,
            SearchType::Mixed,
            "name `{name}` must route to Mixed (not Inventor-only) after issue #72 v2"
        );
        let (rows, total, detected_type) = db
            .search_smart_exact(name, None, None, None, None, 1, 10, false)
            .expect("search succeeds");
        assert_eq!(detected_type, SearchType::Mixed);
        assert!(
            total > 0 && !rows.is_empty(),
            "name `{name}` must still hit (total={total})"
        );
        // 命中须落在 inventor 字段——证明 Mixed 确实搜了 inventor 列
        let hit_inventor = rows.iter().any(|r| r.inventor.contains(name));
        assert!(
            hit_inventor,
            "name `{name}` must match the inventor column (matched_fields 含 inventor), got inventors: {:?}",
            rows.iter().map(|r| r.inventor.as_str()).collect::<Vec<_>>()
        );
    }
}

/// 负控2：把 chars().count() 改回 q.len() ⇒ 回归测试必须变红。
///
/// 关键区分点：3 字中文姓名（如「张三丰」= 9 字节）在**字符**判据下通过
/// （3 ∈ 2..=4），但在**字节**判据下被拒（9 > 6）。若有人把 is_likely_name
/// 的边界从 chars().count() 改回 q.len()，此断言将失败——证明断言真的
/// 测在字节/字符这个点上。
///
/// # issue #72 v2 — 不再断言技术词为非姓名
/// is_likely_name 已降为 name_like 形状 hint，相变/石蜡 判为 true 是无害的（不路由），
/// 故移除旧的 `!is_likely_name("相变")` 断言，只保留字节/字符锚点。
#[test]
fn issue72_byte_vs_char_count_guard() {
    use crate::db::relevance::is_likely_name;
    // 2 字姓名：张三 = 6 bytes, 2 chars。两种判据都通过。
    assert!(
        is_likely_name("张三"),
        "is_likely_name(\"张三\") must be true"
    );
    // 3 字姓名：张三丰 = 9 bytes, 3 chars。字符判据通过，字节判据（9 > 6）拒绝。
    // 这是字节/字符区分的锚点——改回 q.len() 此行必红。
    assert!(
        is_likely_name("张三丰"),
        "3-char name (9 bytes) must pass char-count bound; if this fails, someone reverted to byte-length"
    );
}
