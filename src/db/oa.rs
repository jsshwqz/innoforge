//! OA 答复分析持久化 / Office Action Analysis Persistence
//!
//! 保存和检索 OA 分析结果，支持历史回溯和版本管理。
//! 也保存 OA 讨论会话记录，支持流程追溯。

use anyhow::Result;
use rusqlite::params;

/// OA 分析记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OaAnalysis {
    pub id: i64,
    pub patent_number: String,
    pub patent_title: String,
    pub oa_type: String,
    pub depth: String,
    pub analysis_text: String,
    pub created_at: String,
    pub version: i32,
}

/// OA 讨论会话记录
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OaDiscussion {
    pub id: String,
    pub patent_number: String,
    pub oa_type: Option<String>,
    pub applicant_name: Option<String>,
    pub analysis_snapshot: Option<String>,
    pub oa_text: String,
    pub discussion_history: String,
    pub created_at: String,
    pub updated_at: String,
}

/// OA 轮次记录 — 追踪一件专利从一审到授权/驳回的完整流程
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OaRound {
    pub id: String,
    pub patent_id: String,
    pub round_number: i32,
    pub round_type: String,
    pub oa_date: Option<String>,
    pub oa_text: Option<String>,
    pub response_date: Option<String>,
    pub response_text: Option<String>,
    pub result: Option<String>,
    pub result_date: Option<String>,
    pub strategy_used: Option<String>,
    pub quality_score: Option<f64>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
impl super::Database {
    /// 保存 OA 分析结果（自动计算版本号）
    pub fn save_oa_analysis(
        &self,
        patent_number: &str,
        patent_title: &str,
        oa_type: &str,
        depth: &str,
        analysis_text: &str,
    ) -> Result<i64> {
        let c = self.conn();
        // 计算该专利的下一个版本号
        let next_version: i32 = c
            .query_row(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM oa_analyses WHERE patent_number = ?1",
                params![patent_number],
                |r| r.get(0),
            )
            .unwrap_or(1);
        c.execute(
            "INSERT INTO oa_analyses (patent_number, patent_title, oa_type, depth, analysis_text, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![patent_number, patent_title, oa_type, depth, analysis_text, next_version],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// 获取某专利的所有 OA 分析记录（按版本倒序）
    pub fn list_oa_analyses(&self, patent_number: &str) -> Result<Vec<OaAnalysis>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_number, patent_title, oa_type, depth, analysis_text, created_at, version \
             FROM oa_analyses WHERE patent_number = ?1 ORDER BY version DESC",
        )?;
        let rows = stmt.query_map(params![patent_number], |r| {
            Ok(OaAnalysis {
                id: r.get(0)?,
                patent_number: r.get(1)?,
                patent_title: r.get(2)?,
                oa_type: r.get(3)?,
                depth: r.get(4)?,
                analysis_text: r.get(5)?,
                created_at: r.get(6)?,
                version: r.get(7)?,
            })
        })?;
        let mut analyses = Vec::new();
        for row in rows {
            analyses.push(row?);
        }
        Ok(analyses)
    }

    /// 获取所有 OA 分析记录（按时间倒序，限最近 N 条）
    pub fn list_all_oa_analyses(&self, limit: usize) -> Result<Vec<OaAnalysis>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_number, patent_title, oa_type, depth, analysis_text, created_at, version \
             FROM oa_analyses ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(OaAnalysis {
                id: r.get(0)?,
                patent_number: r.get(1)?,
                patent_title: r.get(2)?,
                oa_type: r.get(3)?,
                depth: r.get(4)?,
                analysis_text: r.get(5)?,
                created_at: r.get(6)?,
                version: r.get(7)?,
            })
        })?;
        let mut analyses = Vec::new();
        for row in rows {
            analyses.push(row?);
        }
        Ok(analyses)
    }

    /// 按 ID 获取 OA 分析记录
    pub fn get_oa_analysis(&self, id: i64) -> Result<Option<OaAnalysis>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_number, patent_title, oa_type, depth, analysis_text, created_at, version \
             FROM oa_analyses WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(OaAnalysis {
                id: r.get(0)?,
                patent_number: r.get(1)?,
                patent_title: r.get(2)?,
                oa_type: r.get(3)?,
                depth: r.get(4)?,
                analysis_text: r.get(5)?,
                created_at: r.get(6)?,
                version: r.get(7)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    /// 删除 OA 分析记录
    pub fn delete_oa_analysis(&self, id: i64) -> Result<usize> {
        let c = self.conn();
        Ok(c.execute("DELETE FROM oa_analyses WHERE id = ?1", params![id])?)
    }

    // =========================================================
    // OA 讨论会话持久化 / OA Discussion Session Persistence
    // =========================================================

    /// 保存或更新 OA 讨论会话
    pub fn save_oa_discussion(&self, discussion: &OaDiscussion) -> Result<()> {
        let c = self.conn();
        // UPSERT: 如果 id 存在则更新，否则插入
        let exists: bool = c
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM oa_discussions WHERE id = ?1)",
                params![discussion.id],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if exists {
            c.execute(
                "UPDATE oa_discussions SET oa_text = ?2, discussion_history = ?3, updated_at = ?4 \
                 WHERE id = ?1",
                params![
                    discussion.id,
                    discussion.oa_text,
                    discussion.discussion_history,
                    discussion.updated_at
                ],
            )?;
        } else {
            c.execute(
                "INSERT INTO oa_discussions \
                 (id, patent_number, applicant_name, oa_type, analysis_snapshot, oa_text, \
                  discussion_history, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    discussion.id,
                    discussion.patent_number,
                    discussion.applicant_name,
                    discussion.oa_type,
                    discussion.analysis_snapshot,
                    discussion.oa_text,
                    discussion.discussion_history,
                    discussion.created_at,
                    discussion.updated_at,
                ],
            )?;
        }
        Ok(())
    }

    /// 获取 OA 讨论会话
    pub fn get_oa_discussion(&self, discussion_id: &str) -> Result<Option<OaDiscussion>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_number, applicant_name, oa_type, analysis_snapshot, oa_text, \
             discussion_history, created_at, updated_at \
             FROM oa_discussions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![discussion_id], |r| {
            Ok(OaDiscussion {
                id: r.get(0)?,
                patent_number: r.get(1)?,
                applicant_name: r.get(2)?,
                oa_type: r.get(3)?,
                analysis_snapshot: r.get(4)?,
                oa_text: r.get(5)?,
                discussion_history: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    /// 列出某专利的所有讨论会话（按更新时间倒序）
    pub fn list_oa_discussions(&self, patent_number: &str) -> Result<Vec<OaDiscussion>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_number, applicant_name, oa_type, analysis_snapshot, oa_text, \
             discussion_history, created_at, updated_at \
             FROM oa_discussions WHERE patent_number = ?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![patent_number], |r| {
            Ok(OaDiscussion {
                id: r.get(0)?,
                patent_number: r.get(1)?,
                applicant_name: r.get(2)?,
                oa_type: r.get(3)?,
                analysis_snapshot: r.get(4)?,
                oa_text: r.get(5)?,
                discussion_history: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        let mut discussions = Vec::new();
        for row in rows {
            discussions.push(row?);
        }
        Ok(discussions)
    }

    // =========================================================
    // OA 轮次追踪 / OA Round Tracking
    // =========================================================

    /// 创建新的 OA 轮次
    pub fn save_oa_round(&self, round: &OaRound) -> Result<()> {
        let c = self.conn();
        c.execute(
            "INSERT INTO oa_rounds \
             (id, patent_id, round_number, round_type, oa_date, oa_text, response_date, \
              response_text, result, result_date, strategy_used, quality_score, notes, \
              created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                round.id,
                round.patent_id,
                round.round_number,
                round.round_type,
                round.oa_date,
                round.oa_text,
                round.response_date,
                round.response_text,
                round.result,
                round.result_date,
                round.strategy_used,
                round.quality_score,
                round.notes,
                round.created_at,
                round.updated_at,
            ],
        )?;
        Ok(())
    }

    /// 获取某专利的所有 OA 轮次（按轮次号正序）
    pub fn list_oa_rounds(&self, patent_id: &str) -> Result<Vec<OaRound>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_id, round_number, round_type, oa_date, oa_text, \
             response_date, response_text, result, result_date, strategy_used, \
             quality_score, notes, created_at, updated_at \
             FROM oa_rounds WHERE patent_id = ?1 ORDER BY round_number ASC",
        )?;
        let rows = stmt.query_map(params![patent_id], |r| {
            Ok(OaRound {
                id: r.get(0)?,
                patent_id: r.get(1)?,
                round_number: r.get(2)?,
                round_type: r.get(3)?,
                oa_date: r.get(4)?,
                oa_text: r.get(5)?,
                response_date: r.get(6)?,
                response_text: r.get(7)?,
                result: r.get(8)?,
                result_date: r.get(9)?,
                strategy_used: r.get(10)?,
                quality_score: r.get(11)?,
                notes: r.get(12)?,
                created_at: r.get(13)?,
                updated_at: r.get(14)?,
            })
        })?;
        let mut rounds = Vec::new();
        for row in rows {
            rounds.push(row?);
        }
        Ok(rounds)
    }

    /// 按 ID 获取单个 OA 轮次
    pub fn get_oa_round(&self, id: &str) -> Result<Option<OaRound>> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, patent_id, round_number, round_type, oa_date, oa_text, \
             response_date, response_text, result, result_date, strategy_used, \
             quality_score, notes, created_at, updated_at \
             FROM oa_rounds WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |r| {
            Ok(OaRound {
                id: r.get(0)?,
                patent_id: r.get(1)?,
                round_number: r.get(2)?,
                round_type: r.get(3)?,
                oa_date: r.get(4)?,
                oa_text: r.get(5)?,
                response_date: r.get(6)?,
                response_text: r.get(7)?,
                result: r.get(8)?,
                result_date: r.get(9)?,
                strategy_used: r.get(10)?,
                quality_score: r.get(11)?,
                notes: r.get(12)?,
                created_at: r.get(13)?,
                updated_at: r.get(14)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    /// 更新 OA 轮次（部分字段）
    #[allow(clippy::too_many_arguments)]
    pub fn update_oa_round(
        &self,
        id: &str,
        oa_date: Option<&str>,
        oa_text: Option<&str>,
        response_date: Option<&str>,
        response_text: Option<&str>,
        result: Option<&str>,
        result_date: Option<&str>,
        strategy_used: Option<&str>,
        quality_score: Option<f64>,
        notes: Option<&str>,
    ) -> Result<usize> {
        let c = self.conn();
        let updated_at = chrono::Utc::now().to_rfc3339();
        Ok(c.execute(
            "UPDATE oa_rounds SET \
             oa_date = COALESCE(?2, oa_date), \
             oa_text = COALESCE(?3, oa_text), \
             response_date = COALESCE(?4, response_date), \
             response_text = COALESCE(?5, response_text), \
             result = COALESCE(?6, result), \
             result_date = COALESCE(?7, result_date), \
             strategy_used = COALESCE(?8, strategy_used), \
             quality_score = COALESCE(?9, quality_score), \
             notes = COALESCE(?10, notes), \
             updated_at = ?11 \
             WHERE id = ?1",
            params![
                id,
                oa_date,
                oa_text,
                response_date,
                response_text,
                result,
                result_date,
                strategy_used,
                quality_score,
                notes,
                updated_at
            ],
        )?)
    }

    /// 删除 OA 轮次
    pub fn delete_oa_round(&self, id: &str) -> Result<usize> {
        let c = self.conn();
        Ok(c.execute("DELETE FROM oa_rounds WHERE id = ?1", params![id])?)
    }

    /// 获取某专利的下一轮次号
    pub fn next_round_number(&self, patent_id: &str) -> Result<i32> {
        let c = self.conn();
        let next: i32 = c
            .query_row(
                "SELECT COALESCE(MAX(round_number), 0) + 1 FROM oa_rounds WHERE patent_id = ?1",
                params![patent_id],
                |r| r.get(0),
            )
            .unwrap_or(1);
        Ok(next)
    }
}
