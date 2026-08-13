use crate::pipeline::context::AiCostRecord;
use super::Database;
use anyhow::Result;

impl Database {
    /// Save an AI cost ledger record.
    pub fn save_cost_record(&self, record: &AiCostRecord) -> Result<(), rusqlite::Error> {
        let c = self.conn();
        c.execute(
            "INSERT OR REPLACE INTO ai_cost_ledger (id, pipeline_run_id, step, model, provider, timestamp, input_tokens, output_tokens, estimated_cost_cents, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![record.id, record.pipeline_run_id, record.step, record.model, record.provider, record.timestamp, record.input_tokens, record.output_tokens, record.estimated_cost_cents, record.duration_ms],
        )?;
        Ok(())
    }

    /// Get cost summary by time range.
    pub fn get_cost_summary(&self, days: i64) -> Result<serde_json::Value, rusqlite::Error> {
        let c = self.conn();
        let mut total_input = 0i64;
        let mut total_output = 0i64;
        let mut total_cost = 0.0f64;
        let mut total_calls = 0i64;
        let mut by_model = serde_json::Map::new();

        {
            let mut stmt = c.prepare(
                "SELECT model, SUM(input_tokens) as in_t, SUM(output_tokens) as out_t, SUM(estimated_cost_cents) as cost, COUNT(*) as cnt
                 FROM ai_cost_ledger
                 WHERE CAST(julianday(timestamp) - julianday('now') AS INTEGER) >= ?1
                 GROUP BY model"
            )?;
            let rows = stmt.query_map(rusqlite::params![days], |row: &rusqlite::Row| {
                let model: String = row.get(0)?;
                let in_t: i64 = row.get(1)?;
                let out_t: i64 = row.get(2)?;
                let cost: f64 = row.get(3)?;
                let cnt: i64 = row.get(4)?;
                Ok((model, in_t, out_t, cost, cnt))
            })?;
            for row in rows {
                let (model, in_t, out_t, cost, cnt) = row?;
                total_input += in_t;
                total_output += out_t;
                total_cost += cost;
                total_calls += cnt;
                by_model.insert(model, serde_json::json!({
                    "input_tokens": in_t,
                    "output_tokens": out_t,
                    "cost_cents": cost,
                    "calls": cnt,
                }));
            }
        }

        Ok(serde_json::json!({
            "period_days": days,
            "total_calls": total_calls,
            "total_input_tokens": total_input,
            "total_output_tokens": total_output,
            "total_cost_cents": total_cost,
            "total_cost_dollars": (total_cost / 100.0).round() * 100.0 / 100.0,
            "by_model": serde_json::Value::Object(by_model),
        }))
    }

    /// Get recent cost records.
    pub fn get_recent_cost_records(&self, limit: i64) -> Result<Vec<AiCostRecord>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, pipeline_run_id, step, model, provider, timestamp, input_tokens, output_tokens, estimated_cost_cents, duration_ms
             FROM ai_cost_ledger
             ORDER BY timestamp DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row: &rusqlite::Row| {
            Ok(AiCostRecord {
                id: row.get(0)?,
                pipeline_run_id: row.get(1)?,
                step: row.get(2)?,
                model: row.get(3)?,
                provider: row.get(4)?,
                timestamp: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                estimated_cost_cents: row.get(8)?,
                duration_ms: row.get(9)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}
