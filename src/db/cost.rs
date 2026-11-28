//! 模块职责：AI 调用次数与 Token 消耗账本（`ai_cost_ledger`）、流式/全步骤用量记账与单日/单创意硬熔断门禁。
//! 上游调用者：`src/routes/ai.rs`、`src/routes/idea.rs`、`src/orchestrator/engine.rs`、`src/routes/settings.rs`。
//! 模块专属禁区：严禁在跨时区「今日」统计中漏掉 `'localtime'`；严禁维护冗长过期的多服务商硬编码单价表，预算管控统一以真实调用次数与 Token 硬熔断为准；严禁在持有 `self.conn()` 锁期间嵌套调用其他 `self.*` 查库方法（会导致单线程 Mutex 死锁）。

use super::Database;
use crate::pipeline::context::AiCostRecord;
use anyhow::Result;

impl Database {
    /// Save an AI cost ledger record.
    pub fn save_cost_record(&self, record: &AiCostRecord) -> Result<(), rusqlite::Error> {
        let c = self.conn();
        c.execute(
            "INSERT OR REPLACE INTO ai_cost_ledger (id, pipeline_run_id, step, model, provider, timestamp, input_tokens, output_tokens, estimated_cost_cents, duration_ms, idea_id, session_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![record.id, record.pipeline_run_id, record.step, record.model, record.provider, record.timestamp, record.input_tokens, record.output_tokens, record.estimated_cost_cents, record.duration_ms, record.idea_id, record.session_id],
        )?;
        Ok(())
    }

    /// 统计今日某 step 的 AI 调用次数（队列 #2 P1 日预算判据）。
    ///
    /// 「今日」按本地日期：timestamp（UTC 存储）与 now 都经 `date(..., 'localtime')`
    /// 转到本地时区再比较（errors.md #77——跨午夜后 UTC 日期 ≠ 本地日期，
    /// 直接比较会让凌晨 0-8 点的调用错位到昨天的预算窗口）。
    /// 预算超限时路由层记 Skipped 并走原行为——响亮降级，不阻断检索。
    pub fn count_ai_calls_today(&self, step: &str) -> Result<i64, rusqlite::Error> {
        let reader = self.open_concurrent_wal_reader();
        let guard;
        let c: &rusqlite::Connection = match reader.as_ref() {
            Some(conn) => conn,
            None => {
                guard = self.conn();
                &guard
            }
        };
        let mut stmt = c.prepare(
            "SELECT COUNT(*) FROM ai_cost_ledger WHERE step = ?1 AND date(timestamp, 'localtime') = date('now','localtime')",
        )?;
        let count: i64 = stmt.query_row(rusqlite::params![step], |row| row.get(0))?;
        Ok(count)
    }

    /// 统计今日多个 step 的 AI 调用次数之和（队列 #2 P2 日预算判据：
    /// 改写与重排共享同一编排预算，超限当日两者都跳过）。
    pub fn count_ai_calls_today_steps(&self, steps: &[&str]) -> Result<i64, rusqlite::Error> {
        if steps.is_empty() {
            return Ok(0);
        }
        let placeholders = vec!["?"; steps.len()].join(",");
        let reader = self.open_concurrent_wal_reader();
        let guard;
        let c: &rusqlite::Connection = match reader.as_ref() {
            Some(conn) => conn,
            None => {
                guard = self.conn();
                &guard
            }
        };
        let mut stmt = c.prepare(&format!(
            "SELECT COUNT(*) FROM ai_cost_ledger WHERE step IN ({placeholders}) AND date(timestamp, 'localtime') = date('now','localtime')"
        ))?;
        let params: Vec<&dyn rusqlite::ToSql> =
            steps.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let count: i64 = stmt.query_row(params.as_slice(), |row| row.get(0))?;
        Ok(count)
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
                by_model.insert(
                    model,
                    serde_json::json!({
                        "input_tokens": in_t,
                        "output_tokens": out_t,
                        "cost_cents": cost,
                        "calls": cnt,
                    }),
                );
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

    /// 从 AiClient 最近一次调用的用量信息生成并保存成本记录。
    /// 模型单价：input 0.001 / output 0.002 美元 per 1K tokens（默认估算）。
    /// Log cost from the AI client's last usage info.
    /// Default pricing: input $0.001 / output $0.002 per 1K tokens.
    #[allow(clippy::too_many_arguments)]
    pub fn save_cost_record_from_client(
        &self,
        input_tokens: i64,
        output_tokens: i64,
        model: &str,
        provider: &str,
        call_type: &str,
        idea_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let estimated_cost_cents = {
            ((input_tokens as f64 * 0.001 + output_tokens as f64 * 0.002) / 1000.0 * 100.0 * 100.0)
                .round()
                / 100.0
        }; // round to 2 decimal places
        let record = crate::pipeline::context::AiCostRecord {
            id,
            pipeline_run_id: "".to_string(),
            step: call_type.to_string(),
            model: model.to_string(),
            provider: provider.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            input_tokens,
            output_tokens,
            estimated_cost_cents,
            duration_ms: 0,
            idea_id: idea_id.map(|s| s.to_string()),
            session_id: session_id.map(|s| s.to_string()),
        };
        self.save_cost_record(&record)?;
        Ok(())
    }

    /// 记录一次 AI 调用的成本：在 AI 调用成功后立即调用一次。
    ///
    /// usage 由 [`crate::ai::AiClient::take_last_usage`] 暂存，读取后即清空。
    /// 正确性前提（当前代码库均满足）：
    /// 1. 调用点必须紧跟自己那一次 AI 调用、在下一次调用同一 client 之前落账，
    ///    否则前一次的 usage 会被覆盖丢账；
    /// 2. AiClient 实例须为单任务独占（routes 每次请求经 `ai_client()` 新建、
    ///    pipeline 每次运行经 `Orchestrator::new` 新建，均满足）；若未来改为
    ///    跨并发任务共享同一实例，last_usage 槽会互相错配，须改为随响应返回。
    /// 3. 流式（SSE）调用路径不解析 usage，无法经此方法落账（已知限制）。
    ///
    /// 服务商未返回用量（或调用失败）时静默跳过，不影响主流程。
    ///
    /// Record one AI call's cost right after a successful call. Usage is taken
    /// from the client's last-call slot and cleared on read. Correctness assumes
    /// each site logs before the next call on the same client, the client is
    /// task-owned (true today: fresh instance per request / per pipeline run),
    /// and note streaming (SSE) paths never populate usage. Missing usage is
    /// silently skipped.
    pub fn log_ai_call(
        &self,
        ai: &crate::ai::AiClient,
        call_type: &str,
        idea_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let Some(usage) = ai.take_last_usage() else {
            return Ok(());
        };
        self.save_cost_record_from_client(
            usage.input_tokens,
            usage.output_tokens,
            ai.model_name(),
            ai.provider_name(),
            call_type,
            idea_id,
            session_id,
        )
    }

    /// 记录流式（SSE）或普通 AI 调用的 Token 用量：
    /// 优先取 `ai.take_last_usage()` 精确值；若流式通道未返回 usage 段，则按实际输入/输出字符数估算 Token 落库，杜绝流式漏账。
    pub fn log_ai_stream_or_call(
        &self,
        ai: &crate::ai::AiClient,
        call_type: &str,
        prompt_chars: usize,
        completion_chars: usize,
        idea_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        if let Some(usage) = ai.take_last_usage() {
            return self.save_cost_record_from_client(
                usage.input_tokens,
                usage.output_tokens,
                ai.model_name(),
                ai.provider_name(),
                call_type,
                idea_id,
                session_id,
            );
        }
        if completion_chars == 0 {
            return Ok(());
        }
        let est_input = prompt_chars.div_ceil(2).max(1) as i64;
        let est_output = completion_chars.div_ceil(2).max(1) as i64;
        self.save_cost_record_from_client(
            est_input,
            est_output,
            ai.model_name(),
            ai.provider_name(),
            call_type,
            idea_id,
            session_id,
        )
    }

    /// 统计今日全量 AI 调用次数与总 Token（按本地日期 `'localtime'` 对齐）
    pub fn get_today_total_usage(&self) -> Result<(i64, i64, i64), rusqlite::Error> {
        let c = self.conn();
        c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
             FROM ai_cost_ledger
             WHERE date(timestamp, 'localtime') = date('now', 'localtime')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    }

    /// 统计单个创意（`idea_id`）全链路累计调用次数、Input/Output Token 及分步明细
    pub fn get_idea_token_summary(
        &self,
        idea_id: &str,
    ) -> Result<serde_json::Value, rusqlite::Error> {
        let (total_calls, in_tokens, out_tokens, steps) = {
            let c = self.conn();
            let (total_calls, in_tokens, out_tokens): (i64, i64, i64) = c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
                 FROM ai_cost_ledger WHERE idea_id = ?1",
                rusqlite::params![idea_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;

            let mut stmt = c.prepare(
                "SELECT step, COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
                 FROM ai_cost_ledger
                 WHERE idea_id = ?1
                 GROUP BY step
                 ORDER BY MAX(timestamp) ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![idea_id], |row| {
                let step: String = row.get(0)?;
                let calls: i64 = row.get(1)?;
                let in_t: i64 = row.get(2)?;
                let out_t: i64 = row.get(3)?;
                Ok(serde_json::json!({
                    "step": step,
                    "calls": calls,
                    "input_tokens": in_t,
                    "output_tokens": out_t,
                    "total_tokens": in_t + out_t,
                }))
            })?;
            let mut steps = Vec::new();
            for r in rows {
                steps.push(r?);
            }
            (total_calls, in_tokens, out_tokens, steps)
        };

        let budget = self
            .get_ai_budget_status()
            .unwrap_or_else(|_| serde_json::json!({}));

        Ok(serde_json::json!({
            "status": "ok",
            "idea_id": idea_id,
            "total_calls": total_calls,
            "input_tokens": in_tokens,
            "output_tokens": out_tokens,
            "total_tokens": in_tokens + out_tokens,
            "steps": steps,
            "budget": budget,
        }))
    }

    /// 读取 AI 预算硬熔断配置与今日实时消耗
    pub fn get_ai_budget_status(&self) -> Result<serde_json::Value, rusqlite::Error> {
        let daily_max_calls = self
            .get_setting("ai_daily_max_calls")
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let daily_max_tokens = self
            .get_setting("ai_daily_max_tokens")
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let idea_max_tokens = self
            .get_setting("ai_idea_max_tokens")
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);

        let (today_calls, today_in, today_out) = self.get_today_total_usage()?;
        let today_tokens = today_in + today_out;

        Ok(serde_json::json!({
            "status": "ok",
            "daily_max_calls": daily_max_calls,
            "daily_max_tokens": daily_max_tokens,
            "idea_max_tokens": idea_max_tokens,
            "today_calls": today_calls,
            "today_input_tokens": today_in,
            "today_output_tokens": today_out,
            "today_total_tokens": today_tokens,
        }))
    }

    /// 保存 AI 预算硬熔断阈值（`0` 表示不限制）
    pub fn save_ai_budget_settings(
        &self,
        daily_max_calls: i64,
        daily_max_tokens: i64,
        idea_max_tokens: i64,
    ) -> anyhow::Result<()> {
        self.set_setting("ai_daily_max_calls", &daily_max_calls.max(0).to_string())?;
        self.set_setting("ai_daily_max_tokens", &daily_max_tokens.max(0).to_string())?;
        self.set_setting("ai_idea_max_tokens", &idea_max_tokens.max(0).to_string())?;
        Ok(())
    }

    /// 硬熔断检查：在发起高消耗 AI 调用前校验单日调用次数、单日总 Token 与单创意总 Token 是否超限。
    /// 若任一已配置阈值（`> 0`）被触达，立即返回可读阻断原因。
    pub fn check_ai_budget_circuit_breaker(&self, idea_id: Option<&str>) -> Result<(), String> {
        let budget = self
            .get_ai_budget_status()
            .map_err(|e| format!("读取预算状态失败: {e}"))?;
        let daily_max_calls = budget["daily_max_calls"].as_i64().unwrap_or(0);
        let daily_max_tokens = budget["daily_max_tokens"].as_i64().unwrap_or(0);
        let idea_max_tokens = budget["idea_max_tokens"].as_i64().unwrap_or(0);
        let today_calls = budget["today_calls"].as_i64().unwrap_or(0);
        let today_tokens = budget["today_total_tokens"].as_i64().unwrap_or(0);

        if daily_max_calls > 0 && today_calls >= daily_max_calls {
            return Err(format!(
                "【预算硬熔断】已达到今日 AI 调用次数上限（今日已调用 {today_calls} 次 / 上限 {daily_max_calls} 次），已阻断本次请求。可在设置页上调或清零阈值。"
            ));
        }
        if daily_max_tokens > 0 && today_tokens >= daily_max_tokens {
            return Err(format!(
                "【预算硬熔断】已达到今日 Token 消耗上限（今日已消耗 {today_tokens} tokens / 上限 {daily_max_tokens} tokens），已阻断本次请求。可在设置页上调或清零阈值。"
            ));
        }
        if idea_max_tokens > 0 {
            if let Some(iid) = idea_id.map(str::trim).filter(|s| !s.is_empty()) {
                if let Ok(summary) = self.get_idea_token_summary(iid) {
                    let idea_tokens = summary["total_tokens"].as_i64().unwrap_or(0);
                    if idea_tokens >= idea_max_tokens {
                        return Err(format!(
                            "【预算硬熔断】当前创意已达到单创意 Token 上限（已消耗 {idea_tokens} tokens / 上限 {idea_max_tokens} tokens），已阻断继续调用。"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// 按时间范围统计成本。
    /// Cost summary grouped by model.
    pub fn get_cost_stats(&self, days: i64) -> Result<serde_json::Value, rusqlite::Error> {
        let c = self.conn();
        let mut total_input = 0i64;
        let mut total_output = 0i64;
        let mut total_cost = 0.0f64;
        let mut total_calls = 0i64;
        let mut by_model: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        let mut by_day: Vec<(String, f64)> = Vec::new();

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
                by_model.insert(
                    model,
                    serde_json::json!({
                        "input_tokens": in_t,
                        "output_tokens": out_t,
                        "cost_cents": cost,
                        "calls": cnt,
                    }),
                );
            }
        }

        // 按天统计趋势
        {
            let mut stmt = c.prepare(
                "SELECT substr(timestamp, 1, 10) as day, SUM(estimated_cost_cents) as cost
                 FROM ai_cost_ledger
                 WHERE CAST(julianday(timestamp) - julianday('now') AS INTEGER) >= ?1
                 GROUP BY substr(timestamp, 1, 10)
                 ORDER BY day ASC",
            )?;
            let rows = stmt.query_map(rusqlite::params![days], |row: &rusqlite::Row| {
                let day: String = row.get(0)?;
                let cost: f64 = row.get(1)?;
                Ok((day, cost))
            })?;
            for (day, cost) in rows.flatten() {
                by_day.push((day, cost));
            }
        }

        let by_day_json: Vec<serde_json::Value> = by_day
            .into_iter()
            .map(|(day, cost)| serde_json::json!({"day": day, "cost_cents": cost}))
            .collect();

        Ok(serde_json::json!({
            "period_days": days,
            "total_calls": total_calls,
            "total_input_tokens": total_input,
            "total_output_tokens": total_output,
            "total_cost_cents": total_cost,
            "total_cost_dollars": (total_cost / 100.0).round() * 100.0 / 100.0,
            "by_model": serde_json::to_value(by_model).unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            "by_day": serde_json::Value::Array(by_day_json),
        }))
    }

    /// Get recent cost records.
    pub fn get_recent_cost_records(
        &self,
        limit: i64,
    ) -> Result<Vec<AiCostRecord>, rusqlite::Error> {
        let c = self.conn();
        let mut stmt = c.prepare(
            "SELECT id, pipeline_run_id, step, model, provider, timestamp, input_tokens, output_tokens, estimated_cost_cents, duration_ms, idea_id, session_id
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
                idea_id: row.get::<_, Option<String>>(10)?,
                session_id: row.get::<_, Option<String>>(11)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::ai::AiClient;

    fn test_client() -> AiClient {
        AiClient::with_config("https://api.example.com/v1", "test-key", "example-model")
    }

    #[test]
    fn take_last_usage_clears_slot_after_read() {
        let ai = test_client();
        assert!(ai.take_last_usage().is_none(), "fresh client has no usage");
        ai.set_last_usage_for_tests(100, 50);
        let usage = ai.take_last_usage().expect("usage should be present");
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert!(
            ai.take_last_usage().is_none(),
            "take must clear the slot so one call is only accounted once"
        );
    }

    #[test]
    fn log_ai_call_skips_silently_when_usage_missing() {
        let db = Database::init(":memory:").expect("in-memory db");
        let ai = test_client();
        db.log_ai_call(&ai, "ai-chat", None, None)
            .expect("missing usage must be a silent no-op");
        let records = db.get_recent_cost_records(10).expect("query records");
        assert!(
            records.is_empty(),
            "no ledger row may be written without usage"
        );
    }

    #[test]
    fn log_ai_call_writes_one_row_with_real_provider_and_clears_slot() {
        let db = Database::init(":memory:").expect("in-memory db");
        let ai = test_client();
        ai.set_last_usage_for_tests(1234, 567);
        db.log_ai_call(&ai, "idea-chat", Some("idea-1"), None)
            .expect("ledger write");
        // 读后即清：第二次落账必须无记录（锁定「每调用点各记各的」语义）
        db.log_ai_call(&ai, "idea-chat", Some("idea-1"), None)
            .expect("second call is a no-op");

        let records = db.get_recent_cost_records(10).expect("query records");
        assert_eq!(records.len(), 1, "exactly one ledger row expected");
        let r = &records[0];
        assert_eq!(r.step, "idea-chat");
        assert_eq!(r.model, "example-model");
        // 回归锁定：provider 必须是真实服务商名，不得被调用类型误填
        // （历史错账：idea-chat 曾把 "idea-chat" 写进 provider 列）
        assert_eq!(r.provider, "primary");
        assert_ne!(r.provider, r.step);
        assert_eq!(r.input_tokens, 1234);
        assert_eq!(r.output_tokens, 567);
        assert_eq!(r.idea_id.as_deref(), Some("idea-1"));
    }

    #[test]
    fn count_ai_calls_today_counts_only_matching_step_and_resets_daily() {
        let db = Database::init(":memory:").expect("in-memory db");
        let mk = |step: &str| crate::pipeline::context::AiCostRecord {
            id: uuid::Uuid::new_v4().to_string(),
            pipeline_run_id: "run".into(),
            step: step.to_string(),
            model: "fast".into(),
            provider: "configured_gateway".into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost_cents: 0.0,
            duration_ms: 1,
            idea_id: None,
            session_id: None,
        };
        db.save_cost_record(&mk("search_rewrite")).expect("save 1");
        db.save_cost_record(&mk("search_rewrite")).expect("save 2");
        db.save_cost_record(&mk("other_step")).expect("save 3");
        assert_eq!(
            db.count_ai_calls_today("search_rewrite").expect("count"),
            2,
            "只计同 step 的今日调用"
        );
        assert_eq!(db.count_ai_calls_today("other_step").expect("count"), 1);
        assert_eq!(
            db.count_ai_calls_today("nonexistent").expect("count"),
            0,
            "无记录时为 0（预算判据不炸）"
        );
    }

    #[test]
    fn budget_circuit_breaker_blocks_when_daily_or_idea_limit_exceeded() {
        let db = Database::init(":memory:").expect("in-memory db");
        let ai = test_client();

        // 默认未设上限 (0) 时全部放行
        assert!(db
            .check_ai_budget_circuit_breaker(Some("idea-budget-1"))
            .is_ok());

        // 记录一笔流式估算用量（prompt 400 字 -> 200 tokens, completion 600 字 -> 300 tokens, 共 500 tokens）
        db.log_ai_stream_or_call(&ai, "ai-chat-stream", 400, 600, Some("idea-budget-1"), None)
            .expect("log stream call");

        let summary = db
            .get_idea_token_summary("idea-budget-1")
            .expect("idea token summary");
        assert_eq!(summary["total_calls"], 1);
        assert_eq!(summary["total_tokens"], 500);

        // 设定单创意上限 450 tokens -> 必须触发硬熔断阻断
        db.save_ai_budget_settings(0, 0, 450).expect("save budget");
        let err = db
            .check_ai_budget_circuit_breaker(Some("idea-budget-1"))
            .expect_err("idea token limit must trigger circuit breaker");
        assert!(err.contains("单创意 Token 上限"));

        // 另一创意未超限应放行
        assert!(db
            .check_ai_budget_circuit_breaker(Some("idea-budget-2"))
            .is_ok());

        // 设定单日调用次数上限 1 次 -> 全局均被熔断阻断
        db.save_ai_budget_settings(1, 0, 0)
            .expect("save daily call limit");
        let daily_err = db
            .check_ai_budget_circuit_breaker(None)
            .expect_err("daily call limit must trigger circuit breaker");
        assert!(daily_err.contains("今日 AI 调用次数上限"));
    }
}
