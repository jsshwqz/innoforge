use crate::pipeline::context::ResearchState;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};

impl super::Database {
    /// 保存/更新创意的研发状态机快照（独立于 pipeline snapshot）
    pub fn upsert_research_state(&self, idea_id: &str, state: &ResearchState) -> Result<()> {
        let c = self.conn();
        let excluded_paths = serde_json::to_string(&state.excluded_paths)?;
        let open_questions = serde_json::to_string(&state.open_questions)?;
        let verified_claims = serde_json::to_string(&state.verified_claims)?;

        c.execute(
            "INSERT INTO idea_research_state (idea_id, current_hypothesis, excluded_paths, open_questions, verified_claims, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(idea_id) DO UPDATE SET
               current_hypothesis=excluded.current_hypothesis,
               excluded_paths=excluded.excluded_paths,
               open_questions=excluded.open_questions,
               verified_claims=excluded.verified_claims,
               updated_at=datetime('now')",
            params![
                idea_id,
                state.current_hypothesis,
                excluded_paths,
                open_questions,
                verified_claims
            ],
        )?;
        Ok(())
    }

    /// 读取创意的研发状态机快照
    pub fn get_research_state(&self, idea_id: &str) -> Result<Option<ResearchState>> {
        let c = self.conn();
        let row: Option<(String, String, String, String)> = c
            .query_row(
                "SELECT COALESCE(current_hypothesis,''), COALESCE(excluded_paths,'[]'),
                        COALESCE(open_questions,'[]'), COALESCE(verified_claims,'[]')
                 FROM idea_research_state WHERE idea_id = ?1",
                params![idea_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let state = row.map(
            |(current_hypothesis, excluded_paths, open_questions, verified_claims)| ResearchState {
                current_hypothesis,
                excluded_paths: serde_json::from_str(&excluded_paths).unwrap_or_default(),
                open_questions: serde_json::from_str(&open_questions).unwrap_or_default(),
                verified_claims: serde_json::from_str(&verified_claims).unwrap_or_default(),
            },
        );

        Ok(state)
    }

    /// 删除创意的研发状态机快照
    pub fn delete_research_state(&self, idea_id: &str) -> Result<()> {
        let c = self.conn();
        c.execute(
            "DELETE FROM idea_research_state WHERE idea_id = ?1",
            params![idea_id],
        )?;
        Ok(())
    }
}
