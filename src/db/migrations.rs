use anyhow::Result;
use rusqlite::Connection;

/// Check if a column exists in a table (idempotency guard for ALTER TABLE).
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Run a migration in an IMMEDIATE transaction: DDL + version update commit atomically
/// (#13), and serialized across processes (#60) — `BEGIN IMMEDIATE` takes the database
/// write lock up front, so two processes starting against the same database file cannot
/// both run their "column missing" branches and race to ALTER.
///
/// The loser of the race replays statements built from its stale out-of-lock checks
/// under the lock; any `duplicate column name` error therefore means "the other
/// process already did it" and is skipped statement-by-statement (statements are split
/// on `;` — every migration in this file is plain DDL with no semicolons inside string
/// literals). The recorded version is never lowered: replaying an old migration keeps
/// whichever higher version the winner already wrote, so concurrent starters converge
/// on the latest schema without crashing.
pub fn run_migration_tx(conn: &Connection, version: i32, sql: &str) -> Result<()> {
    // BEGIN IMMEDIATE with backoff retry: under full-migration load (24 version
    // steps) the other starter can hold the write lock longer than the connection's
    // busy_timeout — SQLITE_BUSY on a concurrent start is exactly the crash class
    // this function exists to prevent (#60), so wait and retry instead of failing.
    let begin = (|| -> Result<()> {
        for attempt in 1..=12u32 {
            match conn.execute_batch("BEGIN IMMEDIATE") {
                Ok(()) => return Ok(()),
                Err(e) if format!("{e}").contains("database is locked") && attempt < 12 => {
                    std::thread::sleep(std::time::Duration::from_millis(150 * u64::from(attempt)));
                }
                Err(e) => return Err(e.into()),
            }
        }
        unreachable!("retry loop always returns")
    })();
    begin?;
    let result = (|| -> Result<()> {
        for stmt in sql.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            if let Err(e) = conn.execute_batch(stmt) {
                if format!("{e}").contains("duplicate column name") {
                    continue;
                }
                return Err(e.into());
            }
        }
        let current_db: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )?;
        if version > current_db {
            conn.execute_batch(&format!(
                "DELETE FROM schema_version; INSERT INTO schema_version (version) VALUES({});",
                version
            ))?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT")?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }
    Ok(())
}

/// Run all pending database migrations.
pub(crate) fn run(conn: &Connection, current_version: i32, target_version: i32) -> Result<()> {
    // Migration 0 → 1: Initial schema
    if current_version < 1 {
        run_migration_tx(
            conn,
            1,
            "
            CREATE TABLE IF NOT EXISTS patents (
                id TEXT PRIMARY KEY, patent_number TEXT NOT NULL, title TEXT NOT NULL,
                abstract_text TEXT, description TEXT, claims TEXT, applicant TEXT,
                inventor TEXT, filing_date TEXT, publication_date TEXT, grant_date TEXT,
                ipc_codes TEXT, cpc_codes TEXT, priority_date TEXT, country TEXT,
                kind_code TEXT, family_id TEXT, legal_status TEXT, citations TEXT,
                cited_by TEXT, source TEXT, raw_json TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_pn ON patents(patent_number);
            CREATE INDEX IF NOT EXISTS idx_applicant ON patents(applicant);
            CREATE INDEX IF NOT EXISTS idx_inventor ON patents(inventor);
            CREATE INDEX IF NOT EXISTS idx_country ON patents(country);
            CREATE INDEX IF NOT EXISTS idx_filing_date ON patents(filing_date);
            CREATE VIRTUAL TABLE IF NOT EXISTS patents_fts USING fts5(
                patent_number, title, abstract_text, claims, applicant, inventor, ipc_codes,
                content='patents', content_rowid='rowid'
            );
            CREATE TABLE IF NOT EXISTS ideas (
                id TEXT PRIMARY KEY, title TEXT NOT NULL, description TEXT NOT NULL,
                input_type TEXT DEFAULT 'text', status TEXT DEFAULT 'pending',
                analysis TEXT DEFAULT '', web_results TEXT DEFAULT '[]',
                patent_results TEXT DEFAULT '[]', novelty_score REAL,
                created_at TEXT DEFAULT (datetime('now')), updated_at TEXT DEFAULT (datetime('now'))
            );
        ",
        )?;
        tracing::info!("Database migrated to version 1");
    }

    if current_version < 2 {
        run_migration_tx(
            conn,
            2,
            "
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT DEFAULT '',
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS patent_collections (
                patent_id TEXT NOT NULL, collection_id TEXT NOT NULL,
                added_at TEXT DEFAULT (datetime('now')), PRIMARY KEY (patent_id, collection_id)
            );
            CREATE TABLE IF NOT EXISTS patent_tags (
                patent_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (patent_id, tag)
            );
            CREATE INDEX IF NOT EXISTS idx_pc_collection ON patent_collections(collection_id);
            CREATE INDEX IF NOT EXISTS idx_pt_tag ON patent_tags(tag);
        ",
        )?;
        tracing::info!("Database migrated to version 2");
    }

    if current_version < 3 {
        let mut sql = String::from(
            "CREATE TABLE IF NOT EXISTS idea_messages (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, role TEXT NOT NULL,
                content TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_im_idea ON idea_messages(idea_id);",
        );
        if !column_exists(conn, "ideas", "discussion_summary")? {
            sql.push_str("ALTER TABLE ideas ADD COLUMN discussion_summary TEXT DEFAULT '';");
        }
        run_migration_tx(conn, 3, &sql)?;
        tracing::info!("Database migrated to version 3");
    }

    if current_version < 4 {
        let mut sql = String::new();
        if !column_exists(conn, "patents", "images")? {
            sql.push_str("ALTER TABLE patents ADD COLUMN images TEXT DEFAULT '[]';");
        }
        if !column_exists(conn, "patents", "pdf_url")? {
            sql.push_str("ALTER TABLE patents ADD COLUMN pdf_url TEXT DEFAULT '';");
        }
        run_migration_tx(conn, 4, &sql)?;
        tracing::info!("Database migrated to version 4 (patent images)");
    }

    if current_version < 5 {
        run_migration_tx(
            conn,
            5,
            "CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY, value TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now'))
            );",
        )?;
        tracing::info!("Database migrated to version 5 (app_settings)");
    }

    if current_version < 6 {
        run_migration_tx(conn, 6,
            "CREATE TABLE IF NOT EXISTS feature_cards (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, title TEXT NOT NULL,
                description TEXT DEFAULT '', novelty_score REAL,
                created_at TEXT DEFAULT (datetime('now')), FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_fc_idea ON feature_cards(idea_id);
            CREATE INDEX IF NOT EXISTS idx_fc_score ON feature_cards(novelty_score);")?;
        tracing::info!("Database migrated to version 6 (feature_cards)");
    }

    if current_version < 7 {
        run_migration_tx(
            conn,
            7,
            "CREATE TABLE IF NOT EXISTS pipeline_snapshots (
                idea_id TEXT PRIMARY KEY, context_json TEXT NOT NULL,
                current_step TEXT NOT NULL, updated_at TEXT DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS search_cache (
                query_hash TEXT PRIMARY KEY, query_text TEXT NOT NULL,
                results_json TEXT NOT NULL, source TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')), expires_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cache_expires ON search_cache(expires_at);",
        )?;
        tracing::info!("Database migrated to version 7");
    }

    if current_version < 8 {
        run_migration_tx(
            conn,
            8,
            "CREATE TABLE IF NOT EXISTS evidence_chain (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, claim TEXT NOT NULL,
                source_type TEXT NOT NULL, source_id TEXT NOT NULL,
                source_title TEXT NOT NULL, source_url TEXT DEFAULT '',
                claim_number TEXT, excerpt TEXT NOT NULL,
                relation TEXT NOT NULL DEFAULT 'supports',
                confidence REAL NOT NULL DEFAULT 0.0, produced_by TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_ev_idea ON evidence_chain(idea_id);
            CREATE INDEX IF NOT EXISTS idx_ev_confidence ON evidence_chain(confidence);",
        )?;
        tracing::info!("Database migrated to version 8 (evidence_chain)");
    }

    if current_version < 9 {
        let mut sql = String::new();
        for (col, def) in [
            ("technical_problem", "TEXT DEFAULT ''"),
            ("core_structure", "TEXT DEFAULT ''"),
            ("key_relations", "TEXT DEFAULT ''"),
            ("process_steps", "TEXT DEFAULT ''"),
            ("application_scenarios", "TEXT DEFAULT ''"),
        ] {
            if !column_exists(conn, "feature_cards", col)? {
                sql.push_str(&format!(
                    "ALTER TABLE feature_cards ADD COLUMN {} {};",
                    col, def
                ));
            }
        }
        run_migration_tx(conn, 9, &sql)?;
        tracing::info!("Database migrated to version 9 (feature_card 5-dim fields)");
    }

    if current_version < 10 {
        run_migration_tx(
            conn,
            10,
            "CREATE TABLE IF NOT EXISTS idea_versions (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, version_number INTEGER NOT NULL,
                context_json TEXT NOT NULL, current_step TEXT NOT NULL,
                branch_id TEXT DEFAULT 'main', created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_iv_idea ON idea_versions(idea_id);
            CREATE INDEX IF NOT EXISTS idx_iv_branch ON idea_versions(branch_id);
            CREATE TABLE IF NOT EXISTS idea_branches (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, name TEXT NOT NULL,
                parent_branch_id TEXT DEFAULT 'main', parent_version_id TEXT,
                status TEXT DEFAULT 'active', created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_ib_idea ON idea_branches(idea_id);
            CREATE TABLE IF NOT EXISTS findings (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, finding_type TEXT NOT NULL,
                title TEXT NOT NULL, content TEXT NOT NULL, source_step TEXT DEFAULT '',
                branch_id TEXT DEFAULT 'main', created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_f_idea ON findings(idea_id);
            CREATE INDEX IF NOT EXISTS idx_f_type ON findings(finding_type);",
        )?;
        tracing::info!("Database migrated to version 10");
    }

    if current_version < 11 {
        run_migration_tx(
            conn,
            11,
            "CREATE TABLE IF NOT EXISTS claim_nodes (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, claim_number INTEGER NOT NULL,
                claim_type TEXT NOT NULL DEFAULT 'independent', parent_claim_id TEXT,
                content TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_cn_idea ON claim_nodes(idea_id);
            CREATE TABLE IF NOT EXISTS technical_features (
                id TEXT PRIMARY KEY, claim_id TEXT NOT NULL, description TEXT NOT NULL,
                novelty_flag INTEGER NOT NULL DEFAULT 0, evidence_ids TEXT DEFAULT '[]',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (claim_id) REFERENCES claim_nodes(id)
            );
            CREATE INDEX IF NOT EXISTS idx_tf_claim ON technical_features(claim_id);",
        )?;
        tracing::info!("Database migrated to version 11");
    }

    if current_version < 12 {
        run_migration_tx(
            conn,
            12,
            "CREATE TABLE IF NOT EXISTS idea_research_state (
                idea_id TEXT PRIMARY KEY, current_hypothesis TEXT DEFAULT '',
                excluded_paths TEXT DEFAULT '[]', open_questions TEXT DEFAULT '[]',
                verified_claims TEXT DEFAULT '[]', updated_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_rs_updated_at ON idea_research_state(updated_at);",
        )?;
        tracing::info!("Database migrated to version 12 (idea_research_state)");
    }

    if current_version < 13 {
        run_migration_tx(
            conn,
            13,
            "CREATE TABLE IF NOT EXISTS chat_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT, session_key TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
                content TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_chat_session ON chat_records(session_key);
            CREATE INDEX IF NOT EXISTS idx_chat_created ON chat_records(created_at);",
        )?;
        tracing::info!("Database migrated to version 13 (chat_records)");
    }

    if current_version < 14 {
        run_migration_tx(
            conn,
            14,
            "CREATE TABLE IF NOT EXISTS oa_analyses (
                id INTEGER PRIMARY KEY AUTOINCREMENT, patent_number TEXT NOT NULL,
                patent_title TEXT DEFAULT '', oa_type TEXT NOT NULL, depth TEXT NOT NULL,
                analysis_text TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now','localtime'))
            );
            CREATE INDEX IF NOT EXISTS idx_oa_patent ON oa_analyses(patent_number);
            CREATE INDEX IF NOT EXISTS idx_oa_created ON oa_analyses(created_at);",
        )?;
        tracing::info!("Database migrated to version 14 (oa_analyses)");
    }

    if current_version < 15 {
        let mut sql = String::new();
        if !column_exists(conn, "oa_analyses", "version")? {
            sql.push_str("ALTER TABLE oa_analyses ADD COLUMN version INTEGER DEFAULT 1;");
        }
        run_migration_tx(conn, 15, &sql)?;
        tracing::info!("Database migrated to version 15 (oa_analyses version)");
    }

    if current_version < 16 {
        run_migration_tx(conn, 16,
            "CREATE TABLE IF NOT EXISTS oa_discussions (
                id TEXT PRIMARY KEY, patent_number TEXT NOT NULL, applicant_name TEXT,
                oa_type TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                analysis_snapshot TEXT, discussion_history TEXT, oa_snippet TEXT,
                oa_text TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_oa_discussions_patent ON oa_discussions(patent_number);
            CREATE INDEX IF NOT EXISTS idx_oa_discussions_created ON oa_discussions(created_at DESC);")?;
        tracing::info!("Database migrated to version 16 (oa_discussions)");
    }

    if current_version < 17 {
        let mut sql = String::new();
        if !column_exists(conn, "oa_discussions", "oa_text")? {
            sql.push_str("ALTER TABLE oa_discussions ADD COLUMN oa_text TEXT NOT NULL DEFAULT '';");
        }
        run_migration_tx(conn, 17, &sql)?;
        tracing::info!("Database migrated to version 17 (oa_text column)");
    }

    if current_version < 18 {
        run_migration_tx(
            conn,
            18,
            "CREATE TABLE IF NOT EXISTS cad_artifacts (
                id TEXT PRIMARY KEY,
                context_kind TEXT NOT NULL CHECK(context_kind IN ('idea','patent','oa')),
                context_id TEXT NOT NULL, parent_artifact_id TEXT, revision INTEGER NOT NULL,
                prompt TEXT NOT NULL, assumptions_json TEXT NOT NULL DEFAULT '[]',
                preview_rel_path TEXT NOT NULL, fcstd_rel_path TEXT NOT NULL,
                step_rel_path TEXT, validation_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY(parent_artifact_id) REFERENCES cad_artifacts(id),
                UNIQUE(context_kind, context_id, revision)
            );
            CREATE INDEX IF NOT EXISTS idx_cad_artifacts_context_created
            ON cad_artifacts(context_kind, context_id, created_at DESC);",
        )?;
        tracing::info!("Database migrated to version 18 (cad_artifacts)");
    }

    if current_version < 19 {
        run_migration_tx(conn, 19,
            "CREATE TABLE IF NOT EXISTS ai_cost_ledger (
                id TEXT PRIMARY KEY, pipeline_run_id TEXT NOT NULL, step TEXT NOT NULL,
                model TEXT NOT NULL, provider TEXT NOT NULL, timestamp TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_cents REAL NOT NULL DEFAULT 0.0, duration_ms INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_ai_cost_pipeline ON ai_cost_ledger(pipeline_run_id);
            CREATE INDEX IF NOT EXISTS idx_ai_cost_timestamp ON ai_cost_ledger(timestamp);")?;
        tracing::info!("Database migrated to version 19 (ai_cost_ledger)");
    }

    if current_version < 20 {
        run_migration_tx(
            conn,
            20,
            "CREATE TABLE IF NOT EXISTS patents_embedding (
                patent_id TEXT NOT NULL UNIQUE, embedding BLOB NOT NULL,
                model_name TEXT NOT NULL DEFAULT 'doc2vec-patent', text_hash TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY(patent_id) REFERENCES patents(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_embedding_model ON patents_embedding(model_name);
            CREATE INDEX IF NOT EXISTS idx_embedding_hash ON patents_embedding(text_hash);",
        )?;
        tracing::info!("Database migrated to version 20 (patents_embedding)");
    }

    if current_version < 21 {
        run_migration_tx(
            conn,
            21,
            "CREATE TABLE IF NOT EXISTS patent_chunks (
                id TEXT PRIMARY KEY, patent_id TEXT NOT NULL, chunk_index INTEGER NOT NULL,
                source_type TEXT NOT NULL DEFAULT 'description'
                    CHECK(source_type IN ('abstract', 'claim', 'description')),
                content TEXT NOT NULL, embedding BLOB NOT NULL,
                model_name TEXT NOT NULL DEFAULT 'char-tfidf-v1',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY(patent_id) REFERENCES patents(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_chunk_patent ON patent_chunks(patent_id);
            CREATE INDEX IF NOT EXISTS idx_chunk_source ON patent_chunks(source_type);",
        )?;
        tracing::info!("Database migrated to version 21 (patent_chunks)");
    }

    if current_version < 22 {
        run_migration_tx(
            conn,
            22,
            "CREATE TABLE IF NOT EXISTS idea_memory (
                id TEXT PRIMARY KEY, idea_id TEXT NOT NULL, concept_name TEXT NOT NULL,
                concept_type TEXT NOT NULL DEFAULT 'domain_concept'
                    CHECK(concept_type IN ('domain_concept', 'decision', 'pattern', 'question')),
                content TEXT NOT NULL, confidence REAL DEFAULT 0.5, source_step TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY(idea_id) REFERENCES ideas(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_idea_memory_idea ON idea_memory(idea_id);
            CREATE INDEX IF NOT EXISTS idx_idea_memory_type ON idea_memory(concept_type);",
        )?;
        tracing::info!("Database migrated to version 22 (idea_memory)");
    }

    if current_version < 23 {
        let mut sql = String::new();
        if !column_exists(conn, "ai_cost_ledger", "idea_id")? {
            sql.push_str("ALTER TABLE ai_cost_ledger ADD COLUMN idea_id TEXT NULL;");
        }
        if !column_exists(conn, "ai_cost_ledger", "session_id")? {
            sql.push_str("ALTER TABLE ai_cost_ledger ADD COLUMN session_id TEXT NULL;");
        }
        sql.push_str("CREATE INDEX IF NOT EXISTS idx_ai_cost_idea ON ai_cost_ledger(idea_id);");
        run_migration_tx(conn, 23, &sql)?;
        tracing::info!("Database migrated to version 23 (ai_cost_idea_session)");
    }

    if current_version < 24 {
        run_migration_tx(
            conn,
            24,
            "CREATE TABLE IF NOT EXISTS oa_rounds (
                id TEXT PRIMARY KEY, patent_id TEXT NOT NULL, round_number INTEGER NOT NULL,
                round_type TEXT NOT NULL, oa_date TEXT, oa_text TEXT,
                response_date TEXT, response_text TEXT, result TEXT, result_date TEXT,
                strategy_used TEXT, quality_score REAL, notes TEXT,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_oa_rounds_patent ON oa_rounds(patent_id);
            CREATE INDEX IF NOT EXISTS idx_oa_rounds_number ON oa_rounds(patent_id, round_number);",
        )?;
        tracing::info!("Database migrated to version 24 (oa_rounds)");
    }

    if current_version > 0 && current_version < target_version {
        tracing::info!(
            "Database migrated from version {} to {}",
            current_version,
            target_version
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #60 核心负控（确定性建模竞态败者）：胜者连接已完成 ALTER，败者拿锁后
    /// 重放由锁外陈旧判据拼出的同批语句——必须幂等跳过 duplicate column，
    /// 不得以  崩溃退出（修复前形态）。
    #[test]
    fn concurrent_migration_loser_replays_stale_sql_without_crash() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("race.db");
        let conn_a = Connection::open(&db_path).unwrap();
        let conn_b = Connection::open(&db_path).unwrap();
        conn_a
            .execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version VALUES(4);
                 CREATE TABLE IF NOT EXISTS patents (id TEXT PRIMARY KEY);",
            )
            .unwrap();
        let sql = "ALTER TABLE patents ADD COLUMN technical_problem TEXT DEFAULT '';";

        // 胜者先完成同一 ALTER（跨进程竞态的胜者侧）
        conn_a.execute_batch(sql).unwrap();
        // 败者此刻才拿锁重放同批语句（其 sql 由锁外的 column_exists=false 拼出）
        run_migration_tx(&conn_b, 5, sql).unwrap();

        let v: i32 = conn_b
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(v >= 5, "版本不得低于本次迁移目标，实际 {v}");
        let cnt: i32 = conn_b
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('patents') WHERE name='technical_problem'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 1, "列必须恰好一份");
    }

    /// 竞态冒烟：两连接对同一文件库各自完整跑 0 到全部迁移，无 duplicate 崩溃、终态一致。
    #[test]
    fn concurrent_full_migration_smoke_two_connections() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir
            .path()
            .join("smoke.db")
            .into_os_string()
            .into_string()
            .unwrap();
        let db_path2 = db_path.clone();
        let db_path3 = db_path2.clone();
        // 贴近生产调用链：Database::init 先设 busy_timeout 并建 schema_version，再跑迁移。
        // busy_timeout 先行单独执行且给足等待（全量并行测试下 24 版本迁移序列可远超 5s，
        // 重负载 CI 中 5s 耗尽即 SQLITE_BUSY——这正是 #60 要消灭的崩溃形态之一）。
        let set_wait = "PRAGMA busy_timeout=30000;";
        let init = "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);";
        let h1 = std::thread::spawn(move || {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(set_wait).unwrap();
            conn.execute_batch(init).unwrap();
            run(&conn, 0, 999)
        });
        let h2 = std::thread::spawn(move || {
            let conn = Connection::open(&db_path2).unwrap();
            conn.execute_batch(set_wait).unwrap();
            conn.execute_batch(init).unwrap();
            run(&conn, 0, 999)
        });
        h1.join().unwrap().unwrap();
        h2.join().unwrap().unwrap();
        let conn = Connection::open(&db_path3).unwrap();
        let v: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(v >= 24, "两连接并发迁移后版本应到最新，实际 {v}");
    }
}
