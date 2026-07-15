use anyhow::Result;
use rusqlite::Connection;

/// Run all pending database migrations.
pub(crate) fn run(conn: &Connection, current_version: i32, target_version: i32) -> Result<()> {
    // Migration 0 → 1: Initial schema
    if current_version < 1 {
        conn.execute_batch(
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
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL,
                input_type TEXT DEFAULT 'text',
                status TEXT DEFAULT 'pending',
                analysis TEXT DEFAULT '',
                web_results TEXT DEFAULT '[]',
                patent_results TEXT DEFAULT '[]',
                novelty_score REAL,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (1);
        ",
        )?;
        tracing::info!("Database migrated to version 1");
    }

    // Migration 1 → 2: Collections and tags
    if current_version < 2 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                created_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS patent_collections (
                patent_id TEXT NOT NULL,
                collection_id TEXT NOT NULL,
                added_at TEXT DEFAULT (datetime('now')),
                PRIMARY KEY (patent_id, collection_id)
            );

            CREATE TABLE IF NOT EXISTS patent_tags (
                patent_id TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (patent_id, tag)
            );

            CREATE INDEX IF NOT EXISTS idx_pc_collection ON patent_collections(collection_id);
            CREATE INDEX IF NOT EXISTS idx_pt_tag ON patent_tags(tag);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (2);
        ",
        )?;
        tracing::info!("Database migrated to version 2");
    }

    // Migration 2 → 3: Idea multi-round chat messages
    if current_version < 3 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS idea_messages (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );

            CREATE INDEX IF NOT EXISTS idx_im_idea ON idea_messages(idea_id);

            -- Add summary field to ideas table
            ALTER TABLE ideas ADD COLUMN discussion_summary TEXT DEFAULT '';

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (3);
        ",
        )?;
        tracing::info!("Database migrated to version 3");
    }

    if current_version < 4 {
        conn.execute_batch(
            "
            ALTER TABLE patents ADD COLUMN images TEXT DEFAULT '[]';
            ALTER TABLE patents ADD COLUMN pdf_url TEXT DEFAULT '';

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (4);
        ",
        )?;
        tracing::info!("Database migrated to version 4 (patent images)");
    }

    // Migration 4 → 5: App settings table (for Android persistence)
    if current_version < 5 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now'))
            );

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (5);
        ",
        )?;
        tracing::info!("Database migrated to version 5 (app_settings 表)");
    }

    // Migration 5 → 6: Feature cards
    if current_version < 6 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS feature_cards (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT DEFAULT '',
                novelty_score REAL,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_fc_idea ON feature_cards(idea_id);
            CREATE INDEX IF NOT EXISTS idx_fc_score ON feature_cards(novelty_score);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (6);
        ",
        )?;
        tracing::info!("Database migrated to version 6 (feature_cards)");
    }

    // Migration 6 → 7: 管道快照 + 搜索缓存 / Pipeline snapshots + search cache
    if current_version < 7 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS pipeline_snapshots (
                idea_id TEXT PRIMARY KEY,
                context_json TEXT NOT NULL,
                current_step TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS search_cache (
                query_hash TEXT PRIMARY KEY,
                query_text TEXT NOT NULL,
                results_json TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                expires_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cache_expires ON search_cache(expires_at);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (7);
        ",
        )?;
        tracing::info!("Database migrated to version 7 (pipeline_snapshots + search_cache)");
    }

    // Migration 7 → 8: 证据链 / Evidence chain
    if current_version < 8 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS evidence_chain (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                claim TEXT NOT NULL,
                source_type TEXT NOT NULL,
                source_id TEXT NOT NULL,
                source_title TEXT NOT NULL,
                source_url TEXT DEFAULT '',
                claim_number TEXT,
                excerpt TEXT NOT NULL,
                relation TEXT NOT NULL DEFAULT 'supports',
                confidence REAL NOT NULL DEFAULT 0.0,
                produced_by TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_ev_idea ON evidence_chain(idea_id);
            CREATE INDEX IF NOT EXISTS idx_ev_confidence ON evidence_chain(confidence);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (8);
        ",
        )?;
        tracing::info!("Database migrated to version 8 (evidence_chain)");
    }

    // Migration 8 → 9: Feature Card 5 维结构化字段
    if current_version < 9 {
        conn.execute_batch(
            "
            ALTER TABLE feature_cards ADD COLUMN technical_problem TEXT DEFAULT '';
            ALTER TABLE feature_cards ADD COLUMN core_structure TEXT DEFAULT '';
            ALTER TABLE feature_cards ADD COLUMN key_relations TEXT DEFAULT '';
            ALTER TABLE feature_cards ADD COLUMN process_steps TEXT DEFAULT '';
            ALTER TABLE feature_cards ADD COLUMN application_scenarios TEXT DEFAULT '';

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (9);
        ",
        )?;
        tracing::info!("Database migrated to version 9 (feature_card 5-dimension fields)");
    }

    // Migration 9 → 10: 版本管理 + 发现记忆 / Version management + Findings memory
    if current_version < 10 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS idea_versions (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                version_number INTEGER NOT NULL,
                context_json TEXT NOT NULL,
                current_step TEXT NOT NULL,
                branch_id TEXT DEFAULT 'main',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_iv_idea ON idea_versions(idea_id);
            CREATE INDEX IF NOT EXISTS idx_iv_branch ON idea_versions(branch_id);

            CREATE TABLE IF NOT EXISTS idea_branches (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                name TEXT NOT NULL,
                parent_branch_id TEXT DEFAULT 'main',
                parent_version_id TEXT,
                status TEXT DEFAULT 'active',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_ib_idea ON idea_branches(idea_id);

            CREATE TABLE IF NOT EXISTS findings (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                finding_type TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                source_step TEXT DEFAULT '',
                branch_id TEXT DEFAULT 'main',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_f_idea ON findings(idea_id);
            CREATE INDEX IF NOT EXISTS idx_f_type ON findings(finding_type);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (10);
        ",
        )?;
        tracing::info!(
            "Database migrated to version 10 (idea_versions + idea_branches + findings)"
        );
    }

    // Migration 10 → 11: 权利要求树 / Claim tree
    if current_version < 11 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS claim_nodes (
                id TEXT PRIMARY KEY,
                idea_id TEXT NOT NULL,
                claim_number INTEGER NOT NULL,
                claim_type TEXT NOT NULL DEFAULT 'independent',
                parent_claim_id TEXT,
                content TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_cn_idea ON claim_nodes(idea_id);

            CREATE TABLE IF NOT EXISTS technical_features (
                id TEXT PRIMARY KEY,
                claim_id TEXT NOT NULL,
                description TEXT NOT NULL,
                novelty_flag INTEGER NOT NULL DEFAULT 0,
                evidence_ids TEXT DEFAULT '[]',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (claim_id) REFERENCES claim_nodes(id)
            );
            CREATE INDEX IF NOT EXISTS idx_tf_claim ON technical_features(claim_id);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (11);
        ",
        )?;
        tracing::info!("Database migrated to version 11 (claim_nodes + technical_features)");
    }

    // Migration 11 → 12: 研发状态机持久化 / Persistent research state
    if current_version < 12 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS idea_research_state (
                idea_id TEXT PRIMARY KEY,
                current_hypothesis TEXT DEFAULT '',
                excluded_paths TEXT DEFAULT '[]',
                open_questions TEXT DEFAULT '[]',
                verified_claims TEXT DEFAULT '[]',
                updated_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (idea_id) REFERENCES ideas(id)
            );
            CREATE INDEX IF NOT EXISTS idx_rs_updated_at ON idea_research_state(updated_at);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (12);
        ",
        )?;
        tracing::info!("Database migrated to version 12 (idea_research_state)");
    }

    // Migration 12 → 13: 聊天记录跨设备同步 / Chat records for cross-device sync
    if current_version < 13 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS chat_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_key TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
                content TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_chat_session ON chat_records(session_key);
            CREATE INDEX IF NOT EXISTS idx_chat_created ON chat_records(created_at);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (13);
        ",
        )?;
        tracing::info!("Database migrated to version 13 (chat_records)");
    }

    if current_version < 14 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS oa_analyses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                patent_number TEXT NOT NULL,
                patent_title TEXT DEFAULT '',
                oa_type TEXT NOT NULL,
                depth TEXT NOT NULL,
                analysis_text TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now','localtime'))
            );
            CREATE INDEX IF NOT EXISTS idx_oa_patent ON oa_analyses(patent_number);
            CREATE INDEX IF NOT EXISTS idx_oa_created ON oa_analyses(created_at);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (14);
        ",
        )?;
        tracing::info!("Database migrated to version 14 (oa_analyses)");
    }

    if current_version < 15 {
        conn.execute_batch(
            "
            ALTER TABLE oa_analyses ADD COLUMN version INTEGER DEFAULT 1;

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (15);
        ",
        )?;
        tracing::info!("Database migrated to version 15 (oa_analyses version)");
    }

    if current_version < 16 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS oa_discussions (
                id TEXT PRIMARY KEY,
                patent_number TEXT NOT NULL,
                applicant_name TEXT,
                oa_type TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                analysis_snapshot TEXT,
                discussion_history TEXT,
                oa_snippet TEXT,
                oa_text TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_oa_discussions_patent ON oa_discussions(patent_number);
            CREATE INDEX IF NOT EXISTS idx_oa_discussions_created ON oa_discussions(created_at DESC);

            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (16);
        ",
        )?;
        tracing::info!("Database migrated to version 16 (oa_discussions)");
    }

    // v17: 为 oa_discussions 表增加 oa_text 字段（用于导入讨论后直接生成答复书）
    if current_version < 17 {
        let mut columns = conn.prepare("PRAGMA table_info(oa_discussions)")?;
        let mut rows = columns.query([])?;
        let mut has_oa_text = false;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == "oa_text" {
                has_oa_text = true;
                break;
            }
        }
        if !has_oa_text {
            conn.execute_batch(
                "ALTER TABLE oa_discussions ADD COLUMN oa_text TEXT NOT NULL DEFAULT '';",
            )?;
        }
        conn.execute_batch(
            "
            DELETE FROM schema_version;
            INSERT INTO schema_version (version) VALUES (17);
            ",
        )?;
        tracing::info!("Database migrated to version 17 (oa_text column)");
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
